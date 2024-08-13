use core::{
    ops::{Index, IndexMut},
    ptr::{addr_of, NonNull},
    sync::atomic::{AtomicU32, Ordering},
};

use crate::{
    fs::vfs::FileRef,
    hart::get_hart_info,
    trap::{self, USER_TRAP_VEC},
    vmm::{self, page_number, Page, PageTable, PhysAddr, Pte, VirtAddr, PGSIZE},
};
use alloc::{boxed::Box, collections::VecDeque};
use servos::{
    arr::HoleArray,
    elf::{ElfFile, PF_W, PF_X, PT_LOAD},
    lock::{Guard, SpinLocked},
    riscv::{enable_intr, r_tp},
};

static NEXTPID: AtomicU32 = AtomicU32::new(0);

pub enum ProcStatus {
    Idle,
    Running,
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

// return_to_user and user_trap_vec both rely on the exact layout of this struct
#[repr(C, align(0x1000))]
pub struct TrapFrame {
    pub regs: [usize; 32], // 0 is PC
    pub hartid: usize,
    pub ksatp: usize,
    pub ksp: *mut u8,
    pub handle_trap: extern "C" fn(sepc: usize, proc: ProcessNode) -> !,
    pub proc: ProcessNode,
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

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ProcessNode(pub NonNull<SpinLocked<Process>>);

impl ProcessNode {
    pub unsafe fn with<T>(self, f: impl FnOnce(Guard<Process>) -> T) -> T {
        f(unsafe { self.0.as_ref() }.lock())
    }

    /// # Safety
    /// The process must not be awaiting scheduling or running on any hart.
    pub unsafe fn destroy(self, lock: Guard<Process>) {
        let _token = Guard::forget_and_keep_token(lock);
        let mut list = PROC_LIST.lock();
        if let Some(i) = list.iter().position(|&rhs| rhs == self) {
            list.swap_remove_back(i);
        }
        unsafe { self.free() };
    }

    unsafe fn free(self) {
        let me = unsafe { Box::from_raw(self.0.as_ptr()) };
        drop(me);
    }
}

pub struct Process {
    pub pid: u32,
    pub pagetable: Box<PageTable>,
    /// This can't be stored as a Box because it will be manipulated from user_trap_vec
    pub trapframe: *mut TrapFrame,
    pub status: ProcStatus,
    pub killed: bool,
    pub files: HoleArray<FileRef, 32>,
}

pub const USER_TRAP_FRAME: VirtAddr = VirtAddr(USER_TRAP_VEC.0 - PGSIZE);

impl Process {
    pub fn spawn_from_function(func: extern "C" fn() -> !) -> Option<()> {
        const ENTRY_ADDR: usize = 0x4000_0000;
        const STACK_ADDR: usize = 0x7fff_f000;

        let func = PhysAddr(func as usize);
        let mut pt = PageTable::try_alloc()?;
        let mut trapframe_page = Page::zeroed()?;
        let trapframe = &mut *trapframe_page as *mut _ as *mut TrapFrame;
        if !trap::map_trap_code(&mut pt)
            || !pt.map_owned_page(trapframe_page, USER_TRAP_FRAME, Pte::Rw)
            || !pt.map_owned_page(Page::uninit()?, VirtAddr(STACK_ADDR), Pte::Urw)
            || !pt.map_page(func, VirtAddr(ENTRY_ADDR), Pte::Urx)
        {
            return None;
        }

        Self::enqueue_process(unsafe {
            let proc = ProcessNode(NonNull::new_unchecked(Box::into_raw(
                Box::try_new(SpinLocked::new(Process {
                    pid: NEXTPID.fetch_add(1, Ordering::Relaxed),
                    pagetable: pt,
                    trapframe,
                    status: ProcStatus::Idle,
                    killed: false,
                    files: HoleArray::empty(),
                }))
                .ok()?,
            )));

            (*trapframe).proc = proc;
            (*trapframe).handle_trap = trap::handle_u_trap;
            (*trapframe).ksatp = PageTable::make_satp(addr_of!(crate::KPAGETABLE));
            (*trapframe).ksp = core::ptr::null_mut();
            (*trapframe).hartid = 0;
            (*trapframe).regs[Reg::PC as usize] = ENTRY_ADDR + vmm::page_offset(func.0);
            (*trapframe).regs[Reg::SP as usize] = STACK_ADDR + PGSIZE;

            proc
        })
        .then_some(())
    }

    pub fn spawn(file: &ElfFile) -> Option<()> {
        let mut pt = PageTable::try_alloc()?;
        let mut trapframe_page = Page::zeroed()?;
        let trapframe = &mut *trapframe_page as *mut _ as *mut TrapFrame;
        if !trap::map_trap_code(&mut pt)
            || !pt.map_owned_page(trapframe_page, USER_TRAP_FRAME, Pte::Rw)
        {
            return None;
        }

        let mut highest_va = 0;
        for phdr in file.pheaders.iter() {
            if phdr.typ != PT_LOAD {
                continue;
            } else if phdr.memsz < phdr.filesz {
                return None;
            }

            let mut perms = Pte::U | Pte::R;
            if phdr.flags & PF_W != 0 {
                perms |= Pte::W;
            }
            if phdr.flags & PF_X != 0 {
                perms |= Pte::X;
            }
            let base = phdr.vaddr as usize;
            highest_va = highest_va.max(base);
            let [first, last] = [
                page_number(base),
                page_number(base.wrapping_add(phdr.memsz as usize) - 1),
            ];
            if last < first {
                return None;
            }

            // this is all somewhat suboptimal
            for page in (first..=last).step_by(PGSIZE) {
                if !pt.map_owned_page(Page::zeroed()?, VirtAddr(page), perms) {
                    return None;
                }
            }

            if !VirtAddr(base).ucopy_to(
                &pt,
                &file.raw[phdr.offset as usize..][..phdr.filesz as usize],
                Some(Pte::empty()),
            ) {
                return None;
            }
        }

        // allocate 1MiB of stack
        let stack_top = page_number(highest_va) + PGSIZE;
        for page in (stack_top + PGSIZE..=stack_top + 0x100000 + PGSIZE).step_by(PGSIZE) {
            if !pt.map_owned_page(Page::uninit()?, VirtAddr(page), Pte::Urw) {
                return None;
            }
        }

        Self::enqueue_process(unsafe {
            let proc = ProcessNode(NonNull::new_unchecked(Box::into_raw(
                Box::try_new(SpinLocked::new(Process {
                    pid: NEXTPID.fetch_add(1, Ordering::Relaxed),
                    pagetable: pt,
                    trapframe,
                    status: ProcStatus::Idle,
                    killed: false,
                    files: HoleArray::empty(),
                }))
                .ok()?,
            )));

            (*trapframe).proc = proc;
            (*trapframe).handle_trap = trap::handle_u_trap;
            (*trapframe).ksatp = PageTable::make_satp(addr_of!(crate::KPAGETABLE));
            (*trapframe).ksp = core::ptr::null_mut();
            (*trapframe).hartid = 0;
            (*trapframe).regs[Reg::PC as usize] = file.ehdr.entry as usize;
            (*trapframe).regs[Reg::SP as usize] = stack_top;

            proc
        })
        .then_some(())
    }

    pub unsafe fn return_into(mut this: Guard<Process>) -> ! {
        this.status = ProcStatus::Running;
        unsafe {
            let satp = PageTable::make_satp(&*this.pagetable);
            (*this.trapframe).hartid = r_tp();
            (*this.trapframe).ksp = get_hart_info().sp.0 as *mut u8;
            trap::return_to_user(Guard::drop_and_keep_token(this), satp)
        }
    }

    fn enqueue_process(proc: ProcessNode) -> bool {
        let mut proc_list = PROC_LIST.lock();
        if !try_push_back(&mut proc_list, proc) {
            unsafe { proc.free() };
            false
        } else if !Scheduler::take(proc) {
            proc_list.pop_back();
            unsafe { proc.free() };
            false
        } else {
            true
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        drop(unsafe { Box::from_raw(self.trapframe) })
    }
}

// maybe this should be a semaphore or something
static SCHEDULER: SpinLocked<Scheduler> = SpinLocked::new(Scheduler::new());
pub static PROC_LIST: SpinLocked<VecDeque<ProcessNode>> = SpinLocked::new(VecDeque::new());

pub struct Scheduler {
    awaiting: VecDeque<ProcessNode>,
}

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            awaiting: VecDeque::new(),
        }
    }

    pub fn try_find_execute() {
        // can't use if/let or we will hold the scheduler lock forever and deadlock
        let Some(next) = SCHEDULER
            .try_lock()
            .and_then(|mut s| s.awaiting.pop_front())
        else {
            return;
        };

        unsafe {
            next.with(|proc| Process::return_into(proc));
        }
    }

    pub fn take(proc: ProcessNode) -> bool {
        try_push_back(&mut SCHEDULER.lock().awaiting, proc)
    }

    pub fn yield_hart() -> ! {
        unsafe { enable_intr() };
        loop {
            Self::try_find_execute();
        }
    }
}

fn try_push_back<T>(vec: &mut VecDeque<T>, item: T) -> bool {
    if vec.try_reserve(1).is_err() && vec.try_reserve_exact(1).is_err() {
        return false;
    }

    vec.push_back(item);
    true
}
