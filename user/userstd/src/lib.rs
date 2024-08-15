#![no_std]

pub mod mem;
pub mod print;
pub mod sys;

use io::OpenFlags;
pub use shared::*;

pub extern crate alloc;

#[panic_handler]
fn on_panic(info: &core::panic::PanicInfo) -> ! {
    println!("panic from init: {info}");

    _ = sys::kill(sys::getpid());
    loop {}
}

extern "Rust" {
    fn main(args: &[*const u8]) -> usize;
}

#[no_mangle]
extern "C" fn _start(argc: usize, argv: *const *const u8) {
    // open stdout and stdin
    _ = sys::open("/dev/uart0", OpenFlags::ReadWrite).unwrap();
    _ = sys::open("/dev/uart0", OpenFlags::empty()).unwrap();

    let bottom = sys::sbrk(0).unwrap();
    let top = sys::sbrk(1024 * 1024).expect("sbrk failed"); // ask for 1mib of heap

    #[allow(deref_nullptr)]
    unsafe {
        mem::init(bottom, top);

        let _code = main(if argc != 0 {
            core::slice::from_raw_parts(argv, argc)
        } else {
            &[]
        });
        // TODO: call exit
        _ = sys::kill(sys::getpid());
        *core::ptr::null_mut::<u8>() = 0;
    }
}
