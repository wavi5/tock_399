// Licensed under the Apache License, Version 2.0 or the MIT License.
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Copyright Tock Contributors 2022.

//! Test reception on the virtualized UART by creating two readers that
//! read in parallel. To add this test, include the line
//! ```
//!    virtual_uart_rx_test::run_virtual_uart_receive(mux_uart);
//! ```
//! to the nucleo_446re boot sequence, where `mux_uart` is a
//! `capsules_core::virtualizers::virtual_uart::MuxUart`. There is a 3-byte and a 7-byte read
//! running in parallel. Test that they are both working by typing and seeing
//! that they both get all characters. If you repeatedly type 'a', for example
//! (0x61), you should see something like:
//! ```
//! Starting receive of length 3
//! Virtual uart read complete: CommandComplete:
//! 61
//! 61
//! 61
//! 61
//! 61
//! 61
//! 61
//! Starting receive of length 7
//! Virtual uart read complete: CommandComplete:
//! 61
//! 61
//! 61
//! Starting receive of length 3
//! Virtual uart read complete: CommandComplete:
//! 61
//! 61
//! 61
//! Starting receive of length 3
//! Virtual uart read complete: CommandComplete:
//! 61
//! 61
//! 61
//! 61
//! 61
//! 61
//! 61
//! Starting receive of length 7
//! Virtual uart read complete: CommandComplete:
//! 61
//! 61
//! 61
//! ```

use capsules_core::test::virtual_uart::TestVirtualUartReceive;
use capsules_core::virtualizers::virtual_uart::{MuxUart, UartDevice};
use kernel::debug;
use kernel::hil::uart::Receive;
use kernel::static_init;

pub unsafe fn run_virtual_uart_receive(mux: &'static MuxUart<'static>) {
    debug!("Starting virtual reads.");
    let small = static_init_test_receive_small(mux);
    let large = static_init_test_receive_large(mux);
    small.run();
    large.run();
}

// *static_init_test_receive_small*
// params:
// mux: borrowed static MuxUart object
//
// 1. Creates a 3-element zero-array
// 2. Create new UartDevice from mux, write it to a static UartDevice, then borrows static UartDevice
//    - It adds the `mux` that was passed in as a constructor and sets it as the `UartDevice.mux`
//    attribute
// 3. Sets up attributes for device
//    - Adds the `UartDevice` self to `UartDevice.mux.devices[]`
// 4. creates `test`, which is just a borrowed mut `TestVirtualUartReceive` from `/test/virtual_uart.rs`
//    - A `TestVirtualUartReceive` is just going to be an implementation of a `Uart`
//    - Constructor takes in a static `UartDevice` as well as a mutable u8[] `buffer`
//    - What attributes does a `TestVirtualUartReceive` have?
//    - It's basically a type of `Uart` and just copies its traits
// 5.

unsafe fn static_init_test_receive_small(
    mux: &'static MuxUart<'static>,
) -> &'static TestVirtualUartReceive {
    static mut SMALL: [u8; 3] = [0; 3]; // Create a 3-element zero-array
    let device =
        static_init!(UartDevice<'static>,
            UartDevice::new(mux, true));

    device.setup();
    let test = static_init!(
        TestVirtualUartReceive,
        TestVirtualUartReceive::new(device, &mut SMALL)
    );
    device.set_receive_client(test);
    return test;
}

unsafe fn static_init_test_receive_large(
    mux: &'static MuxUart<'static>,
) -> &'static TestVirtualUartReceive {
    static mut BUFFER: [u8; 7] = [0; 7];
    let device = static_init!(UartDevice<'static>, UartDevice::new(mux, true));
    device.setup();
    let test = static_init!(
        TestVirtualUartReceive,
        TestVirtualUartReceive::new(device, &mut BUFFER)
    );
    device.set_receive_client(test);
    return test;
}
