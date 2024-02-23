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
                        // TODO: Figure out what to do about processid here ^
                        None => CommandReturn::failure(ErrorCode::NODEVICE),
                    };
                    let res = SyscallReturn::from_command_return(cres);

                    process.set_syscall_return_value(res); // TODO: Figure out what to do about process here
                });
        }
    }
}
