// Licensed under the Apache License, Version 2.0 or the MIT License.
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Copyright Tock Contributors 2022.

//! Virtualize a UART bus.
//!
//! This allows multiple Tock capsules to use the same UART bus. This is likely
//! most useful for `printf()` like applications where multiple things want to
//! write to the same UART channel.
//!
//! Clients can choose if they want to receive. Incoming messages will be sent
//! to all clients that have enabled receiving.
//!
//! `MuxUart` provides shared access to a single UART bus for multiple users.
//! `UartDevice` provides access for a single client.
//!
//! Usage
//! -----
//!
//! ```rust
//! # use kernel::{hil, static_init};
//! # use capsules::virtual_uart::{MuxUart, UartDevice};
//!
//! // Create a shared UART channel for the console and for kernel debug.
//! let uart_mux = static_init!(
//!     MuxUart<'static>,
// !     MuxUart::new(&sam4l::usart::USART0, &mut capsules::virtual_uart::RX_BUF)
//! );
//! hil::uart::UART::set_receive_client(&sam4l::usart::USART0, uart_mux);
//! hil::uart::UART::set_transmit_client(&sam4l::usart::USART0, uart_mux);
//!
//! // Create a UartDevice for the console.
//! let console_uart = static_init!(UartDevice, UartDevice::new(uart_mux, true));
//! console_uart.setup(); // This is important!
//! let console = static_init!(
//!     capsules::console::Console<'static>,
//!     capsules::console::Console::new(
//!         console_uart,
//!         &mut capsules::console::WRITE_BUF,
//!         &mut capsules::console::READ_BUF,
//!         board_kernel.create_grant(&grant_cap)
//!     )
//! );
//! hil::uart::UART::set_transmit_client(console_uart, console);
//! hil::uart::UART::set_receive_client(console_uart, console);
//! ```

use core::cell::Cell;
use core::cmp;

use kernel::collections::list::{List, ListLink, ListNode};
use kernel::deferred_call::{DeferredCall, DeferredCallClient};
use kernel::hil::gpio;
use kernel::hil::uart;
use kernel::utilities::cells::{OptionalCell, TakeCell};
use kernel::ErrorCode;

pub const RX_BUF_LEN: usize = 64;

pub struct UartCapsule<'a> {
    uart: &'a dyn uart::Uart<'a>,
    tx_buffer: TakeCell<'static, [u8]>,
    rx_buffer: TakeCell<'static, [u8]>,
    tx_in_progress: Cell<bool>,
    rx_in_progress: Cell<bool>,
    // tx_ready: &'a dyn kernel::hil::gpio::Pin,
    // rx_ready: &'a dyn kernel::hil::gpio::Pin,
}

impl<'a> UartCapsule<'a> {
    pub fn new(
        uart: &'a dyn uart::Uart<'a>,
        tx_buffer: &'static mut [u8],
        rx_buffer: &'static mut [u8],
        tx_in_progress: Cell<bool>,
        rx_in_progress: Cell<bool>,
        // tx_ready: &'a dyn kernel::hil::gpio::Pin,
        // rx_ready: &'a dyn kernel::hil::gpio::Pin,
    ) -> UartCapsule<'a> {
        //
        UartCapsule {
            uart: uart,
            tx_buffer: TakeCell::new(tx_buffer),
            rx_buffer: TakeCell::new(rx_buffer),
            tx_in_progress: Cell::new(false),
            rx_in_progress: Cell::new(false),
            // tx_ready: tx_ready,
            // rx_ready: rx_ready,
        }
    }

    pub fn init(&self) {
        let _ = self.uart.configure(uart::Parameters {
            baud_rate: 115200,
            width: uart::Width::Eight,
            stop_bits: uart::StopBits::One,
            parity: uart::Parity::None,
            hw_flow_control: false,
        });
    }

    //
    // UartCapsule.send()
    // This is a fn that just handles sending bytes (essentially just
    // a wrapper for transmit_buffer)
    pub fn send(&self, buffer: &'static mut [u8], rval: Result<(), ErrorCode>) {
        /*
         * params:
         * buffer: the buffer that is being sent
         * tx_len: the length of the buffer that is being sent
         * rval: the return value that is being returned from the function
         */

        self.tx_buffer.replace(buffer);
        let buf = self.tx_buffer.take().unwrap();
        let _len = buf.len();

        // Transmit the buffer
        let _ = self.uart.transmit_buffer(buf, _len);
    }
}

//
// TransmitClient for UartCapsule
impl<'a> uart::TransmitClient for UartCapsule<'a> {
    //
    // TransmitClient.transmitted_buffer()
    //  This is called whenever `UartCapsule.send()` finishes
    fn transmitted_buffer(
        &self,
        buffer: &'static mut [u8],
        tx_len: usize,
        rval: Result<(), ErrorCode>,
    ) {
        // First step is to always replace the `self.tx_buffer` with `buffer`
        self.tx_buffer.replace(buffer);
    }

    //
    // TransmitClient.transmitted_word()
    fn transmitted_word(&self, _rval: Result<(), ErrorCode>) {}
}

impl<'a> uart::ReceiveClient for UartCapsule<'a> {
    fn received_buffer(
        &self,
        buffer: &'static mut [u8],
        rx_len: usize,
        rcode: Result<(), ErrorCode>,
        error: uart::Error,
    ) {
        if self.rx_buffer.is_some() {
            // Err(ErrorCode::BUSY);
        } else {
            self.rx_buffer.replace(buffer);
            // let rx_buffer = buffer.iter().take(rx_len);
            // self.rx_in_progress.take() = true;
            // set the in progress flag

            // if read is successful, call read again to make sure that you read everything
        }

        // TODO: Put stuff here
    }

    fn received_word(&self, _word: u32, _rval: Result<(), ErrorCode>, _error: uart::Error) {}
}
