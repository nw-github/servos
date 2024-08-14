#![no_std]
#![no_main]
#![feature(naked_functions)]
#![feature(asm_const)]
#![feature(abi_riscv_interrupt)]
#![feature(fn_align)]
#![feature(allocator_api)]
#![feature(new_uninit)]
#![feature(stmt_expr_attributes)]
#![feature(slice_split_once)]
#![feature(try_with_capacity)]
#![feature(pointer_is_aligned_to)]
#![feature(slice_from_ptr_range)]
#![feature(ptr_sub_ptr)]
#![feature(cell_update)]
#![feature(maybe_uninit_slice)]
#![deny(unsafe_op_in_unsafe_fn)]

use alloc::sync::Arc;
use core::{
    alloc::Allocator,
    arch::asm,
    mem::MaybeUninit,
    ops::Range,
    ptr::{addr_of, addr_of_mut},
    sync::atomic::AtomicUsize,
};
use dev::{console::Console, null::NullDevice};
use fdt_rs::{
    base::{DevTree, DevTreeNode},
    prelude::{FallibleIterator, PropReader},
};
use fs::{
    dev::DeviceFs,
    initrd::InitRd,
    path::Path,
    vfs::{Vfs, VFS},
};
use hart::{get_hart_info, HartInfo, PowerManagement, HART_INFO, HART_STACK_LEN, MAX_HARTS, POWER};
use plic::PLIC;
use proc::{Process, Scheduler, USER_TRAP_FRAME};
use servos::{
    drivers::{Ns16550a, Syscon},
    heap::BlockAlloc,
    lock::SpinLocked,
    riscv::{self, disable_intr, r_satp, r_tp},
    sbi::{self, hsm::HartState},
    Align16,
};
use shared::io::OpenFlags;
use uart::{DebugIo, CONS};
use vmm::{Page, PageTable, Pte};

mod dev;
mod dump_fdt;
mod fs;
mod hart;
mod plic;
mod proc;
mod sys;
mod trap;
mod uart;
mod vmm;

extern crate alloc;

static mut BOOT_STACK: Align16<MaybeUninit<[u8; HART_STACK_LEN]>> = Align16(MaybeUninit::uninit());

#[global_allocator]
static ALLOCATOR: SpinLocked<BlockAlloc> = SpinLocked::new(BlockAlloc::new());

static mut KPAGETABLE: PageTable = PageTable::new();

static BOOT_HART: AtomicUsize = AtomicUsize::new(0);

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

    loop {
        unsafe {
            asm!("wfi", options(nomem, nostack));
        }
    }
}

#[naked]
#[no_mangle]
#[link_section = ".text.init"]
extern "C" fn _start(_hartid: usize, _fdt: *const u8) -> ! {
    unsafe {
        asm!(
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
            stack = sym BOOT_STACK,
            stack_len = const HART_STACK_LEN,
            options(noreturn),
        );
    }
}

#[naked]
extern "C" fn _start_hart(_hartid: usize, _satp: usize) -> ! {
    unsafe {
        asm!(
            r"
            .option push
            .option norelax
            la      gp, _global_pointer
            .option pop

            # enable virtual memory first, since sp will be a virtual address
            sfence.vma zero, zero
            csrw    satp, a1
            sfence.vma zero, zero

            # read the address of this harts sp from HART_INFO
            mv      tp, a0
            la      t0, {hart_info}
            li      t1, {hart_info_sz}
            mul     t1, tp, t1
            add     t0, t1, t0
            ld      sp, {info_sp_offs}(t0)

            tail    {init}",

            hart_info = sym HART_INFO,
            hart_info_sz = const core::mem::size_of::<HartInfo>(),
            info_sp_offs = const core::mem::offset_of!(HartInfo, sp),

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
        let kend = kend.add(kend.align_offset(Page::SIZE)); // align to next page
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

unsafe fn init_harts() {
    use alloc::alloc::{Global, Layout};

    let mut next_top = USER_TRAP_FRAME - Page::SIZE;

    let tp = r_tp();
    let pt = unsafe { &mut *addr_of_mut!(KPAGETABLE) };
    // TODO: maybe look in the device tree for hart count
    for (i, state) in unsafe { HART_INFO.iter_mut().enumerate() } {
        // TODO: make sure this hart is M + S + U mode capable
        if i != tp && !matches!(sbi::hsm::hart_get_status(i), Ok(HartState::Stopped)) {
            continue;
        }

        *state = HartInfo { sp: next_top };

        let bottom = next_top - HART_STACK_LEN;
        let stack = Global
            .allocate(unsafe { Layout::from_size_align_unchecked(HART_STACK_LEN, 16) })
            .expect("allocation failure allocating stack for a hart")
            .as_ptr();
        assert!(pt.map_pages(stack.into(), bottom, HART_STACK_LEN, Pte::Rw));
        // extra Page::SIZE for guard page
        next_top = bottom - Page::SIZE;
    }

    let satp = PageTable::make_satp(pt);
    for hartid in 0..MAX_HARTS {
        if matches!(sbi::hsm::hart_get_status(hartid), Ok(HartState::Stopped)) {
            if let Err(err) = sbi::hsm::hart_start(hartid, _start_hart, satp) {
                panic!("failed to start hart {hartid}: {err:?}");
            }
        }
    }
}

extern "C" fn kmain(hartid: usize, fdt: *const u8) -> ! {
    unsafe {
        println!("\n\n");

        BOOT_HART.store(hartid, core::sync::atomic::Ordering::SeqCst);

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
            addr_of!(BOOT_STACK),
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
        init_harts();

        // dump_fdt::dump_tree(dt).unwrap();

        _start_hart(hartid, PageTable::make_satp(addr_of!(KPAGETABLE)))
    }
}

extern "C" fn kinithart(hartid: usize) -> ! {
    println!("Hello world from hart {hartid}: sp: {}", get_hart_info().sp);

    if BOOT_HART.load(core::sync::atomic::Ordering::SeqCst) == hartid {
        let mut devices = DeviceFs::new();
        devices
            .add_device(Path::new("uart0").try_into().unwrap(), Arc::new(Console))
            .unwrap();
        devices
            .add_device(Path::new("null").try_into().unwrap(), Arc::new(NullDevice))
            .unwrap();

        static INITRD: &[u8] = include_bytes!("../../initrd.img");
        {
            let mut vfs = VFS.lock();
            vfs.mount(
                Path::new("/").try_into().unwrap(),
                Arc::new(InitRd::new(INITRD).unwrap()),
            )
            .unwrap();
            vfs.mount(Path::new("/dev").try_into().unwrap(), Arc::new(devices))
                .unwrap();
        }

        let root = Vfs::open("/", OpenFlags::empty()).unwrap();
        Process::spawn(Path::new("/bin/init"), root, &[]).expect("couldn't spawn init process");
    }

    // ask for PLIC interrupts
    PLIC.set_hart_priority_threshold(0);
    if let Some(irq) = PLIC.get_uart0() {
        PLIC.hart_enable(irq.into());
    }

    // enable traps and install the trap handler
    trap::hart_install();

    Scheduler::yield_hart()
}
