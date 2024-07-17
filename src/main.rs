#![no_std]
#![no_main]
#![feature(naked_functions)]
#![feature(asm_const)]
#![deny(unsafe_op_in_unsafe_fn)]

use core::{mem::MaybeUninit, ptr::addr_of};

use arrayvec::{ArrayString, ArrayVec};
use config::{HART_STACK_LEN, MAX_CPUS};
use fdt_rs::{
    base::DevTree,
    error::DevTreeError,
    prelude::{FallibleIterator, PropReader},
};
use servos::riscv;
use uart::SbiConsole;

mod config;
mod dump_fdt;
mod uart;

#[repr(C, align(16))]
pub struct Align16<T>(pub T);

static mut KSTACK: Align16<MaybeUninit<[[u8; HART_STACK_LEN]; MAX_CPUS + 1]>> =
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
            add     t0, t0, t0
            add     sp, sp, t0
            mv      a2, sp
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

struct Syscon {
    base: *mut u32,
    shutdown_magic: u32,
    restart_magic: u32,
}

impl Syscon {
    pub unsafe fn init_with_magic(
        base: *mut u32,
        shutdown_magic: u32,
        restart_magic: u32,
    ) -> Self {
        Self {
            base,
            shutdown_magic,
            restart_magic,
        }
    }

    pub fn shutdown(&self) -> ! {
        unsafe { self.base.write_volatile(self.shutdown_magic) };
        halt()
    }

    pub fn restart(&self) -> ! {
        unsafe { self.base.write_volatile(self.restart_magic) };
        halt()
    }
}

unsafe fn locate_syscon(dt: &DevTree) -> Option<Syscon> {
    let node = dt
        .compatible_nodes("syscon")
        .iterator()
        .next()
        .and_then(|n| n.ok())?;
    // TODO: assuming u64 addresses and sizes
    let base = node
        .props()
        .iterator()
        .find_map(|prop| prop.ok().filter(|p| p.name().is_ok_and(|n| n == "reg")))
        .and_then(|prop| prop.u64(0).ok())?;
    // TODO: search device tree for these
    let shutdown_magic = 0x5555;
    let restart_magic = 0x7777;
    unsafe {
        Some(Syscon::init_with_magic(
            base as *mut u32,
            shutdown_magic,
            restart_magic,
        ))
    }
}

extern "C" fn kmain(hartid: usize, fdt: *const u8, sp: usize) -> ! {
    println!(
        "\n\nHello world from kernel hart {hartid}!\n_kernel_end: {:?}\nsp: {sp:#x}\nKSTACK: {:?}\nfdt: {:?}",
        unsafe { addr_of!(_kernel_end) },
        unsafe { addr_of!(KSTACK) },
        fdt
    );

    // static mut BUFFER: [u8; 0x4000] = [0; 0x4000];
    let dt = unsafe { DevTree::from_raw_pointer(fdt) }.unwrap();
    // dump_fdt::dump_tree(dt, unsafe { &mut BUFFER[..] }).unwrap();
    println!("Loaded device tree");

    // println!("\n----");
    // loop {
    //     if let Some(ch) = SbiConsole::read() {
    //         println!("{ch:#02x}");
    //     }
    // }

    let syscon = &unsafe { locate_syscon(&dt) };
    if let Some(syscon) = syscon {
        println!("Syscon compatible device found at {:?}", syscon.base);
    } else {
        println!("No syscon compatible device found. Shutdown will spin.");
    }

    let mut cmd = ArrayString::<256>::new();
    let mut buf = ArrayVec::<u8, 4>::new();
    let process_cmd = |cmd: &mut ArrayString<256>| {
        println!("\nCommand: '{cmd}'");
        if cmd == "exit" {
            match syscon {
                Some(s) => s.shutdown(),
                None => halt(),
            }
        } else if cmd == "restart" {
            match syscon {
                Some(s) => s.restart(),
                None => halt(),
            }
        }
        cmd.clear();
        print!(">> ");
    };

    print!("\n>> ");
    loop {
        let ch = loop {
            if let Some(b) = SbiConsole::read() {
                break b;
            }
        };
        match ch {
            0x0d => {
                process_cmd(&mut cmd);
                buf.clear();
            }
            0x7f => {
                /* DEL */
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
                        print!("\nToo long!");
                        process_cmd(&mut cmd);
                    }
                    buf.clear();
                }
            }
            _ => {}
        }
    }
}
