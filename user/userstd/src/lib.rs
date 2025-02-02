#![no_std]

pub mod io;
pub mod mem;
pub mod sys;

use shared::io::OpenFlags;

pub extern crate alloc;

#[panic_handler]
fn on_panic(info: &core::panic::PanicInfo) -> ! {
    println!("panic: {info}");

    _ = sys::kill(sys::getpid());

    #[allow(deref_nullptr)]
    unsafe {
        *core::ptr::null_mut::<u8>() = 0;
    }
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
    let top = sys::sbrk(1024 * 512).expect("sbrk failed");

    unsafe {
        mem::init(bottom, top);
        sys::exit(main(core::slice::from_raw_parts(argv, argc))).expect("exit shouldn't return");
    }
}
