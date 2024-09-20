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
    trap::{self, USER_TRAP_VEC},
    vmm::{Page, PageTable, Pte, User, VirtAddr},
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
    pub unsafe fn destroy(self, lock: Guard<Process>, ecode: usize) {
        let mypid = lock.pid;
        if mypid == 0 {
            panic!("return from the init process");
        }

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
                        proc.trapframe()[Reg::A0] = ecode;
                        proc.trapframe()[Reg::A1] = 0;
                    }
                })
            }
        }
        unsafe { self.free() };
    }

    unsafe fn free(self) {
        drop(unsafe { Box::from_raw(self.0.as_ptr()) });
    }
}

pub struct Process {
    pub pid: u32,
    pub status: ProcStatus,
    pub files: HoleArray<Fd, 32>,
    pub cwd: Fd,
    pub brk: VirtAddr,
    pub killed: Option<usize>,
    pagetable: *mut PageTable,
    trapframe: *mut TrapFrame,
}

pub const USER_TRAP_FRAME: VirtAddr = VirtAddr(USER_TRAP_VEC.0 - Page::SIZE);
pub const HART_STACK_LEN: usize = Page::SIZE * 4;
pub const HART_FIRST_STACK: VirtAddr = VirtAddr(USER_TRAP_FRAME.0 - Page::SIZE);

const USER_STACK_SZ: usize = 1024 * 1024;

impl Process {
    pub fn spawn(path: &Path, cwd: Fd, args: &[&[u8]]) -> Result<u32, SysError> {
        let file = Vfs::open_in_cwd(&cwd, path, OpenFlags::empty())?;
        let stat = file.stat()?;
        let mut buf = Vec::try_with_capacity(stat.size)?;
        let Some(file) = ElfFile::new(file.read(0, buf.spare_capacity_mut())?) else {
            return Err(SysError::BadArg);
        };

        let mut pt = PageTable::alloc()?;
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
                return Err(SysError::BadArg);
            }

            let mut perms = Pte::U | Pte::R;
            if phdr.flags & PF_W != 0 {
                perms |= Pte::W;
            }
            if phdr.flags & PF_X != 0 {
                perms |= Pte::X;
            }
            let base = User::from(phdr.vaddr as usize);
            if !pt.map_new_pages(base.addr(), phdr.memsz as usize, perms, false) {
                return Err(SysError::NoMem);
            }

            let filesz = phdr.filesz as usize;
            base.write_arr(
                &pt,
                &file.raw[phdr.offset as usize..][..filesz],
                Some(Pte::empty()),
            )?;

            base.add(filesz)
                .addr()
                .iter_phys(&pt, (phdr.memsz - phdr.filesz) as usize, perms)
                .zero();

            highest_va = highest_va.max(base.add(phdr.memsz as usize).addr());
        }

        let mut sp = User::from(USER_TRAP_FRAME - Page::SIZE);
        if !pt.map_new_pages(sp.sub(USER_STACK_SZ).addr(), USER_STACK_SZ, Pte::Urw, true) {
            return Err(SysError::NoMem);
        }

        let mut ptrs = Vec::try_with_capacity(args.len() + 1)?;
        for arg in core::iter::once(path.as_ref()).chain(args.iter().cloned()) {
            // stack is already zeroed, so just add 1 for the null terminator
            sp = sp.sub(arg.len() + 1);
            sp.write_arr(&pt, arg, None)?;
            ptrs.push(sp);
        }

        let mut sp = User::from(sp.addr().0 & !(core::mem::align_of::<VirtAddr>() - 1));
        for arg in ptrs.iter().rev() {
            sp = sp.sub(1);
            sp.write(&pt, arg)?;
        }

        let pid = NEXTPID.fetch_add(1, Ordering::Relaxed);
        let proc = Box::try_new(SpinLocked::new(Process {
            pid,
            pagetable: Box::into_raw(pt),
            trapframe,
            status: ProcStatus::Idle,
            killed: None,
            files: HoleArray::empty(),
            cwd,
            brk: highest_va,
        }))?;
        let success = Self::enqueue_process(unsafe {
            let proc = ProcessNode(NonNull::new_unchecked(Box::into_raw(proc)));

            addr_of_mut!((*trapframe).proc).write(proc);
            addr_of_mut!((*trapframe).handle_trap).write(trap::handle_u_trap);
            (*trapframe).ksatp = PageTable::make_satp(addr_of!(crate::KPAGETABLE));
            (*trapframe)[Reg::PC] = file.ehdr.entry as usize;
            (*trapframe)[Reg::SP] = sp.addr().0;
            (*trapframe)[Reg::A0] = args.len() + 1;
            (*trapframe)[Reg::A1] = sp.addr().0;

            proc
        });

        success.then_some(pid).ok_or(SysError::NoMem)
    }

    pub unsafe fn resume(mut this: Guard<Process>) -> ! {
        this.status = ProcStatus::Running;
        this.trapframe().hartid = r_tp();
        this.trapframe().ksp = hart_stack_top(r_tp()).0 as *mut u8;
        let satp = PageTable::make_satp(this.pagetable());
        trap::return_to_user(Guard::drop_and_keep_token(this), satp)
    }

    pub fn trapframe(&mut self) -> &mut TrapFrame {
        // Safety: the pagetable owns the trapframe page
        unsafe { &mut *self.trapframe }
    }

    pub fn pagetable(&self) -> &PageTable {
        // Safety: Process owns the pagetable, but because page table entries can be modified by the
        // cpu it can't be stored as an owned Box
        unsafe { &*self.pagetable }
    }

    pub fn pagetable_mut(&mut self) -> &mut PageTable {
        unsafe { &mut *self.pagetable }
    }

    pub fn kill(&mut self, code: Option<usize>) {
        self.killed = Some(code.unwrap_or(usize::MAX));
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
        drop(unsafe { Box::from_raw(self.pagetable) });
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
                        Process::resume(proc);
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

pub const fn hart_stack_top(hart: usize) -> VirtAddr {
    // extra Page::SIZE for guard page
    VirtAddr(HART_FIRST_STACK.0 - (hart * (HART_STACK_LEN + Page::SIZE)))
}
