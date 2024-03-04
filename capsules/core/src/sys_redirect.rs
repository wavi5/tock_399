// Licensed under the Apache License, Version 2.0 or the MIT License.
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Copyright Tock Contributors 2022.

//! Syscall Interface
//! -----------------
//!
//! - Stability: 2 - Stable
//!

use kernel::syscall::{CommandReturn, SyscallDriver};
use kernel::{ErrorCode, ProcessId};
use kernel::debug;

/// Syscall driver number.
use crate::driver;
pub const DRIVER_NUM: usize = driver::NUM::SysRedirect as usize;
pub const MAX_DRIVERS: usize = 2;
pub const MARY_AGE: usize = 20;

/// Implements a `Driver` interface.
pub struct SysRedirect {
    drivers_list: [usize; MAX_DRIVERS]
}

impl SysRedirect {
    // New function to create a new instance of SysRedirect
    pub fn new() -> Self {
        Self {
            drivers_list: [0x80000000; MAX_DRIVERS]
        }
    }
    
    // new function to check if the syscall being redirected is in the list
    pub fn validate_sys(&self, redirected_sys_num: usize) -> bool {
        for x in self.drivers_list {
            if x == redirected_sys_num {
                return true;
            }
        }
        false
    }
}

impl SyscallDriver for SysRedirect {
    ///
    /// ### `command_num`
    ///

    // add if statement that checks if the second element of the tuple is not none --> 
    // if so, call f(Some(whatever_driver)) directly?

    fn command(&self, command_num: usize, data: usize, _: usize, _: ProcessId) -> CommandReturn {
        match command_num {
            0 => {
                debug!("Driver number {:X} got command 0", data);
                CommandReturn::success_u32(MARY_AGE as u32)
            },

            1 => {
                if data != MARY_AGE {
                    CommandReturn::failure(ErrorCode::INVAL) /* wrong age */
                } else {
                    debug!("Driver number {:X} got command 1", data);
                    CommandReturn::success()
                }
            },

            // default
            _ => CommandReturn::failure(ErrorCode::NOSUPPORT),
        }
    }

    fn allocate_grant(&self, _processid: ProcessId) -> Result<(), kernel::process::Error> {
        Ok(())
    }
}

// NOTES //

// Figure out how to make it work so you get a driver number. 
// Easy but janky solution: just save the driver number and pass it to command.
// Harder: save the driver in a tuple in drivers_list?

// Have a debug in this sys_redirect.rs file in command that says "driver_num got command x"

// Your application tells LED to blink LED through external --> sys_redirect receives it 
// --> prints debug saying its recieved it --> LED blinks (i.e. the application functions normally despite our stuff in the middle)
// You will have to make your own mechanism, or trick the kernel into thinking it is an application that wanted a 
// system call

// This capsule already does interception, now you need to figure out dispatch
// Learn how the system currently does system calls

// This capsule might turn into part of the kernel instead
