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

use crate::virtualizers::virtual_uart::UartDevice;
use core::cell::Cell;
use core::cmp;
use core::fmt::Error;
use kernel::debug;

use kernel::collections::list::{List, ListLink, ListNode};
use kernel::deferred_call::{DeferredCall, DeferredCallClient};
use kernel::hil::gpio;
use kernel::hil::uart;
use kernel::hil::uart::{Receive, Transmit};
use kernel::utilities::cells::{OptionalCell, TakeCell};
use kernel::ErrorCode;

pub const RX_BUF_LEN: usize = 64;

pub struct UartCapsule {
    device: &'static UartDevice<'static>,
    tx_buffer: TakeCell<'static, [u8]>,
    rx_buffer: TakeCell<'static, [u8]>,
    // tx_in_progress: Cell<bool>,
    // rx_in_progress: Cell<bool>,
    // tx_ready: &'a dyn kernel::hil::gpio::Pin,
    // rx_ready: &'a dyn kernel::hil::gpio::Pin,
}

impl UartCapsule {
    pub fn new(
        device: &'static UartDevice,
        tx_buffer: &'static mut [u8],

        rx_buffer: &'static mut [u8],
        // tx_in_progress: Cell<bool>,
        // rx_in_progress: Cell<bool>,
        // tx_ready: &'a dyn kernel::hil::gpio::Pin,
        // rx_ready: &'a dyn kernel::hil::gpio::Pin,
    ) -> UartCapsule {
        //
        UartCapsule {
            device: device,
            tx_buffer: TakeCell::new(tx_buffer),
            rx_buffer: TakeCell::new(rx_buffer),
            // tx_in_progress: Cell::new(false),
            // rx_in_progress: Cell::new(false),
            // tx_ready: tx_ready,
            // rx_ready: rx_ready,
        }
    }
    //
    // UartCapsule.start_transmission()
    // buf should not take ownership of, should borrow, buffer
    pub fn start_transmission(&self, buffer: &[u8]) -> Result<(), ErrorCode> {
        // for byte in buffer copy into buf
        // debug!("[DEBUG] send() works!");
        self.tx_buffer
            .take()
            .map_or(Err(ErrorCode::BUSY), |tx_buf| {
                for (i, c) in buffer.iter().enumerate() {
                    // Don't need to account for mismatched data length
                    if i < tx_buf.len() {
                        tx_buf[i] = *c;
                        // debug!("{}", tx_buf[i]);
                    } else {
                        debug!("buffer too big");
                    }
                }
                // let copy_len = dest.len().max(len);

                // dest[0..copy_len].copy_from_slice(&buffer[0..copy_len]);
                // }
                let len = tx_buf.len();
                let result = self.device.transmit_buffer(tx_buf, len);
                match result {
                    Ok(()) => Ok(()),
                    Err((code, buffer)) => {
                        self.tx_buffer.replace(buffer);
                        Err(code)
                    }
                }
            })
    }
    // if !(self.tx_buffer.is_none()) {
    //     // self.tx_buffer.replace(buffer);
    //     let buf = self.tx_buffer.take().unwrap();
    //     let len = buf.len();

    //     let _ = self.device.transmit_buffer(buf, len);

    //     //return empty or error
    // }
    //
    // UartCapsule.receive()
    // TODO
    // 1) Continuous receiving
    // 2) In-progress flags
    // 3) Mismatch buffer lengths
    pub fn receive(&self) -> Result<(), ErrorCode> {
        // Base Case 1: If the rx_buffer has something in it,
        // then we are able to actually receive stuff
        // if self.rx_buffer.is_none() {
        //     return Err(ErrorCode::BUSY);
        // }
        self.rx_buffer
            .take()
            .map_or(Err(ErrorCode::BUSY), |rx_buf| {
                let len = rx_buf.len();
                let result: Result<(), (ErrorCode, &mut [u8])> =
                    self.device.receive_buffer(rx_buf, len);
                match result {
                    Ok(()) => Ok(()),
                    Err((code, buffer)) => {
                        self.rx_buffer.replace(buffer);
                        Err(code)
                    }
                }
            })
    }
}

impl uart::TransmitClient for UartCapsule {
    fn transmitted_buffer(
        &self,
        buffer: &'static mut [u8],
        tx_len: usize,
        rval: Result<(), ErrorCode>,
    ) {
        self.rx_buffer.replace(buffer);

        // for pong: call self.receive()
        let result = self.receive();
        // debug!("started receiving :)");

        if let Err(code) = result {
            debug!("{:?}", code);
        } 
    }
    fn transmitted_word(&self, _rval: Result<(), ErrorCode>) {}
}

impl uart::ReceiveClient for UartCapsule {
    fn received_buffer(
        &self,
        buffer: &'static mut [u8],
        rx_len: usize,
        rcode: Result<(), ErrorCode>,
        error: uart::Error,
    ) {
        debug!("{}", buffer[0]);

        // Print out what was received in transmission
        buffer[0] += 1; // Increment the 0th value of the buffer for pong
                        // self.send(buffer);

        let mut new_buffer: [u8; 20] = [0; 20];

        for (i, c) in buffer.iter().enumerate() {
            new_buffer[i] = *c;
        }

        self.tx_buffer.replace(buffer);
        // self.rx_buffer.replace(new_buffer);
        // Copy the contents of the original buffer into the new buffer

        // let receive_result = self.receive();

        // match receive_result {
        //     Ok(()) => {
        //         debug!("receive started");
        //     }
        //     Err(code) => {
        //         debug!("{:?}", code);
        //     }
        // }

        let transmission_result = self.start_transmission(&new_buffer);
        if let Err(code) = transmission_result {
            debug!("{:?}", code);
        }
        // check result/error code
    }

    fn received_word(&self, _word: u32, _rval: Result<(), ErrorCode>, _error: uart::Error) {}
}
