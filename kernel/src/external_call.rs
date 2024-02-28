use crate::kernel;
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

// import the kernel
use crate::kernel::Kernel;

/// This bool tracks whether there are any external calls pending for service.
static mut JOB_PENDING: bool = false;

pub struct ExternalCall {
    kernel: &'static Kernel,
    processid: ProcessId,
    //TODO:: buffer
}

impl ExternalCall {
    /// Creates a new deferred call with a unique ID.
    pub fn new(kernel: &'static Kernel) -> Self {
        // SAFETY: No accesses to CTR are via an &mut, and the Tock kernel is
        // single-threaded so all accesses will occur from this thread.

        let unique_identifier = kernel.create_process_identifier();

        // Create a dummy processid //TODO: Unsure about what to put for index
        let processid = ProcessId::new(kernel, unique_identifier, 0);

        ExternalCall {
            kernel: kernel,
            processid: processid,
        }
    }

    /// Schedule a deferred callback on the client associated with this deferred call
    pub fn set() {
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

    /// Services and clears the next pending `DeferredCall`, returns which index
    /// was serviced
    pub fn service_next_pending<KR: KernelResources<C>, C: Chip>(&self, resources: &KR) {
        // SAFETY: No accesses to BITMASK/DEFCALLS are via an &mut, and the Tock kernel is
        // single-threaded so all accesses will occur from this thread.
        let job = unsafe { JOB_PENDING };
        if job {
            unsafe {
                JOB_PENDING = false;
            }

            // Dummy syscall values
            let driver_number = 2;
            let subdriver_number = 1;
            let arg0 = 1;
            let arg1 = 0;

            // Creating a syscall of type "command"
            let syscall = Syscall::Command {
                driver_number,
                subdriver_number,
                arg0,
                arg1,
            };

            handle_external_syscall::<_, _>(resources, self.processid, syscall);
        }
    }
}

// Function to handle external syscalls and process them
pub fn handle_external_syscall<KR: KernelResources<C>, C: Chip>(
    resources: &KR,
    // process: &dyn process::Process,
    processid: ProcessId,
    syscall: Syscall,
) {
    // Hook for process debugging.
    // process.debug_syscall_called(syscall); // TODO:: << Figure out what to do about process here

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
                    Some(d) => d.command(subdriver_number, arg0, arg1, processid), // TODO: << instead of process.processid(), we will try using processid
                    None => CommandReturn::failure(ErrorCode::NODEVICE),
                };

                let res = SyscallReturn::from_command_return(cres);
                // process.set_syscall_return_value(res); // TODO: << Figure out what to do about process here
            });
    }
}
