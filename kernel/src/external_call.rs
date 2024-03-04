// Redirect external syscalls

// use crate::kernel;

use crate::platform::chip::Chip;
use crate::platform::platform::KernelResources;
use crate::process::ProcessId;
use crate::syscall::Syscall;

// use crate::debug;
use crate::errorcode::ErrorCode;

use crate::platform::platform::SyscallDriverLookup;
// use crate::syscall::SyscallDriver;
use crate::syscall_driver::CommandReturn;

// import the kernel
use crate::kernel::Kernel;


// This bool tracks whether an external syscall is pending
static mut WAITING_SYS: bool = false;

//TODO: How many external syscalls can there be?
pub const MAX_DRIVERS: usize = 2;


pub struct ExternalCall {
    // kernel: &'static Kernel,
    processid: ProcessId,
    drivers_list: [usize; MAX_DRIVERS],
    //TODO: buffer
}

impl ExternalCall {
    // Creates a new external call with a unique ID.
    pub fn new(kernel: &'static Kernel) -> Self {

        let unique_identifier = kernel.create_process_identifier();

        // Create a dummy processid //TODO: Unsure about what to put for index
        let processid = ProcessId::new(kernel, unique_identifier, 0);

        ExternalCall {
            // kernel: kernel,
            processid: processid,
            drivers_list: [0x80000000; MAX_DRIVERS],
        }
    }

    // Returns true if the syscall being redirected is in the list
    pub fn driver_num_is_external(&self, driver_num: usize) -> bool {
        if driver_num >> 31 == 1 {
            for x in self.drivers_list {
                if x == driver_num {
                    return true;
                }
                else {
                    return false;
                }
            }
        }
        false
    }

    // Returns true if an external syscall is waiting to be serviced
    pub fn has_tasks() -> bool {
        unsafe { WAITING_SYS }
    }

    // Schedules an external call
    pub fn set() {
        unsafe {
            WAITING_SYS = true
        };
    }

    // Services and clears the pending external syscall
    pub fn service_pending<KR: KernelResources<C>, C: Chip>(&self, resources: &KR) {
        let job = unsafe { WAITING_SYS };

        if job {
            unsafe {
                WAITING_SYS = false;
            }
        }
        
        // Dummy syscall values
        let driver_number = 2;
        let subdriver_number = 1;
        let arg0 = 0;
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

// Function to handle external syscalls and process them
pub fn handle_external_syscall<KR: KernelResources<C>, C: Chip>(
    resources: &KR,
    processid: ProcessId,
    syscall: Syscall,
) {
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
                let _cres = match driver {
                    Some(d) => d.command(subdriver_number, arg0, arg1, processid),
                    None => CommandReturn::failure(ErrorCode::NODEVICE),
                };
                
                // let res = SyscallReturn::from_command_return(cres);
                // process.set_syscall_return_value(res); // TODO: No process.set_syscall_return_value (just save a message)
            });
    }
}