#![no_std]
#![no_main]
#![feature(naked_functions)]
#![feature(asm_const)]
#![feature(abi_riscv_interrupt)]
#![feature(fn_align)]
#![feature(allocator_api)]
#![feature(new_uninit)]
#![feature(stmt_expr_attributes)]
#![deny(unsafe_op_in_unsafe_fn)]

use core::{
    mem::MaybeUninit,
    ops::Range,
    ptr::{addr_of, addr_of_mut},
};

use alloc::boxed::Box;
use fdt_rs::{
    base::{DevTree, DevTreeNode},
    prelude::{FallibleIterator, PropReader},
};
use hart::{set_hart_info, HartInfo, PowerManagement, HART_STACK_LEN, MAX_HARTS, POWER};
use plic::PLIC;
use proc::{Process, Scheduler};
use servos::{
    drivers::{Ns16550a, Syscon},
    heap::BlockAlloc,
    lock::SpinLocked,
    riscv::{self, disable_intr, halt, r_satp, sfence_vma, w_satp},
    sbi::{self, hsm::HartState},
    Align16,
};
use uart::{DebugIo, CONS};
use vmm::{PageTable, Pte, PGSIZE};

mod dump_fdt;
mod hart;
mod plic;
mod proc;
mod trap;
mod uart;
mod vmm;

extern crate alloc;

static mut KSTACK: Align16<MaybeUninit<[u8; HART_STACK_LEN]>> = Align16(MaybeUninit::uninit());

#[global_allocator]
static ALLOCATOR: SpinLocked<BlockAlloc> = SpinLocked::new(BlockAlloc::new());

static mut KPAGETABLE: PageTable = PageTable::new();

extern "C" {
    static _text_start: u8;
    static _rodata_start: u8;
    static _data_start: u8;
    static _bss_start: u8;

    static _text_end: u8;
    static _rodata_end: u8;
    static _data_end: u8;
    static _bss_end: u8;

    static mut _kernel_end: u8;
}

#[panic_handler]
fn on_panic(info: &core::panic::PanicInfo) -> ! {
    let _ = disable_intr();
    println!("[FATAL] panic: {info}");
    halt()
}

#[naked]
#[no_mangle]
#[link_section = ".text.init"]
extern "C" fn _start(_hartid: usize, _fdt: *const u8) -> ! {
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
            tail    {init}",

            init = sym kmain,
            stack = sym KSTACK,
            stack_len = const HART_STACK_LEN,
            options(noreturn),
        );
    }
}

#[naked]
extern "C" fn _start_hart(_hartid: usize, _sp: usize) {
    unsafe {
        core::arch::asm!(
            r"
            .option push
            .option norelax
            la      gp, _global_pointer
            .option pop

            mv      tp, a0
            mv      sp, a1
            tail    {init}",

            init = sym kinithart,
            options(noreturn),
        );
    }
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

    let base = (unsafe { find_reg_addr(&node) })?;
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
    *uart::CONS.lock() = uart::DebugIo::Ns16550a(unsafe { Ns16550a::new(base, clock, 76800) });

    Some(plic_irq)
}

unsafe fn init_heap(dt: &DevTree) {
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
        panic!("cannot initialize heap, no memory found in the device tree.");
    };

    unsafe {
        let kend = addr_of_mut!(_kernel_end);
        let kend = kend.add(kend.align_offset(PGSIZE)); // align to next page
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
    }
}

unsafe fn init_plic(dt: &DevTree, uart_plic_irq: Option<u32>) -> bool {
    let Ok(Some(node)) = dt.compatible_nodes("riscv,plic0").next() else {
        return false;
    };

    let Some(base) = (unsafe { find_reg_addr(&node) }) else {
        return false;
    };

    println!("PLIC found at address {:?}", base as *mut u8);
    unsafe {
        PLIC.init(base as *mut _, uart_plic_irq);
    }

    if let Some(uart_plic_irq) = uart_plic_irq {
        println!("UART PLIC IRQ is {uart_plic_irq:#x}");
        unsafe {
            PLIC.set_priority(uart_plic_irq, 1);
        }
    }

    true
}

unsafe fn init_vmem() {
    let pt = unsafe { &mut *addr_of_mut!(KPAGETABLE) };
    unsafe {
        assert!(pt.map_identity(addr_of!(_text_start), addr_of!(_text_end), Pte::Rx));
        assert!(pt.map_identity(addr_of!(_rodata_start), addr_of!(_rodata_end), Pte::R));
        assert!(pt.map_identity(addr_of!(_data_start), addr_of!(_bss_end), Pte::Rw));

        assert!(pt.map_identity(PLIC.addr(), PLIC.addr().add(0x3ff_fffc), Pte::Rw));
    }

    // TODO: might be worth adding support for mega/gigapages to save some space on page tables
    let Range { start, end } = ALLOCATOR.lock().range();
    assert!(pt.map_identity(start, end, Pte::Rw));
    let uart_addr = match &*CONS.lock() {
        DebugIo::Ns16550a(uart) => Some(uart.addr()),
        DebugIo::Sbi(_) => None,
    };
    if let Some(uart_addr) = uart_addr {
        assert!(pt.map_identity(uart_addr, uart_addr, Pte::Rw));
    }
    let syscon = match &*POWER.lock() {
        PowerManagement::Syscon(s) => Some(s.addr()),
        PowerManagement::Sbi(_) => None,
    };
    if let Some(syscon) = syscon {
        assert!(pt.map_identity(syscon, syscon, Pte::Rw));
    }

    // the trap vector and return to user code must be mapped in the same place for the kernel
    // and user programs, or it would cause a page fault as soon as the page table switched
    // when entering/exiting user mode
    assert!(trap::map_trap_code(pt));
}

unsafe fn init_hart(sp: usize) {
    set_hart_info(HartInfo { sp });

    // enable virtual memory
    sfence_vma();
    w_satp(PageTable::make_satp(unsafe { addr_of!(KPAGETABLE) }));
    sfence_vma();

    // ask for PLIC interrupts
    PLIC.set_hart_priority_threshold(0);
    if let Some(irq) = PLIC.get_uart0() {
        PLIC.hart_enable(irq.into());
    }

    // enable traps and install the trap handler
    trap::hart_install();
}

extern "C" fn kmain(hartid: usize, fdt: *const u8) -> ! {
    unsafe {
        println!("\n\n");

        let dt = DevTree::from_raw_pointer(fdt).expect("Couldn't parse device tree from a1");
        let uart_plic_irq = init_uart(&dt);
        if uart_plic_irq.is_none() {
            println!("No Ns16550a node found in the device tree. Defaulting to SBI for I/O.");
        }

        if !init_plic(&dt, uart_plic_irq) {
            panic!("No PLIC node found in the device tree.");
        }

        println!(
            "Boot hart: {hartid}. KSTACK: {:?} fdt: {fdt:?} satp: {:?}",
            addr_of!(KSTACK),
            r_satp() as *const u8,
        );

        if let Some(syscon) = init_syscon(&dt) {
            println!("Syscon compatible device found at {:?}", syscon.addr());
            *POWER.lock() = PowerManagement::Syscon(syscon);
        }

        // note: the device tree lives somewhere in RAM outside the kernel area, it's potentially
        // invalidated once we initialize the heap over it
        init_heap(&dt);
        init_vmem();
        init_hart(addr_of!(KSTACK) as usize + HART_STACK_LEN);
        println!(
            "Initialized kernel page table, satp: {:?}",
            r_satp() as *const u8
        );

        // dump_fdt::dump_tree(dt).unwrap();

        for _ in 0..5 {
            Process::spawn_from_function(init_user_mode).expect("couldn't create process");
        }

        // TODO: maybe look in the device tree for hart count
        for hartid in 0..MAX_HARTS {
            // TODO: make sure this hart is M + S + U mode capable
            let Ok(HartState::Stopped) = sbi::hsm::hart_get_status(hartid) else {
                continue;
            };

            // TODO: kernel stack guard pages
            let stack = Box::leak(Box::<Align16<[u8; HART_STACK_LEN]>>::new_uninit());
            let stack = stack as *mut _ as usize + HART_STACK_LEN;
            if let Err(err) = sbi::hsm::hart_start(hartid, _start_hart, stack) {
                panic!("failed to start hart {hartid}: {err:?}");
            }
        }

        Scheduler::yield_hart()
    }
}

extern "C" fn kinithart(hartid: usize, sp: usize) -> ! {
    println!("Hello world from hart {hartid}: sp: {sp:#010x}");
    unsafe {
        init_hart(sp);
    }

    Scheduler::yield_hart()
}

#[naked]
extern "C" fn init_user_mode() -> ! {
    unsafe {
        core::arch::asm!(
            r"
            0:
            li  a0, 1000000000
            1:
            addi a0, a0, -1
            bne  zero, a0, 1b
            ecall
            j    0b
            ",
            options(noreturn)
        )
    }
}
