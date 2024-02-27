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

use crate::debug;

use crate::errorcode::ErrorCode;

use crate::platform::platform::{ProcessFault, SyscallDriverLookup, SyscallFilter};

use crate::syscall::SyscallDriver;

use crate::syscall_driver::CommandReturn;

/// This bool tracks whether there are any external calls pending for service.
static mut JOB_PENDING: bool = false;

pub struct ExternalCall {}

impl ExternalCall {
    /// Creates a new deferred call with a unique ID.
    pub fn new() -> Self {
        // SAFETY: No accesses to CTR are via an &mut, and the Tock kernel is
        // single-threaded so all accesses will occur from this thread.
        ExternalCall {}
    }

    /// Schedule a deferred callback on the client associated with this deferred call
    pub fn set(&self) {
        // SAFETY: No accesses to BITMASK are via an &mut, and the Tock kernel is
        // single-threaded so all accesses will occur from this thread.
        unsafe {
            JOB_PENDING = true;
        }
    }

    /// Returns true if any deferred calls are waiting to be serviced,
    /// false otherwise.
    pub fn has_tasks() -> bool {
        // SAFETY: No accesses to BITMASK are via an &mut, and the Tock kernel is
        // single-threaded so all accesses will occur from this thread.
        unsafe { JOB_PENDING }
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

        // Handles only the `Command` syscall
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
                        Some(d) => d.command(subdriver_number, arg0, arg1, process.processid()), // TODO: << Figure out what to do about processid here
                        None => CommandReturn::failure(ErrorCode::NODEVICE),
                    };

                    let res = SyscallReturn::from_command_return(cres);
                    process.set_syscall_return_value(res); // TODO: << Figure out what to do about process here
                });
        }
    }

    /// Services and clears the next pending `DeferredCall`, returns which index
    /// was serviced
    pub fn service_next_pending<KR: KernelResources<C>, C: Chip>(
        &self,
        resources: &KR,
        process: &dyn process::Process,
        syscall: Syscall,
    ) {
        // SAFETY: No accesses to BITMASK/DEFCALLS are via an &mut, and the Tock kernel is
        // single-threaded so all accesses will occur from this thread.
        let job = unsafe { JOB_PENDING };
        if !job {
            unsafe {
                JOB_PENDING = false;
            }
            self.handle_external_syscall::<_, _>(resources, process, syscall);
        }
    }
}
