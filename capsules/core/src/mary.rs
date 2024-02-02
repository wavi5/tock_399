// Licensed under the Apache License, Version 2.0 or the MIT License.
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Copyright Tock Contributors 2022.

//! Syscall Interface
//! -----------------
//!
//! - Stability: 2 - Stable
//!
//! ### Command
//!
//! This capsule only uses the `command` syscall.
//!
//! #### `command_num`
//! 
//! - `0`: Returns data (allows for checking for this driver).
//!   - `data`: int.
//!   - Return: data.
//! - `1`: Returns.
//!   - `data`: Unused.
//!   - Return: success.

use kernel::syscall::{CommandReturn, SyscallDriver};
use kernel::{ErrorCode, ProcessId};

/// Syscall driver number.
use crate::driver;
pub const DRIVER_NUM: usize = driver::NUM::Mary as usize;
pub const MARY_AGE: usize = 20;

/// Implements a `Driver` interface.
pub struct MaryDriver;

impl MaryDriver {
    // New function to create a new instance of MaryDriver
    pub fn new() -> Self {
        Self
    }
}

impl SyscallDriver for MaryDriver {
    ///
    /// ### `command_num`
    ///
    /// - `0`: Returns Mary as a u32 (allows for checking for this driver).
    /// - `1`: Returns succesfully.

    fn command(&self, command_num: usize, data: usize, _: usize, _: ProcessId) -> CommandReturn {
        match command_num {
            // get data
            0 => CommandReturn::success_u32(MARY_AGE as u32),

            // return
            1 => {
                if data != MARY_AGE {
                    CommandReturn::failure(ErrorCode::INVAL) /* wrong age */
                } else {
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
