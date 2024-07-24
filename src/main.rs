#![no_std]
#![no_main]
#![feature(naked_functions)]
#![feature(asm_const)]
#![feature(abi_riscv_interrupt)]
#![feature(fn_align)]
#![deny(unsafe_op_in_unsafe_fn)]

use core::{
    mem::MaybeUninit,
    ptr::{addr_of, addr_of_mut},
};

use alloc::vec;
use arrayvec::{ArrayString, ArrayVec};
use config::HART_STACK_LEN;
use fdt_rs::{
    base::{DevTree, DevTreeNode},
    prelude::{FallibleIterator, PropReader},
};
use plic::PLIC;
use servos::{
    drivers::{Ns16550a, Syscon},
    heap::BlockAlloc,
    lock::SpinLocked,
    riscv::{self, halt},
};

mod config;
mod dump_fdt;
mod plic;
mod trap;
mod uart;

extern crate alloc;

#[repr(C, align(16))]
pub struct Align16<T>(pub T);

static mut KSTACK: Align16<MaybeUninit<[u8; HART_STACK_LEN]>> = Align16(MaybeUninit::uninit());

#[global_allocator]
static ALLOCATOR: SpinLocked<BlockAlloc> = SpinLocked::new(BlockAlloc::new());

#[panic_handler]
fn on_panic(info: &core::panic::PanicInfo) -> ! {
    println!("panic: {info}");
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
    static mut _kernel_end: u8;
}

unsafe fn find_reg_addr(node: &DevTreeNode) -> Option<usize> {
    // TODO: assuming u64 addresses and sizes
    node.props()
        .find(|prop| prop.name().map(|n| n == "reg"))
        .ok()
        .flatten()
        .and_then(|prop| prop.u64(0).ok().map(|s| s as usize))
}

unsafe fn init_syscon(dt: &DevTree) -> Option<Syscon> {
    let node = dt
        .compatible_nodes("syscon")
        .iterator()
        .next()
        .and_then(|n| n.ok())?;
    let base = unsafe { find_reg_addr(&node)? };
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

unsafe fn init_uart(dt: &DevTree) -> Option<u32> {
    let Ok(Some(node)) = dt.compatible_nodes("ns16550a").next() else {
        return None;
    };

    let Some(base) = (unsafe { find_reg_addr(&node) }) else {
        return None;
    };

    let Some(Ok(clock)) = node
        .props()
        .find(|prop| Ok(prop.name()? == "clock-frequency"))
        .ok()
        .flatten()
        .map(|prop| prop.u32(0))
    else {
        return None;
    };

    let Some(Ok(plic_irq)) = node
        .props()
        .find(|prop| Ok(prop.name()? == "interrupts"))
        .ok()
        .flatten()
        .map(|prop| prop.u32(0))
    else {
        return None;
    };

    println!("Found Ns16550a compatible device at address {base:#010x}");
    *uart::CONS.lock() = uart::DebugIo::Ns16550a(unsafe { Ns16550a::new(base, clock) });

    Some(plic_irq)
}

unsafe fn init_heap(dt: &DevTree) -> bool {
    let Ok(Some((addr, size))) = dt.nodes().find_map(|node| {
        let Some(dev) = node
            .props()
            .find(|prop| Ok(prop.name()? == "device_type"))?
        else {
            return Ok(None);
        };
        if dev.str() != Ok("memory") {
            return Ok(None);
        }

        let Some(reg) = node.props().find(|prop| Ok(prop.name()? == "reg"))? else {
            return Ok(None);
        };

        Ok(Some((reg.u64(0)?, reg.u64(1)?)))
    }) else {
        return false;
    };

    unsafe {
        let kend = addr_of_mut!(_kernel_end);
        let size = size as usize - (kend as usize - addr as usize);
        let heap = core::slice::from_raw_parts_mut(kend as *mut MaybeUninit<u8>, size);
        println!("Initializing heap:");
        println!("    RAM starts at {:?}", addr as *const u8);
        println!("    Kernel ends at {kend:?}");
        println!(
            "    Heap size: {size:#x} bytes ({} MiB), range [{:?}, {:?})",
            (size >> 20),
            heap.as_ptr(),
            heap.as_ptr_range().end,
        );

        ALLOCATOR.lock().init(heap);
        true
    }
}

unsafe fn init_plic(dt: &DevTree) -> bool {
    let Ok(Some(node)) = dt.compatible_nodes("riscv,plic0").next() else {
        return false;
    };

    let Some(base) = (unsafe { find_reg_addr(&node) }) else {
        return false;
    };

    println!("PLIC found at address {:?}", base as *mut u8);
    unsafe {
        PLIC.init(base as *mut _);
    }

    true
}

fn console(syscon: Option<&Syscon>) -> ! {
    fn getchar() -> u8 {
        // loop {
        //     if let Some(b) = uart::CONS.lock().read() {
        //         break b;
        //     }
        // }
        loop {}
    }
    // loop {
    //     let Some(ch) = uart::CONS.lock().read_sync() else {
    //         continue;
    //     };
    //     println!("{ch:#02x}");
    // }

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
        let ch = getchar();
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

extern "C" fn kmain(hartid: usize, fdt: *const u8) -> ! {
    unsafe {
        println!("\n\n");

        let dt = DevTree::from_raw_pointer(fdt).unwrap();
        let uart_plic_irq = init_uart(&dt);
        if uart_plic_irq.is_none() {
            println!("No Ns16550a node found in the device tree. Defaulting to SBI for I/O.");
        }

        if !init_plic(&dt) {
            panic!("No PLIC node found in the device tree.");
        }

        println!(
            "Boot hart: {hartid}. KSTACK: {:?} fdt: {fdt:?} satp: {:?}",
            addr_of!(KSTACK),
            riscv::r_satp() as *const u8,
        );

        if !init_heap(&dt) {
            panic!("[FATAL] Cannot initialize heap, no memory found in the device tree.");
        }

        let syscon = init_syscon(&dt);
        if let Some(syscon) = &syscon {
            println!("Syscon compatible device found at {:?}", syscon.base());
        } else {
            println!("No syscon compatible device found. Shutdown will spin.");
        }

        // static mut BUFFER: [u8; 0x4000] = [0; 0x4000];
        // dump_fdt::dump_tree(dt, &mut BUFFER[..] }).unwrap();

        PLIC.set_hart_priority_threshold(0);
        if let Some(uart_plic_irq) = uart_plic_irq {
            println!("UART PLIC IRQ is {uart_plic_irq:#x}");
            PLIC.set_priority(uart_plic_irq, 1);
            PLIC.hart_enable(uart_plic_irq);
        }

        trap::install();

        console(syscon.as_ref())
    }
}
