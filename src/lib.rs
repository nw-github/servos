#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
#![feature(const_mut_refs)]
#![feature(asm_const)]

pub mod lock;
pub mod riscv;
pub mod sbi;
pub mod drivers;
pub mod heap;

#[repr(C, align(16))]
pub struct Align16<T>(pub T);
