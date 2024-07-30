use core::{
    ops::{Index, IndexMut},
    sync::atomic::{AtomicU32, Ordering},
};

use crate::{
    trap::{return_to_user, USER_TRAP_VEC},
    vmm::{self, Page, PageTable, PhysAddr, VirtAddr, PGSIZE, PTE_R, PTE_U, PTE_W, PTE_X},
};
use alloc::{boxed::Box, vec::Vec};
use servos::riscv::{disable_intr, r_satp, r_tp};

static NEXTPID: AtomicU32 = AtomicU32::new(0);

pub enum ProcStatus {
    Idle,
    Running,
}

// return_to_user and user_trap_vec both rely on the exact layout of this struct
#[repr(C, align(0x1000))]
pub struct TrapFrame {
    pub regs: [usize; 32], // 0 is PC
    pub hartid: usize,
    pub ksatp: usize,
    pub ksp: *const u8,
    pub handle_trap: extern "C" fn(sepc: usize) -> !,
}

impl Index<Reg> for TrapFrame {
    type Output = usize;

    fn index(&self, index: Reg) -> &Self::Output {
        &self.regs[index as usize]
    }
}

impl IndexMut<Reg> for TrapFrame {
    fn index_mut(&mut self, index: Reg) -> &mut Self::Output {
        &mut self.regs[index as usize]
    }
}

pub struct Process {
    pub pid: u32,
    pub pagetable: Box<PageTable>,
    /// This can't be stored as a Box because it will be manipulated from user_trap_vec
    pub trapframe: *mut TrapFrame,
    pub status: ProcStatus,
    pub data: Vec<usize>,
}

pub static mut CURRENT_PROC: Option<Process> = None;

pub const USER_TRAP_FRAME: VirtAddr = VirtAddr(USER_TRAP_VEC.0 - PGSIZE);

impl Process {
    pub fn from_function(func: extern "C" fn() -> !) -> Option<Self> {
        const ENTRY_ADDR: usize = 0x4000_0000;
        const STACK_ADDR: usize = 0x7fff_f000;

        let func = PhysAddr(func as usize);
        let mut pt = PageTable::try_alloc()?;
        let stack = Box::into_raw(Page::alloc()?) as usize;
        let mut trapframe = Box::<TrapFrame>::try_new_zeroed()
            .map(|v| unsafe { v.assume_init() })
            .ok()?;

        let trapframe_p = &*trapframe as *const TrapFrame;
        if !crate::trap::map_trap_code(&mut pt)
            || !pt.map_page(trapframe_p.into(), USER_TRAP_FRAME, PTE_R | PTE_W)
            || !pt.map_page(func, VirtAddr(ENTRY_ADDR), PTE_R | PTE_X | PTE_U)
            || !pt.map_page(stack.into(), VirtAddr(STACK_ADDR), PTE_R | PTE_W | PTE_U)
        {
            return None;
        }

        trapframe[Reg::PC] = ENTRY_ADDR + vmm::page_offset(func);
        trapframe[Reg::SP] = STACK_ADDR + PGSIZE;
        trapframe.ksatp = r_satp();
        trapframe.handle_trap = handle_u_trap;

        let mut data = Vec::try_with_capacity(1).ok()?;
        data.push(stack);
        Some(Process {
            pid: NEXTPID.fetch_add(1, Ordering::SeqCst),
            pagetable: pt,
            trapframe: Box::into_raw(trapframe),
            status: ProcStatus::Idle,
            data,
        })
    }

    pub unsafe fn return_into(mut self) -> ! {
        // disable interrupts until we get back into user mode
        let token = disable_intr();

        self.status = ProcStatus::Running;
        unsafe {
            let satp = PageTable::make_satp(&*self.pagetable);
            (*self.trapframe).hartid = r_tp();
            (*self.trapframe).ksp = crate::KSTACK
                .0
                .as_ptr()
                .cast::<u8>()
                .add(crate::HART_STACK_LEN);

            CURRENT_PROC = Some(self);
            return_to_user(token, satp)
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        for &page in self.data.iter() {
            drop(unsafe { Box::from_raw(page as *mut Page) });
        }

        drop(unsafe { Box::from_raw(self.trapframe) })
    }
}

#[repr(usize)]
#[allow(unused)]
pub enum Reg {
    PC, // x0 is the zero register, so use this slot to store the normally inaccessibly PC
    RA,
    SP,
    GP,
    TP,
    T0,
    T1,
    T2,
    S0,
    S1,
    A0,
    A1,
    A2,
    A3,
    A4,
    A5,
    A6,
    A7,
    S2,
    S3,
    S4,
    S5,
    S6,
    S7,
    S8,
    S9,
    S10,
    S11,
    T3,
    T4,
    T5,
    T6,
}

extern "C" fn handle_u_trap(sepc: usize) -> ! {
    let _token = disable_intr();
    unsafe {
        let mut proc = CURRENT_PROC.take().unwrap();
        (*proc.trapframe)[Reg::PC] = crate::trap::handle_trap(sepc, Some(&mut proc));
        proc.return_into();
    }
}
