// Licensed under the Apache License, Version 2.0 or the MIT License.
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Copyright Tock Contributors 2022.

//! Support for the 32-bit RISC-V architecture.

#![crate_name = "rv32i"]
#![crate_type = "rlib"]
#![feature(naked_functions)]
#![no_std]

use core::fmt::Write;

use kernel::utilities::registers::interfaces::{Readable, Writeable};

pub mod clic;
pub mod machine_timer;
pub mod pmp;
pub mod support;
pub mod syscall;

// Re-export the shared CSR library so that dependent crates do not have to have
// both rv32i and riscv as dependencies.
pub use riscv::csr;

extern "C" {
    // Where the end of the stack region is (and hence where the stack should
    // start), and the start of the stack region.
    static _estack: usize;
    static _sstack: usize;

    // Boundaries of the .bss section.
    static mut _szero: usize;
    static mut _ezero: usize;

    // Where the .data section is stored in flash.
    static mut _etext: usize;

    // Boundaries of the .data section.
    static mut _srelocate: usize;
    static mut _erelocate: usize;

    // The global pointer, value set in the linker script
    #[link_name = "__global_pointer$"]
    static __global_pointer: usize;
}

/// Entry point of all programs (`_start`).
///
/// This assembly does three functions:
///
/// 1. It initializes the stack pointer, the frame pointer (needed for closures
///    to work in start_rust) and the global pointer.
/// 2. It initializes the .bss and .data RAM segments. This must be done before
///    any Rust code runs. See https://github.com/tock/tock/issues/2222 for more
///    information.
/// 3. Finally it calls `main()`, the main entry point for Tock boards.
#[cfg(all(target_arch = "riscv32", target_os = "none"))]
#[link_section = ".riscv.start"]
#[export_name = "_start"]
#[naked]
pub extern "C" fn _start() {
    use core::arch::asm;
    unsafe {
        asm! ("
            // Set the global pointer register using the variable defined in the
            // linker script. This register is only set once. The global pointer
            // is a method for sharing state between the linker and the CPU so
            // that the linker can emit code with offsets that are relative to
            // the gp register, and the CPU can successfully execute them.
            //
            // https://gnu-mcu-eclipse.github.io/arch/riscv/programmer/#the-gp-global-pointer-register
            // https://groups.google.com/a/groups.riscv.org/forum/#!msg/sw-dev/60IdaZj27dY/5MydPLnHAQAJ
            // https://www.sifive.com/blog/2017/08/28/all-aboard-part-3-linker-relaxation-in-riscv-toolchain/
            //
            // Disable linker relaxation for code that sets up GP so that this doesn't
            // get turned into `mv gp, gp`.
            .option push
            .option norelax

            la gp, {gp}                 // Set the global pointer from linker script.

            // Re-enable linker relaxations.
            .option pop

            // Initialize the stack pointer register. This comes directly from
            // the linker script.
            la sp, {estack}             // Set the initial stack pointer.

            // Set s0 (the frame pointer) to the start of the stack.
            add  s0, sp, zero           // s0 = sp

            // Initialize mscratch to 0 so that we know that we are currently
            // in the kernel. This is used for the check in the trap handler.
            csrw 0x340, zero            // CSR=0x340=mscratch

            // INITIALIZE MEMORY

            // Start by initializing .bss memory. The Tock linker script defines
            // `_szero` and `_ezero` to mark the .bss segment.
            la a0, {sbss}               // a0 = first address of .bss
            la a1, {ebss}               // a1 = first address after .bss

          100: // bss_init_loop
            beq  a0, a1, 101f           // If a0 == a1, we are done.
            sw   zero, 0(a0)            // *a0 = 0. Write 0 to the memory location in a0.
            addi a0, a0, 4              // a0 = a0 + 4. Increment pointer to next word.
            j 100b                      // Continue the loop.

          101: // bss_init_done


            // Now initialize .data memory. This involves coping the values right at the
            // end of the .text section (in flash) into the .data section (in RAM).
            la a0, {sdata}              // a0 = first address of data section in RAM
            la a1, {edata}              // a1 = first address after data section in RAM
            la a2, {etext}              // a2 = address of stored data initial values

          200: // data_init_loop
            beq  a0, a1, 201f           // If we have reached the end of the .data
                                        // section then we are done.
            lw   a3, 0(a2)              // a3 = *a2. Load value from initial values into a3.
            sw   a3, 0(a0)              // *a0 = a3. Store initial value into
                                        // next place in .data.
            addi a0, a0, 4              // a0 = a0 + 4. Increment to next word in memory.
            addi a2, a2, 4              // a2 = a2 + 4. Increment to next word in flash.
            j 200b                      // Continue the loop.

          201: // data_init_done

            // With that initial setup out of the way, we now branch to the main
            // code, likely defined in a board's main.rs.
            j main
        ",
        gp = sym __global_pointer,
        estack = sym _estack,
        sbss = sym _szero,
        ebss = sym _ezero,
        sdata = sym _srelocate,
        edata = sym _erelocate,
        etext = sym _etext,
        options(noreturn)
        );
    }
}

/// The various privilege levels in RISC-V.
pub enum PermissionMode {
    User = 0x0,
    Supervisor = 0x1,
    Reserved = 0x2,
    Machine = 0x3,
}

/// Tell the MCU what address the trap handler is located at.
///
/// This is a generic implementation. There may be board specific versions as
/// some platforms have added more bits to the `mtvec` register.
///
/// The trap handler is called on exceptions and for interrupts.
pub unsafe fn configure_trap_handler(mode: PermissionMode) {
    match mode {
        PermissionMode::Machine => csr::CSR.mtvec.write(
            csr::mtvec::mtvec::trap_addr.val(_start_trap as usize >> 2)
                + csr::mtvec::mtvec::mode::CLEAR,
        ),
        PermissionMode::Supervisor => csr::CSR.stvec.write(
            csr::stvec::stvec::trap_addr.val(_start_trap as usize >> 2)
                + csr::stvec::stvec::mode::CLEAR,
        ),
        PermissionMode::User => csr::CSR.utvec.write(
            csr::utvec::utvec::trap_addr.val(_start_trap as usize >> 2)
                + csr::utvec::utvec::mode::CLEAR,
        ),
        PermissionMode::Reserved => {
            // TODO some sort of error handling?
        }
    }
}

// Mock implementation for tests on Travis-CI.
#[cfg(not(any(target_arch = "riscv32", target_os = "none")))]
pub extern "C" fn _start_trap() {
    unimplemented!()
}

/// This is the trap handler function. This code is called on all traps,
/// including interrupts, exceptions, and system calls from applications.
///
/// Tock uses only the single trap handler, and does not use any vectored
/// interrupts or other exception handling. The trap handler has to determine
/// why the trap handler was called, and respond accordingly. Generally, there
/// are two reasons the trap handler gets called: an interrupt occurred or an
/// application called a syscall.
///
/// In the case of an interrupt while the kernel was executing we only need to
/// save the kernel registers and then run whatever interrupt handling code we
/// need to. If the trap happens while and application was executing, we have to
/// save the application state and then resume the `switch_to()` function to
/// correctly return back to the kernel.
#[cfg(all(target_arch = "riscv32", target_os = "none"))]
#[link_section = ".riscv.trap"]
#[export_name = "_start_trap"]
#[naked]
pub extern "C" fn _start_trap() {
    use core::arch::asm;
    unsafe {
        asm!(
            "
            // The first thing we have to do is determine if we came from user
            // mode or kernel mode, as we need to save state and proceed
            // differently. We cannot, however, use any registers because we do
            // not want to lose their contents. So, we rely on `mscratch`. If
            // mscratch is 0, then we came from the kernel. If it is >0, then it
            // contains the kernel's stack pointer and we came from an app.
            //
            // We use the csrrw instruction to save the current stack pointer
            // so we can retrieve it if necessary.
            //
            // If we could enter this trap handler twice (for example,
            // handling an interrupt while an exception is being
            // handled), storing a non-zero value in mscratch
            // temporarily could cause a race condition similar to the
            // one of PR 2308[1].
            // However, as indicated in section 3.1.6.1 of the RISC-V
            // Privileged Spec[2], MIE will be set to 0 when taking a
            // trap into machine mode. Therefore, this can only happen
            // when causing an exception in the trap handler itself.
            //
            // [1] https://github.com/tock/tock/pull/2308
            // [2] https://github.com/riscv/riscv-isa-manual/releases/download/draft-20201222-42dc13a/riscv-privileged.pdf
            csrrw sp, 0x340, sp // CSR=0x340=mscratch
            bnez  sp, 300f      // If sp != 0 then we must have come from an app.


        // _from_kernel:
            // Swap back the zero value for the stack pointer in mscratch
            csrrw sp, 0x340, sp // CSR=0x340=mscratch

            // Now, since we want to use the stack to save kernel registers, we
            // first need to make sure that the trap wasn't the result of a
            // stack overflow, in which case we can't use the current stack
            // pointer. We also, however, cannot modify any of the current
            // registers until we save them, and we cannot save them to the
            // stack until we know the stack is valid. So, we use the mscratch
            // trick again to get one register we can use.

            // Save t0's contents to mscratch
            csrw 0x340, t0                      // CSR=0x340=mscratch

            // Load the address of the bottom of the stack (`_sstack`) into our
            // newly freed-up t0 register.
            la t0, {sstack}                     // t0 = _sstack

            // Compare the kernel stack pointer to the bottom of the stack. If
            // the stack pointer is above the bottom of the stack, then continue
            // handling the fault as normal.
            bgtu sp, t0, 100f                   // branch if sp > t0

            // If we get here, then we did encounter a stack overflow. We are
            // going to panic at this point, but for that to work we need a
            // valid stack to run the panic code. We do this by just starting
            // over with the kernel stack and placing the stack pointer at the
            // top of the original stack.
            la sp, {estack}                     // sp = _estack


        100: // _from_kernel_continue

            // Restore t0, and make sure mscratch is set back to 0 (our flag
            // tracking that the kernel is executing).
            csrrw t0, 0x340, zero // t0=mscratch, mscratch=0

            // Make room for the caller saved registers we need to restore after
            // running any trap handler code.
            addi sp, sp, -16*4

            // Save all of the caller saved registers.
            sw   ra, 0*4(sp)
            sw   t0, 1*4(sp)
            sw   t1, 2*4(sp)
            sw   t2, 3*4(sp)
            sw   t3, 4*4(sp)
            sw   t4, 5*4(sp)
            sw   t5, 6*4(sp)
            sw   t6, 7*4(sp)
            sw   a0, 8*4(sp)
            sw   a1, 9*4(sp)
            sw   a2, 10*4(sp)
            sw   a3, 11*4(sp)
            sw   a4, 12*4(sp)
            sw   a5, 13*4(sp)
            sw   a6, 14*4(sp)
            sw   a7, 15*4(sp)

            // Jump to board-specific trap handler code. Likely this was an
            // interrupt and we want to disable a particular interrupt, but each
            // board/chip can customize this as needed.
            jal ra, _start_trap_rust_from_kernel

            // Restore the registers from the stack.
            lw   ra, 0*4(sp)
            lw   t0, 1*4(sp)
            lw   t1, 2*4(sp)
            lw   t2, 3*4(sp)
            lw   t3, 4*4(sp)
            lw   t4, 5*4(sp)
            lw   t5, 6*4(sp)
            lw   t6, 7*4(sp)
            lw   a0, 8*4(sp)
            lw   a1, 9*4(sp)
            lw   a2, 10*4(sp)
            lw   a3, 11*4(sp)
            lw   a4, 12*4(sp)
            lw   a5, 13*4(sp)
            lw   a6, 14*4(sp)
            lw   a7, 15*4(sp)

            // Reset the stack pointer.
            addi sp, sp, 16*4

            // mret returns from the trap handler. The PC is set to what is in
            // mepc and execution proceeds from there. Since we did not modify
            // mepc we will return to where the exception occurred.
            mret



            // Handle entering the trap handler from an app differently.
        300: // _from_app

            // At this point all we know is that we entered the trap handler
            // from an app. We don't know _why_ we got a trap, it could be from
            // an interrupt, syscall, or fault (or maybe something else).
            // Therefore we have to be very careful not to overwrite any
            // registers before we have saved them.
            //
            // We ideally want to save registers in the per-process stored state
            // struct. However, we don't have a pointer to that yet, and we need
            // to use a temporary register to get that address. So, we save s0
            // to the kernel stack before we can it to the proper spot.
            sw   s0, 0*4(sp)

            // Ideally it would be better to save all of the app registers once
            // we return back to the `switch_to_process()` code. However, we
            // also potentially need to disable an interrupt in case the app was
            // interrupted, so it is safer to just immediately save all of the
            // app registers.
            //
            // We do this by retrieving the stored state pointer from the kernel
            // stack and storing the necessary values in it.
            lw   s0,  1*4(sp)  // Load the stored state pointer into s0.
            sw   x1,  0*4(s0)  // ra
            sw   x3,  2*4(s0)  // gp
            sw   x4,  3*4(s0)  // tp
            sw   x5,  4*4(s0)  // t0
            sw   x6,  5*4(s0)  // t1
            sw   x7,  6*4(s0)  // t2
            sw   x9,  8*4(s0)  // s1
            sw   x10, 9*4(s0)  // a0
            sw   x11, 10*4(s0) // a1
            sw   x12, 11*4(s0) // a2
            sw   x13, 12*4(s0) // a3
            sw   x14, 13*4(s0) // a4
            sw   x15, 14*4(s0) // a5
            sw   x16, 15*4(s0) // a6
            sw   x17, 16*4(s0) // a7
            sw   x18, 17*4(s0) // s2
            sw   x19, 18*4(s0) // s3
            sw   x20, 19*4(s0) // s4
            sw   x21, 20*4(s0) // s5
            sw   x22, 21*4(s0) // s6
            sw   x23, 22*4(s0) // s7
            sw   x24, 23*4(s0) // s8
            sw   x25, 24*4(s0) // s9
            sw   x26, 25*4(s0) // s10
            sw   x27, 26*4(s0) // s11
            sw   x28, 27*4(s0) // t3
            sw   x29, 28*4(s0) // t4
            sw   x30, 29*4(s0) // t5
            sw   x31, 30*4(s0) // t6
            // Now retrieve the original value of s0 and save that as well.
            lw   t0,  0*4(sp)
            sw   t0,  7*4(s0)  // s0,fp

            // We also need to store the app stack pointer, mcause, and mepc. We
            // need to store mcause because we use that to determine why the app
            // stopped executing and returned to the kernel. We store mepc
            // because it is where we need to return to in the app at some
            // point. We need to store mtval in case the app faulted and we need
            // mtval to help with debugging.
            csrr t0, 0x340    // CSR=0x340=mscratch
            sw   t0, 1*4(s0)  // Save the app sp to the stored state struct
            csrr t0, 0x341    // CSR=0x341=mepc
            sw   t0, 31*4(s0) // Save the PC to the stored state struct
            csrr t0, 0x343    // CSR=0x343=mtval
            sw   t0, 33*4(s0) // Save mtval to the stored state struct

            // Save mcause last, as we depend on it being loaded in t0 below
            csrr t0, 0x342    // CSR=0x342=mcause
            sw   t0, 32*4(s0) // Save mcause to the stored state struct, leave in t0

            // Now we need to check if this was an interrupt, and if it was,
            // then we need to disable the interrupt before returning from this
            // trap handler so that it does not fire again. If mcause is greater
            // than or equal to zero this was not an interrupt (i.e. the most
            // significant bit is not 1).
            bge  t0, zero, 200f
            // Copy mcause into a0 and then call the interrupt disable function.
            mv   a0, t0
            jal  ra, _disable_interrupt_trap_rust_from_app

        200: // _from_app_continue
            // Now determine the address of _return_to_kernel and resume the
            // context switching code. We need to load _return_to_kernel into
            // mepc so we can use it to return to the context switch code.
            lw   t0, 2*4(sp)  // Load _return_to_kernel into t0.
            csrw 0x341, t0    // CSR=0x341=mepc

            // Ensure that mscratch is 0. This makes sure that we know that on
            // a future trap that we came from the kernel.
            csrw 0x340, zero  // CSR=0x340=mscratch

            // Need to set mstatus.MPP to 0b11 so that we stay in machine mode.
            csrr t0, 0x300    // CSR=0x300=mstatus
            li   t1, 0x1800   // Load 0b11 to the MPP bits location in t1
            or   t0, t0, t1   // Set the MPP bits to one
            csrw 0x300, t0    // CSR=0x300=mstatus

            // Use mret to exit the trap handler and return to the context
            // switching code.
            mret
        ",
            estack = sym _estack,
            sstack = sym _sstack,
            options(noreturn)
        );
    }
}

/// RISC-V semihosting needs three exact instructions in uncompressed form.
///
/// See https://github.com/riscv/riscv-semihosting-spec/blob/main/riscv-semihosting-spec.adoc#11-semihosting-trap-instruction-sequence
/// for more details on the three insturctions.
///
/// In order to work with semihosting we include the assembly here
/// where we are able to disable compressed instruction support. This
/// follows the example used in the Linux kernel:
/// https://elixir.bootlin.com/linux/v5.12.10/source/arch/riscv/include/asm/jump_label.h#L21
/// as suggested by the RISC-V developers:
/// https://groups.google.com/a/groups.riscv.org/g/isa-dev/c/XKkYacERM04/m/CdpOcqtRAgAJ
#[cfg(all(target_arch = "riscv32", target_os = "none"))]
pub unsafe fn semihost_command(command: usize, arg0: usize, arg1: usize) -> usize {
    use core::arch::asm;
    let res;
    asm!(
    "
      .balign 16
      .option push
      .option norelax
      .option norvc
      slli x0, x0, 0x1f
      ebreak
      srai x0, x0, 7
      .option pop
      ",
    in("a0") command,
    in("a1") arg0,
    in("a2") arg1,
    lateout("a0") res,
    );
    res
}

// Mock implementation for tests on Travis-CI.
#[cfg(not(any(target_arch = "riscv32", target_os = "none")))]
pub unsafe fn semihost_command(_command: usize, _arg0: usize, _arg1: usize) -> usize {
    unimplemented!()
}

/// Print a readable string for an mcause reason.
pub unsafe fn print_mcause(mcval: csr::mcause::Trap, writer: &mut dyn Write) {
    match mcval {
        csr::mcause::Trap::Interrupt(interrupt) => match interrupt {
            csr::mcause::Interrupt::UserSoft => {
                let _ = writer.write_fmt(format_args!("User software interrupt"));
            }
            csr::mcause::Interrupt::SupervisorSoft => {
                let _ = writer.write_fmt(format_args!("Supervisor software interrupt"));
            }
            csr::mcause::Interrupt::MachineSoft => {
                let _ = writer.write_fmt(format_args!("Machine software interrupt"));
            }
            csr::mcause::Interrupt::UserTimer => {
                let _ = writer.write_fmt(format_args!("User timer interrupt"));
            }
            csr::mcause::Interrupt::SupervisorTimer => {
                let _ = writer.write_fmt(format_args!("Supervisor timer interrupt"));
            }
            csr::mcause::Interrupt::MachineTimer => {
                let _ = writer.write_fmt(format_args!("Machine timer interrupt"));
            }
            csr::mcause::Interrupt::UserExternal => {
                let _ = writer.write_fmt(format_args!("User external interrupt"));
            }
            csr::mcause::Interrupt::SupervisorExternal => {
                let _ = writer.write_fmt(format_args!("Supervisor external interrupt"));
            }
            csr::mcause::Interrupt::MachineExternal => {
                let _ = writer.write_fmt(format_args!("Machine external interrupt"));
            }
            csr::mcause::Interrupt::Unknown => {
                let _ = writer.write_fmt(format_args!("Reserved/Unknown"));
            }
        },
        csr::mcause::Trap::Exception(exception) => match exception {
            csr::mcause::Exception::InstructionMisaligned => {
                let _ = writer.write_fmt(format_args!("Instruction access misaligned"));
            }
            csr::mcause::Exception::InstructionFault => {
                let _ = writer.write_fmt(format_args!("Instruction access fault"));
            }
            csr::mcause::Exception::IllegalInstruction => {
                let _ = writer.write_fmt(format_args!("Illegal instruction"));
            }
            csr::mcause::Exception::Breakpoint => {
                let _ = writer.write_fmt(format_args!("Breakpoint"));
            }
            csr::mcause::Exception::LoadMisaligned => {
                let _ = writer.write_fmt(format_args!("Load address misaligned"));
            }
            csr::mcause::Exception::LoadFault => {
                let _ = writer.write_fmt(format_args!("Load access fault"));
            }
            csr::mcause::Exception::StoreMisaligned => {
                let _ = writer.write_fmt(format_args!("Store/AMO address misaligned"));
            }
            csr::mcause::Exception::StoreFault => {
                let _ = writer.write_fmt(format_args!("Store/AMO access fault"));
            }
            csr::mcause::Exception::UserEnvCall => {
                let _ = writer.write_fmt(format_args!("Environment call from U-mode"));
            }
            csr::mcause::Exception::SupervisorEnvCall => {
                let _ = writer.write_fmt(format_args!("Environment call from S-mode"));
            }
            csr::mcause::Exception::MachineEnvCall => {
                let _ = writer.write_fmt(format_args!("Environment call from M-mode"));
            }
            csr::mcause::Exception::InstructionPageFault => {
                let _ = writer.write_fmt(format_args!("Instruction page fault"));
            }
            csr::mcause::Exception::LoadPageFault => {
                let _ = writer.write_fmt(format_args!("Load page fault"));
            }
            csr::mcause::Exception::StorePageFault => {
                let _ = writer.write_fmt(format_args!("Store/AMO page fault"));
            }
            csr::mcause::Exception::Unknown => {
                let _ = writer.write_fmt(format_args!("Reserved"));
            }
        },
    }
}

/// Prints out RISCV machine state, including basic system registers
/// (mcause, mstatus, mtvec, mepc, mtval, interrupt status).
pub unsafe fn print_riscv_state(writer: &mut dyn Write) {
    let mcval: csr::mcause::Trap = core::convert::From::from(csr::CSR.mcause.extract());
    let _ = writer.write_fmt(format_args!("\r\n---| RISC-V Machine State |---\r\n"));
    let _ = writer.write_fmt(format_args!("Last cause (mcause): "));
    print_mcause(mcval, writer);
    let interrupt = csr::CSR.mcause.read(csr::mcause::mcause::is_interrupt);
    let code = csr::CSR.mcause.read(csr::mcause::mcause::reason);
    let _ = writer.write_fmt(format_args!(
        " (interrupt={}, exception code={:#010X})",
        interrupt, code
    ));
    let _ = writer.write_fmt(format_args!(
        "\r\nLast value (mtval):  {:#010X}\
         \r\n\
         \r\nSystem register dump:\
         \r\n mepc:    {:#010X}    mstatus:     {:#010X}\
         \r\n mcycle:  {:#010X}    minstret:    {:#010X}\
         \r\n mtvec:   {:#010X}",
        csr::CSR.mtval.get(),
        csr::CSR.mepc.get(),
        csr::CSR.mstatus.get(),
        csr::CSR.mcycle.get(),
        csr::CSR.minstret.get(),
        csr::CSR.mtvec.get()
    ));
    let mstatus = csr::CSR.mstatus.extract();
    let uie = mstatus.is_set(csr::mstatus::mstatus::uie);
    let sie = mstatus.is_set(csr::mstatus::mstatus::sie);
    let mie = mstatus.is_set(csr::mstatus::mstatus::mie);
    let upie = mstatus.is_set(csr::mstatus::mstatus::upie);
    let spie = mstatus.is_set(csr::mstatus::mstatus::spie);
    let mpie = mstatus.is_set(csr::mstatus::mstatus::mpie);
    let spp = mstatus.is_set(csr::mstatus::mstatus::spp);
    let _ = writer.write_fmt(format_args!(
        "\r\n mstatus: {:#010X}\
         \r\n  uie:    {:5}  upie:   {}\
         \r\n  sie:    {:5}  spie:   {}\
         \r\n  mie:    {:5}  mpie:   {}\
         \r\n  spp:    {}",
        mstatus.get(),
        uie,
        upie,
        sie,
        spie,
        mie,
        mpie,
        spp
    ));
    let e_usoft = csr::CSR.mie.is_set(csr::mie::mie::usoft);
    let e_ssoft = csr::CSR.mie.is_set(csr::mie::mie::ssoft);
    let e_msoft = csr::CSR.mie.is_set(csr::mie::mie::msoft);
    let e_utimer = csr::CSR.mie.is_set(csr::mie::mie::utimer);
    let e_stimer = csr::CSR.mie.is_set(csr::mie::mie::stimer);
    let e_mtimer = csr::CSR.mie.is_set(csr::mie::mie::mtimer);
    let e_uext = csr::CSR.mie.is_set(csr::mie::mie::uext);
    let e_sext = csr::CSR.mie.is_set(csr::mie::mie::sext);
    let e_mext = csr::CSR.mie.is_set(csr::mie::mie::mext);

    let p_usoft = csr::CSR.mip.is_set(csr::mip::mip::usoft);
    let p_ssoft = csr::CSR.mip.is_set(csr::mip::mip::ssoft);
    let p_msoft = csr::CSR.mip.is_set(csr::mip::mip::msoft);
    let p_utimer = csr::CSR.mip.is_set(csr::mip::mip::utimer);
    let p_stimer = csr::CSR.mip.is_set(csr::mip::mip::stimer);
    let p_mtimer = csr::CSR.mip.is_set(csr::mip::mip::mtimer);
    let p_uext = csr::CSR.mip.is_set(csr::mip::mip::uext);
    let p_sext = csr::CSR.mip.is_set(csr::mip::mip::sext);
    let p_mext = csr::CSR.mip.is_set(csr::mip::mip::mext);
    let _ = writer.write_fmt(format_args!(
        "\r\n mie:   {:#010X}   mip:   {:#010X}\
         \r\n  usoft:  {:6}              {:6}\
         \r\n  ssoft:  {:6}              {:6}\
         \r\n  msoft:  {:6}              {:6}\
         \r\n  utimer: {:6}              {:6}\
         \r\n  stimer: {:6}              {:6}\
         \r\n  mtimer: {:6}              {:6}\
         \r\n  uext:   {:6}              {:6}\
         \r\n  sext:   {:6}              {:6}\
         \r\n  mext:   {:6}              {:6}\r\n",
        csr::CSR.mie.get(),
        csr::CSR.mip.get(),
        e_usoft,
        p_usoft,
        e_ssoft,
        p_ssoft,
        e_msoft,
        p_msoft,
        e_utimer,
        p_utimer,
        e_stimer,
        p_stimer,
        e_mtimer,
        p_mtimer,
        e_uext,
        p_uext,
        e_sext,
        p_sext,
        e_mext,
        p_mext
    ));
}
