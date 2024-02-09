// Licensed under the Apache License, Version 2.0 or the MIT License.
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Copyright Tock Contributors 2022.

//! Test reception on the virtualized UART by creating two readers that
//! read in parallel. To add this test, include the line
//! ```
//!    virtual_uart_nrf_test::run_virtual_uart_receive(uart_mux);
//! ```
//! to the imix boot sequence, where `uart_mux` is a
//! `capsules_core::virtualizers::virtual_uart::MuxUart`.  There is a 3-byte and a 7-byte
//! read running in parallel. Test that they are both working by typing
//! and seeing that they both get all characters. If you repeatedly
//! type 'a', for example (0x61), you should see something like:
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

// use core::{result, error};

use capsules_core::test::virtual_uart::{TestVirtualUartReceive, TestVirtualUartTransmit};
use capsules_core::virtualizers::virtual_uart::{MuxUart, UartDevice};
use kernel::debug;
use kernel::hil::uart::{Receive, ReceiveClient, Error};
use kernel::hil::uart::Transmit;
use kernel::static_init;

pub unsafe fn run_virtual_uart_transmit(mux: &'static MuxUart<'static>) {
    debug!("Starting virtual writes.");
    let small = static_init_test_transmit_small(mux);
    let large: &TestVirtualUartTransmit = static_init_test_transmit_large(mux);
    small.run();
    large.run();
}

pub unsafe fn run_virtual_uart_receive(mux: &'static MuxUart<'static>) {
    debug!("Starting virtual reads.");
    // let small = static_init_test_receive_small(mux);
    let large = static_init_test_receive_large(mux);
    // small.run();
    large.run();
}


unsafe fn static_init_test_receive_small(
    mux: &'static MuxUart<'static>,
) -> &'static TestVirtualUartReceive {
    static mut SMALL: [u8; 3] = [0; 3];
    let device = static_init!(UartDevice<'static>, UartDevice::new(mux, true));
    device.setup();
    let test = static_init!(
        TestVirtualUartReceive,
        TestVirtualUartReceive::new(device, &mut SMALL)
    );
    device.set_receive_client(test);
    test
    // if test == Ok(()) {
    //     let error = Error::None;
    // } else {
    //     let error = result;
    // }
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
    test
    // let result = test;
    // if result == Ok(()) {
    //     let error = Error::None;
    // } else {
    //     let error = result;
    // }
    // device.received_buffer(&BUFFER, len, result, error)
}

unsafe fn static_init_test_transmit_small(
    mux: &'static MuxUart<'static>,
) -> &'static TestVirtualUartTransmit {
    static mut SMALL: [u8; 1] = [42; 1];
    let device = static_init!(UartDevice<'static>, UartDevice::new(mux, true));
    device.setup();
    let test = static_init!(
        TestVirtualUartTransmit,
        TestVirtualUartTransmit::new(device, &mut SMALL)
    );
    device.set_transmit_client(test);
    test
}

unsafe fn static_init_test_transmit_large(
    mux: &'static MuxUart<'static>,
) -> &'static TestVirtualUartTransmit {
    static mut BUFFER: [u8; 7] = [100; 7];
    let device = static_init!(UartDevice<'static>, UartDevice::new(mux, true));
    device.setup();
    let test = static_init!(
        TestVirtualUartTransmit,
        TestVirtualUartTransmit::new(device, &mut BUFFER)
    );
    device.set_transmit_client(test);
    test
}
