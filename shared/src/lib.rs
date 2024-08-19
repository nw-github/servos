#![no_std]
#![feature(allocator_api)]

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod io;
pub mod sys;
