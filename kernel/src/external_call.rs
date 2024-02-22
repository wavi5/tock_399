use crate::utilities::cells::OptionalCell;
use core::cell::Cell;
use core::marker::Copy;
use core::marker::PhantomData;

use crate::config;
use crate::platform::chip::Chip;
use crate::platform::platform::KernelResources;
use crate::process::{self, Process, ProcessId, ShortID, Task};
use crate::syscall::{ContextSwitchReason, SyscallReturn};
use crate::syscall::{Syscall, YieldCall};

use crate::capabilities;

use crate::debug;
use crate::deferred_call::DeferredCall;
use crate::errorcode::ErrorCode;
use crate::grant::{AllowRoSize, AllowRwSize, Grant, UpcallSize};
use crate::ipc;
use crate::memop;

use crate::platform::mpu::MPU;
use crate::platform::platform::ContextSwitchCallback;

use crate::platform::platform::{ProcessFault, SyscallDriverLookup, SyscallFilter};
use crate::platform::scheduler_timer::SchedulerTimer;
use crate::platform::watchdog::WatchDog;

use crate::process_checker::{self, CredentialsCheckingPolicy};
use crate::process_loading::ProcessLoadError;
use crate::scheduler::{Scheduler, SchedulingDecision};
use crate::syscall::SyscallDriver;

use crate::syscall_driver::CommandReturn;
use crate::upcall::{Upcall, UpcallId};

// This trait is not intended to be used as a trait object;
// e.g. you should not create a `&dyn DeferredCallClient`.
// The `Sized` supertrait prevents this.
/// This trait should be implemented by clients which need to
/// receive DeferredCalls
pub trait ExternalCallClient: Sized {
    fn handle_external_call(&self);
    fn register(&'static self); // This function should be implemented as
                                // `self.deferred_call.register(&self);`
}

/// This struct serves as a lightweight alternative to the use of trait objects
/// (e.g. `&dyn DeferredCall`). Using a trait object, will include a 20 byte vtable
/// per instance, but this alternative stores only the data and function pointers,
/// 8 bytes per instance.
#[derive(Copy, Clone)]
struct DynExtCallRef<'a> {
    data: *const (),
    callback: fn(*const ()),
    _lifetime: PhantomData<&'a ()>,
}

impl<'a> DynExtCallRef<'a> {
    // SAFETY: We define the callback function as being a closure which casts
    // the passed pointer to be the appropriate type (a pointer to `T`)
    // and then calls `T::handle_deferred_call()`. In practice, the closure
    // is optimized away by LLVM when the ABI of the closure and the underlying function
    // are identical, making this zero-cost, but saving us from having to trust
    // that `fn(*const ())` and `fn handle_deferred_call(&self)` will always have the same calling
    // convention for any type.
    fn new<T: ExternalCallClient>(x: &'a T) -> Self {
        Self {
            data: x as *const _ as *const (),
            callback: |p| unsafe { T::handle_external_call(&*p.cast()) },
            _lifetime: PhantomData,
        }
    }
}

impl DynExtCallRef<'_> {
    // more efficient pass by `self` if we don't have to implement `DeferredCallClient` directly
    fn handle_external_call(self) {
        (self.callback)(self.data)
    }
}

// The below constant lets us get around Rust not allowing short array initialization
// for non-default types
const EMPTY: OptionalCell<DynExtCallRef<'static>> = OptionalCell::empty();

// All 3 of the below global statics are accessed only in this file, and all accesses
// are via immutable references. Tock is single threaded, so each will only ever be
// accessed via an immutable reference from the single kernel thread.
// TODO: Once Tock decides on an approach to replace `static mut` with some sort of
// `SyncCell`, migrate all three of these to that approach
// (https://github.com/tock/tock/issues/1545)
/// Counter for the number of deferred calls that have been created, this is
/// used to track that no more than 32 deferred calls have been created.
static mut CTR: Cell<usize> = Cell::new(0);

/// This bitmask tracks which of the up to 32 existing deferred calls have been scheduled.
/// Any bit that is set in that mask indicates the deferred call with its `idx` field set
/// to the index of that bit has been scheduled and not yet serviced.
static mut BITMASK: Cell<u32> = Cell::new(0);

// This is a 256 byte array, but at least resides in .bss
/// An array that stores references to up to 32 `DeferredCall`s via the low-cost
/// `DynDefCallRef`.
static mut EXTCALLS: [OptionalCell<DynExtCallRef<'static>>; 32] = [EMPTY; 32];

pub struct ExternalCall {
    idx: usize,
}

impl ExternalCall {
    /// Creates a new deferred call with a unique ID.
    pub fn new() -> Self {
        // SAFETY: No accesses to CTR are via an &mut, and the Tock kernel is
        // single-threaded so all accesses will occur from this thread.
        let ctr = unsafe { &CTR };
        let idx = ctr.get() + 1;
        ctr.set(idx);
        ExternalCall { idx }
    }

    // To reduce monomorphization bloat, the non-generic portion of register is moved into this
    // function without generic parameters.
    #[inline(never)]
    fn register_internal_non_generic(&self, handler: DynExtCallRef<'static>) {
        // SAFETY: No accesses to DEFCALLS are via an &mut, and the Tock kernel is
        // single-threaded so all accesses will occur from this thread.
        let extcalls = unsafe { &EXTCALLS };
        if self.idx >= extcalls.len() {
            // This error will be caught by the scheduler at the beginning of the kernel loop,
            // which is much better than panicking here, before the debug writer is setup.
            // Also allows a single panic for creating too many deferred calls instead
            // of NUM_DCS panics (this function is monomorphized).
            return;
        }
        extcalls[self.idx].set(handler);
    }

    /// This function registers the passed client with this deferred call, such
    /// that calls to `DeferredCall::set()` will schedule a callback on the
    /// `handle_deferred_call()` method of the passed client.
    pub fn register<EC: ExternalCallClient>(&self, client: &'static EC) {
        let handler = DynExtCallRef::new(client);
        self.register_internal_non_generic(handler);
    }

    /// Schedule a deferred callback on the client associated with this deferred call
    pub fn set(&self) {
        // SAFETY: No accesses to BITMASK are via an &mut, and the Tock kernel is
        // single-threaded so all accesses will occur from this thread.
        let bitmask = unsafe { &BITMASK };
        bitmask.set(bitmask.get() | (1 << self.idx));
    }

    /// Check if a deferred callback has been set and not yet serviced on this deferred call.
    pub fn is_pending(&self) -> bool {
        // SAFETY: No accesses to BITMASK are via an &mut, and the Tock kernel is
        // single-threaded so all accesses will occur from this thread.
        let bitmask = unsafe { &BITMASK };
        bitmask.get() & (1 << self.idx) == 1
    }

    /// Services and clears the next pending `DeferredCall`, returns which index
    /// was serviced
    pub fn service_next_pending() -> Option<usize> {
        // SAFETY: No accesses to BITMASK/DEFCALLS are via an &mut, and the Tock kernel is
        // single-threaded so all accesses will occur from this thread.
        let bitmask = unsafe { &BITMASK };
        let extcalls = unsafe { &EXTCALLS };
        let val = bitmask.get();
        if val == 0 {
            None
        } else {
            let bit = val.trailing_zeros() as usize;
            let new_val = val & !(1 << bit);
            bitmask.set(new_val);
            extcalls[bit].map(|ec| {
                ec.handle_external_call();
                bit
            })
        }
    }

    /// Returns true if any deferred calls are waiting to be serviced,
    /// false otherwise.
    pub fn has_tasks() -> bool {
        // SAFETY: No accesses to BITMASK are via an &mut, and the Tock kernel is
        // single-threaded so all accesses will occur from this thread.
        let bitmask = unsafe { &BITMASK };
        bitmask.get() != 0
    }

    /// This function should be called at the beginning of the kernel loop
    /// to verify that deferred calls have been correctly initialized. This function
    /// verifies two things:
    /// 1. That <= `DEFCALLS.len()` deferred calls have been created, which is the
    ///    maximum this interface supports
    /// 2. That exactly as many deferred calls were registered as were created, which helps to
    ///    catch bugs if board maintainers forget to call `register()` on a created `DeferredCall`.
    /// Neither of these checks are necessary for soundness, but they are necessary for confirming
    /// that DeferredCalls will actually be delivered as expected. This function costs about 300
    /// bytes, so you can remove it if you are confident your setup will not exceed 32 deferred
    /// calls, and that all of your components register their deferred calls.
    // Ignore the clippy warning for using `.filter(|opt| opt.is_some())` since
    // we don't actually have an Option (we have an OptionalCell) and
    // IntoIterator is not implemented for OptionalCell.
    #[allow(clippy::iter_filter_is_some)]
    pub fn verify_setup() {
        // SAFETY: No accesses to CTR/DEFCALLS are via an &mut, and the Tock kernel is
        // single-threaded so all accesses will occur from this thread.
        let ctr = unsafe { &CTR };
        let extcalls = unsafe { &EXTCALLS };
        let num_external_calls = ctr.get();
        if num_external_calls >= extcalls.len()
            || extcalls.iter().filter(|opt| opt.is_some()).count() != num_external_calls
        {
            panic!(
                "ERROR: > 32 external calls, or a component forgot to register an external call."
            );
        }
    }

    // Function to handle external syscalls and process them
    pub fn handle_external_syscall<KR: KernelResources<C>, C: Chip>(
        &self,
        resources: &KR,
        process: &dyn process::Process,
        syscall: Syscall,
    ) {
        // Hook for process debugging.
        process.debug_syscall_called(syscall);

        // Enforce platform-specific syscall filtering here.
        //
        // Before continuing to handle non-yield syscalls the kernel first
        // checks if the platform wants to block that syscall for the process,
        // and if it does, sets a return value which is returned to the calling
        // process.
        //
        // Filtering a syscall (i.e. blocking the syscall from running) does not
        // cause the process to lose its timeslice. The error will be returned
        // immediately (assuming the process has not already exhausted its
        // timeslice) allowing the process to decide how to handle the error.
        if let Syscall::Command {
            driver_number,
            subdriver_number,
            arg0,
            arg1,
        } = syscall
        {
            resources
                .syscall_driver_lookup()
                .with_driver(driver_number, |driver| {
                    let cres = match driver {
                        Some(d) => d.command(subdriver_number, arg0, arg1, process.processid()),
                        None => CommandReturn::failure(ErrorCode::NODEVICE),
                    };
                    let res = SyscallReturn::from_command_return(cres);

                    process.set_syscall_return_value(res);
                });
        }
    }
}
