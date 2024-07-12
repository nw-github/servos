#![no_std]
#![no_main]
#![feature(naked_functions)]
#![feature(asm_const)]
#![deny(unsafe_op_in_unsafe_fn)]

use core::{mem::MaybeUninit, ptr::addr_of};

use arrayvec::{ArrayString, ArrayVec};
use config::{HART_STACK_LEN, MAX_CPUS};
use fdt_rs::base::DevTree;
use servos::riscv;
use uart::SbiConsole;

mod config;
mod dump_fdt;
mod uart;

#[repr(C, align(16))]
pub struct Align16<T>(pub T);

static mut KSTACK: Align16<MaybeUninit<[[u8; HART_STACK_LEN]; MAX_CPUS]>> =
    Align16(MaybeUninit::uninit());

#[inline(always)]
fn halt() -> ! {
    loop {
        unsafe {
            core::arch::asm!("wfi");
        }
    }
}

#[panic_handler]
fn on_panic(info: &core::panic::PanicInfo) -> ! {
    println!("panic: {}", info);
    halt()
}

#[naked]
#[no_mangle]
#[link_section = ".text.init"]
extern "C" fn _start() -> ! {
    unsafe {
        core::arch::asm!(
            r"
            .option push
            .option norelax
            la      gp, _global_pointer
            .option pop

            mv      tp, a0
            la      sp, {stack}
            li      t0, {stack_len}
            addi    t1, a0, 1
            mul     t0, t0, t1
            add     sp, sp, t0
            tail    {init}",

            init = sym kmain,
            stack = sym KSTACK,
            stack_len = const HART_STACK_LEN,
            options(noreturn),
        );
    }
}

extern "C" {
    static _kernel_end: u8;
}

extern "C" fn kmain(hartid: usize, fdt: *const u8) -> ! {
    println!(
        "\n\nHello world from kernel hart {hartid}!\ntp: {:#x}\nfdt_addr: {fdt:?}\n_kernel_end: {:?}",
        riscv::r_tp(),
        unsafe { addr_of!(_kernel_end) },
    );

    // static mut BUFFER: [u8; 0x4000] = [0; 0x4000];
    // let dt = unsafe { DevTree::from_raw_pointer(fdt) }.unwrap();
    // dump_fdt::dump_tree(dt, unsafe { &mut BUFFER[..] }).unwrap();

    // println!("\n----");
    // loop {
    //     if let Some(ch) = SbiConsole::read() {
    //         println!("{ch:#02x}");
    //     }
    // }

    let mut cmd = ArrayString::<256>::new();
    let mut buf = ArrayVec::<u8, 4>::new();
    print!("\n>> ");
    loop {
        let ch = loop {
            if let Some(b) = SbiConsole::read() {
                break b;
            }
        };
        match ch {
            0x0d => {
                print!("\nCommand: '{cmd}'\n>> ");
                cmd.clear();
                buf.clear();
            }
            0x7f => {
                /* carriage return (sent on backspace) */
                if !cmd.is_empty() {
                    cmd.truncate(cmd.len() - cmd.chars().last().map(|c| c.len_utf8()).unwrap_or(0));
                    print!("\x08 \x08");
                }
            }
            0x17 => {
                /* CTRL + backspace */
                if let Some(&last) = cmd.as_bytes().last() {
                    let pos = cmd
                        .bytes()
                        .rev()
                        .position(|c| if last == b' ' { c != b' ' } else { c == b' ' })
                        .map(|p| cmd.len() - p)
                        .unwrap_or(0);
                    cmd[pos..].chars().for_each(|_| print!("\x08 \x08"));
                    cmd.truncate(pos);
                }
            }
            ch if !ch.is_ascii_control() => {
                if buf.try_push(ch).is_err() {
                    buf.clear();
                } else if let Ok(s) = core::str::from_utf8(&buf) {
                    print!("{s}");
                    if cmd.try_push_str(s).is_err() {
                        print!("\nToo long! Command: '{cmd}'\n>> ");
                        cmd.clear();
                    }
                    buf.clear();
                }
            }
            _ => {}
        }
    }
}
