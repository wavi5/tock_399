// Licensed under the Apache License, Version 2.0 or the MIT License.
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Copyright Tock Contributors 2022.

//! Test reception on the virtualized UART: best if multiple Tests are
//! instantiated and tested in parallel.
use crate::virtualizers::virtual_uart::UartDevice;

use kernel::debug;
use kernel::hil::uart;
use kernel::hil::uart::Receive;
use kernel::hil::uart::Transmit;
use kernel::utilities::cells::TakeCell;
use kernel::ErrorCode;

pub struct TestVirtualUartReceive {
    device: &'static UartDevice<'static>,
    buffer: TakeCell<'static, [u8]>,
}
pub struct TestVirtualUartTransmit{
    device: &'static UartDevice<'static>,
    buffer: TakeCell<'static, [u8]>,
}

impl TestVirtualUartTransmit {
    pub fn new(device: &'static UartDevice<'static>, buffer: &'static mut [u8]) -> Self {
        TestVirtualUartTransmit {
            device: device,
            buffer: TakeCell::new(buffer),
        }
    }

    pub fn run(&self) {
        let buf = self.buffer.take().unwrap();
        let len = buf.len();
        debug!("Starting transmit of length {}", len);
        self.device
            .transmit_buffer(buf, len)
            .expect("Calling receive_buffer() in virtual_uart test failed");
    }
}

impl TestVirtualUartReceive {
    pub fn new(device: &'static UartDevice<'static>, buffer: &'static mut [u8]) -> Self {
        TestVirtualUartReceive {
            device: device,
            buffer: TakeCell::new(buffer),
        }
    }

    pub fn run(&self) {
        let buf = self.buffer.take().unwrap();
        let len = buf.len();
        debug!("Starting receive of length {}", len);
        self.device
            .receive_buffer(buf, len)
            .expect("Calling receive_buffer() in virtual_uart test failed");
    }
}

impl uart::ReceiveClient for TestVirtualUartReceive {
    fn received_buffer(
        &self,
        rx_buffer: &'static mut [u8],
        rx_len: usize,
        rcode: Result<(), ErrorCode>,
        _error: uart::Error,
    ) {
        debug!("Virtual uart read complete: {:?}: ", rcode);
        for i in 0..rx_len {
            debug!("{:02x} ", rx_buffer[i]);
        }
        debug!("Starting receive of length {}", rx_len);
        self.device
            .receive_buffer(rx_buffer, rx_len)
            .expect("Calling receive_buffer() in virtual_uart test failed");
    }
}

impl uart::TransmitClient for TestVirtualUartTransmit {
    fn transmitted_buffer(
        &self,
        tx_buffer: &'static mut [u8],
        tx_len: usize,
        rcode: Result<(), ErrorCode>,
    ) {
        debug!("Virtual uart write complete: {:?}: ", rcode);
        for i in 0..tx_len {
            debug!("{:02x} ", tx_buffer[i]);
        }
        debug!("Starting write of length {}", tx_len);
        self.device
            .transmit_buffer(tx_buffer, tx_len)
            .expect("Calling transmit_buffer() in virtual_uart test failed");
    }
}
