use core::{
    ops::{Index, IndexMut},
    ptr::{addr_of, addr_of_mut, NonNull},
    sync::atomic::{AtomicU32, Ordering},
};

use crate::{
    fs::{
        path::Path,
        vfs::{Fd, Vfs},
    },
    hart::get_hart_info,
    trap::{self, USER_TRAP_VEC},
    vmm::{Page, PageTable, Pte, VirtAddr},
};
use alloc::{boxed::Box, collections::VecDeque, vec::Vec};
use servos::{
    arr::HoleArray,
    elf::{ElfFile, PF_W, PF_X, PT_LOAD},
    lock::{Guard, SpinLocked},
    riscv::{enable_intr, r_tp},
};
use shared::{io::OpenFlags, sys::SysError};

static NEXTPID: AtomicU32 = AtomicU32::new(0);

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum ProcStatus {
    Idle,
    Running,
    Waiting(u32),
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
        let mypid = lock.pid;
        let _token = Guard::forget_and_keep_token(lock);
        let mut list = PROC_LIST.lock();
        if let Some(i) = list.iter().position(|&rhs| rhs == self) {
            list.swap_remove_back(i);
        }

        for proc in list.iter() {
            unsafe {
                proc.with(|mut proc| {
                    if proc.status == ProcStatus::Waiting(mypid) {
                        proc.status = ProcStatus::Idle;
                    }
                })
            }
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
    trapframe: *mut TrapFrame,
    pub status: ProcStatus,
    pub killed: bool,
    pub files: HoleArray<Fd, 32>,
    pub cwd: Fd,
    pub brk: VirtAddr,
}

pub const USER_TRAP_FRAME: VirtAddr = VirtAddr(USER_TRAP_VEC.0 - Page::SIZE);

impl Process {
    pub fn spawn(path: &Path, cwd: Fd, args: &[&[u8]]) -> Result<u32, SysError> {
        let file = Vfs::open_in_cwd(&cwd, path, OpenFlags::empty())?;
        let stat = file.stat()?;
        let mut buf = Vec::try_with_capacity(stat.size)?;
        let Some(file) = ElfFile::new(file.read(u64::MAX, buf.spare_capacity_mut())?) else {
            return Err(SysError::InvalidArgument);
        };

        let mut pt = PageTable::try_alloc()?;
        let mut trapframe_page = Page::zeroed()?;
        let trapframe = &mut *trapframe_page as *mut _ as *mut TrapFrame;
        if !trap::map_trap_code(&mut pt)
            || !pt.map_owned_page(trapframe_page, USER_TRAP_FRAME, Pte::Rw)
        {
            return Err(SysError::NoMem);
        }

        let mut highest_va = VirtAddr(0);
        for phdr in file.pheaders.iter() {
            if phdr.typ != PT_LOAD {
                continue;
            } else if phdr.memsz < phdr.filesz {
                return Err(SysError::InvalidArgument);
            }

            let mut perms = Pte::U | Pte::R;
            if phdr.flags & PF_W != 0 {
                perms |= Pte::W;
            }
            if phdr.flags & PF_X != 0 {
                perms |= Pte::X;
            }
            let base = VirtAddr(phdr.vaddr as usize);
            if !pt.map_new_pages(base, phdr.memsz as usize, perms, false) {
                return Err(SysError::NoMem);
            }

            let filesz = phdr.filesz as usize;
            base.copy_to(
                &pt,
                &file.raw[phdr.offset as usize..][..filesz],
                Some(Pte::empty()),
            )?;

            (base + filesz).iter_phys(&pt, (phdr.memsz - phdr.filesz) as usize, perms).zero();

            highest_va = highest_va.max(base + phdr.memsz as usize);
        }

        // allocate 1MiB of stack
        const STACK_SZ: usize = 0x100000;

        let mut sp = USER_TRAP_FRAME - Page::SIZE;
        if !pt.map_new_pages(sp - STACK_SZ, STACK_SZ, Pte::Urw, true) {
            return Err(SysError::NoMem);
        }

        let mut ptrs = Vec::try_with_capacity(args.len() + 1)?;
        for arg in core::iter::once(path.as_ref()).chain(args.iter().cloned()) {
            // stack is already zeroed, so just add 1 for the null terminator
            sp.0 -= arg.len() + 1;
            sp.copy_to(&pt, arg, None)?;
            ptrs.push(sp);
        }

        sp = VirtAddr(sp.0 & !(core::mem::align_of::<VirtAddr>() - 1));
        for &arg in ptrs.iter().rev() {
            sp.0 -= core::mem::size_of::<VirtAddr>();
            sp.copy_type_to(&pt, &arg, None)?;
        }

        let pid = NEXTPID.fetch_add(1, Ordering::Relaxed);
        let success = Self::enqueue_process(unsafe {
            let proc = ProcessNode(NonNull::new_unchecked(Box::into_raw(Box::try_new(
                SpinLocked::new(Process {
                    pid,
                    pagetable: pt,
                    trapframe,
                    status: ProcStatus::Idle,
                    killed: false,
                    files: HoleArray::empty(),
                    cwd,
                    brk: highest_va.next_page(),
                }),
            )?)));

            addr_of_mut!((*trapframe).proc).write(proc);
            addr_of_mut!((*trapframe).handle_trap).write(trap::handle_u_trap);
            (*trapframe).ksatp = PageTable::make_satp(addr_of!(crate::KPAGETABLE));
            (*trapframe).regs[Reg::PC as usize] = file.ehdr.entry as usize;
            (*trapframe).regs[Reg::SP as usize] = sp.0;
            (*trapframe).regs[Reg::A0 as usize] = args.len() + 1;
            (*trapframe).regs[Reg::A1 as usize] = sp.0;

            proc
        });

        success.then_some(pid).ok_or(SysError::NoMem)
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

    pub fn trapframe(&mut self) -> &mut TrapFrame {
        // Safety: Process owns the trapframe, but because it can be modified from user_trap_vec
        // it can't be stored as an owned Box
        unsafe { &mut *self.trapframe }
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
        if let Some((next, mut sched)) = SCHEDULER
            .try_lock()
            .and_then(|mut s| s.awaiting.pop_front().zip(Some(s)))
        {
            unsafe {
                next.with(|proc| {
                    if !matches!(proc.status, ProcStatus::Waiting(_)) {
                        drop(sched);
                        Process::return_into(proc);
                    } else {
                        sched.awaiting.push_back(next);
                    }
                });
            }
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
