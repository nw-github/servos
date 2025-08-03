use servos::{
    riscv::{r_sstatus, r_stval, r_tp, w_sstatus, SSTATUS_SPIE, SSTATUS_SPP},
    sbi,
};

use crate::{
    plic::PLIC,
    println,
    proc::{ProcStatus, Process, ProcessNode, Reg, Scheduler, USER_TRAP_FRAME},
    riscv::{
        enable_intr, r_scause, r_time, w_sie, w_stvec, InterruptToken, SIE_SEIE, SIE_SSIE, SIE_STIE,
    },
    sys,
    uart::CONS,
    vmm::{self, Page, PageTable, Pte, VirtAddr},
    CONSOLE_DEV,
};

const INTERRUPT_FLAG_BIT: usize = 1 << (usize::BITS - 1);

#[repr(usize)]
#[derive(strum::FromRepr, Debug)]
#[allow(clippy::enum_clike_unportable_variant)]
pub enum TrapCause {
    // Interrupts
    SoftwareIntr = 1 | INTERRUPT_FLAG_BIT,
    TimerIntr = 5 | INTERRUPT_FLAG_BIT,
    ExternalIntr = 9 | INTERRUPT_FLAG_BIT,
    CounterOverflowIntr = 13 | INTERRUPT_FLAG_BIT,
    // Exceptions
    InstrAddrMisaligned = 0,
    InstrAccessFault,
    IllegalInstr,
    Breakpoint,
    LoadMisaligned,
    LoadAccessFault,
    StoreAddrMisaligned,
    StoreAccessFault,
    EcallFromUMode,
    EcallFromSMode,
    InstrPageFault = 12,
    LoadPageFault,
    StorePageFault = 15,
    SoftwareCheck = 18,
    HardwareError = 19,
}

impl TrapCause {
    pub fn current() -> Result<TrapCause, usize> {
        let cause = r_scause();
        TrapCause::from_repr(cause & (INTERRUPT_FLAG_BIT | 0xff)).ok_or(cause)
    }
}

pub const USER_TRAP_VEC: VirtAddr = VirtAddr(VirtAddr::MAX.0 - Page::SIZE);
pub const TIMER_INTERVAL: usize = 10_000_000 / 2;

#[unsafe(naked)]
#[link_section = ".text.trap"]
extern "C" fn user_trap_vec() {
    unsafe {
        core::arch::naked_asm!(
            r"
            .align 4
            csrrw t0, sscratch, t0

            sd   ra, 0x08(t0)
            sd   sp, 0x10(t0)
            sd   gp, 0x18(t0)
            sd   tp, 0x20(t0)

            sd   t1, 0x30(t0)
            sd   t2, 0x38(t0)
            sd   s0, 0x40(t0)
            sd   s1, 0x48(t0)
            sd   a0, 0x50(t0)
            sd   a1, 0x58(t0)
            sd   a2, 0x60(t0)
            sd   a3, 0x68(t0)
            sd   a4, 0x70(t0)
            sd   a5, 0x78(t0)
            sd   a6, 0x80(t0)
            sd   a7, 0x88(t0)
            sd   s2, 0x90(t0)
            sd   s3, 0x98(t0)
            sd   s4, 0xa0(t0)
            sd   s5, 0xa8(t0)
            sd   s6, 0xb0(t0)
            sd   s7, 0xb8(t0)
            sd   s8, 0xc0(t0)
            sd   s9, 0xc8(t0)
            sd   s10, 0xd0(t0)
            sd   s11, 0xd8(t0)
            sd   t3, 0xe0(t0)
            sd   t4, 0xe8(t0)
            sd   t5, 0xf0(t0)
            sd   t6, 0xf8(t0)

            csrr t1, sscratch
            sd   t1, 0x28(t0)           # save t0 as well

            csrr a0, sepc
            sd   a0, 0x00(t0)           # load previous PC into TrapFrame::regs[0]

            ld         a1, {proc}(t0)
            ld         tp, {hartid}(t0)          # load kernel hartid
            ld         sp, {stack}(t0)
            ld         ra, {handle}(t0)
            ld         t1, {satp}(t0)    # load kernel SATP and switch to kernel page table
            sfence.vma zero, zero
            csrw       satp, t1
            sfence.vma zero, zero

            jr ra
            ",
            satp = const core::mem::offset_of!(crate::proc::TrapFrame, ksatp),
            hartid = const core::mem::offset_of!(crate::proc::TrapFrame, hartid),
            stack = const core::mem::offset_of!(crate::proc::TrapFrame, ksp),
            handle = const core::mem::offset_of!(crate::proc::TrapFrame, handle_trap),
            proc = const core::mem::offset_of!(crate::proc::TrapFrame, proc),
        );
    }
}

#[unsafe(naked)]
#[link_section = ".text.trap"]
extern "C" fn __return_to_user(satp: usize) -> ! {
    unsafe {
        core::arch::naked_asm!(
            r"
            li   t0, {trap_frame}
            csrw sscratch, t0

            sfence.vma zero, zero
            csrw satp, a0
            sfence.vma zero, zero       # switch to user page table

            ld   t1,  0x00(t0)
            csrw sepc, t1               # restore PC

            ld   ra,  0x08(t0)
            ld   sp,  0x10(t0)
            ld   gp,  0x18(t0)
            ld   tp,  0x20(t0)

            ld   t1,  0x30(t0)
            ld   t2,  0x38(t0)
            ld   s0,  0x40(t0)
            ld   s1,  0x48(t0)
            ld   a0,  0x50(t0)
            ld   a1,  0x58(t0)
            ld   a2,  0x60(t0)
            ld   a3,  0x68(t0)
            ld   a4,  0x70(t0)
            ld   a5,  0x78(t0)
            ld   a6,  0x80(t0)
            ld   a7,  0x88(t0)
            ld   s2,  0x90(t0)
            ld   s3,  0x98(t0)
            ld   s4,  0xa0(t0)
            ld   s5,  0xa8(t0)
            ld   s6,  0xb0(t0)
            ld   s7,  0xb8(t0)
            ld   s8,  0xc0(t0)
            ld   s9,  0xc8(t0)
            ld   s10, 0xd0(t0)
            ld   s11, 0xd8(t0)
            ld   t3,  0xe0(t0)
            ld   t4,  0xe8(t0)
            ld   t5,  0xf0(t0)
            ld   t6,  0xf8(t0)

            ld   t0,  0x28(t0)
            sret
            ",
            trap_frame = const USER_TRAP_FRAME.0,
        )
    }
}

#[repr(align(4))]
extern "riscv-interrupt-s" fn sv_trap_vec() {
    // FIXME: if the kernel overruns its stack, it will cause a page fault that brings it to this
    // routine. However, the first thing we do here (courtesy of the riscv-interrupt-s calling conv)
    // is backup all registers on the stack, which will cause a double-fault in this case.
    match TrapCause::current() {
        Ok(TrapCause::ExternalIntr) => handle_external_intr(),
        Ok(TrapCause::TimerIntr) => {
            _ = sbi::timer::set_timer(r_time() + TIMER_INTERVAL);
        }
        Ok(ex) => panic!("Unhandled trap: {ex:?}"),
        Err(cause) => panic!("Unhandled trap: unknown {cause:#x}"),
    }
}

pub extern "C" fn handle_u_trap(mut sepc: usize, paddr: ProcessNode) -> ! {
    w_stvec(sv_trap_vec as usize);

    let mut must_yield = false;
    let proc = unsafe { paddr.0.as_ref() };
    match TrapCause::current() {
        Ok(TrapCause::ExternalIntr) => handle_external_intr(),
        Ok(TrapCause::TimerIntr) => {
            _ = sbi::timer::set_timer(r_time() + TIMER_INTERVAL);
            must_yield = true;
        }
        Ok(TrapCause::EcallFromUMode) => {
            sys::handle_syscall(proc);
            sepc += 4;
        }
        Ok(
            cause @ (TrapCause::LoadPageFault
            | TrapCause::StorePageFault
            | TrapCause::InstrPageFault),
        ) => {
            let pid = proc.with(|mut proc| {
                proc.kill(None);
                proc.pid
            });

            println!(
                "PID {pid} page fault ({cause:?}) on hart {} at address {:#x}",
                r_tp(),
                r_stval(),
            );
        }
        Ok(unk) => {
            let pid = proc.with(|mut proc| {
                proc.kill(None);
                proc.pid
            });
            println!(
                "ETrap from process with PID {pid} on hart {}: exception {unk:?} raised, killing process",
                r_tp(),
            );
        }
        Err(cause) => panic!("Unhandled trap: no match for cause {cause:#x}"),
    }

    proc.with(|mut proc| {
        proc.trapframe()[Reg::PC] = sepc;
        unsafe {
            if let Some(ecode) = proc.killed {
                paddr.destroy(proc, ecode); // proc is invalidated here
            } else if !must_yield && !matches!(proc.status, ProcStatus::Waiting(_)) {
                Process::resume(proc);
            } else if !Scheduler::take(paddr) {
                println!("Scheduler::take failed for PID {}, OOM!", proc.pid);
                paddr.destroy(proc, usize::MAX);
                /* OOM */
            }
        }
    });
    Scheduler::yield_hart()
}

pub fn hart_install() {
    w_stvec(sv_trap_vec as usize);
    w_sie(SIE_SEIE | SIE_STIE | SIE_SSIE);
    unsafe { enable_intr() };

    sbi::timer::set_timer(r_time() + TIMER_INTERVAL).expect("SBI Timer support is not present");
}

pub fn map_trap_code(pt: &mut PageTable) -> bool {
    pt.map_pages(
        vmm::page_number(user_trap_vec as usize).into(),
        USER_TRAP_VEC,
        Page::SIZE,
        Pte::Rx,
    )
}

pub fn return_to_user(_token: InterruptToken, satp: usize) -> ! {
    #[allow(unused_assignments)]
    let mut __ret = __return_to_user as extern "C" fn(usize) -> !; // just for type inference

    #[allow(clippy::missing_transmute_annotations)]
    {
        // we must __return_to_user through the commonly mapped USER_TRAP_VEC page, so the code is
        // still there when satp is switched to the user table
        let return_to_user = USER_TRAP_VEC.0 + vmm::page_offset(__return_to_user as usize);
        __ret = unsafe { core::mem::transmute(return_to_user as *const ()) };
    }

    w_stvec(USER_TRAP_VEC.0 + vmm::page_offset(user_trap_vec as usize));
    w_sstatus(r_sstatus() & !(SSTATUS_SPP) | SSTATUS_SPIE); // set user mode, enable interrupts in user mode
    __ret(satp);
}

fn handle_external_intr() {
    let irq = PLIC.hart_claim();
    let Some(num) = irq.value() else {
        return;
    };

    if irq.is_uart0() {
        let Some(ch) = CONS.lock().read() else {
            // println!("Got spurious uart0 interrupt");
            return;
        };
        // println!("UART interrupt: {ch:#04x} ({})", ch as char);
        unsafe {
            if !CONSOLE_DEV.get().unwrap().put(ch) {
                CONS.lock().put(0x07); // ASCII BEL
            }
        }
    } else {
        println!("PLIC interrupt with unknown irq {num:#x}");
    }
}
