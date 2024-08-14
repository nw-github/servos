#![no_std]
#![no_main]

use shared::io::OpenFlags;

pub mod print;
pub mod sys;

#[panic_handler]
fn on_panic(info: &core::panic::PanicInfo) -> ! {
    println!("panic from init: {info}");

    _ = sys::kill(sys::getpid());
    loop {}
}

static mut GLOBAL_STATIC: usize = 5;

#[link_section = ".bss"]
static ZEROED: [u8; 0x2000] = [0; 0x2000];

#[no_mangle]
pub extern "C" fn _start() {
    // open stdout and stdin
    _ = sys::open("/dev/uart0", OpenFlags::ReadWrite).unwrap();
    _ = sys::open("/dev/uart0", OpenFlags::empty()).unwrap();
    println!("\n\nHello world from the init process!");

    unsafe {
        assert_eq!(GLOBAL_STATIC, 5);
        while GLOBAL_STATIC != 0 {
            GLOBAL_STATIC -= 1;
        }
        assert_eq!(GLOBAL_STATIC, 0);
    }

    {
        let fd = sys::open("/1001_A.txt", OpenFlags::empty()).unwrap();
        print!("page boundary read cross test (fd {fd}): ");

        let mut buf = ZEROED;
        assert_eq!(buf.iter().map(|&p| p as usize).sum::<usize>(), 0);

        let read = sys::read(fd, 0, &mut buf).unwrap();
        assert_eq!(read, 0x1001);
        assert_eq!(buf.iter().map(|&p| p as usize).sum::<usize>(), b'A' as usize * 0x1001);

        _ = sys::close(fd);
        assert_eq!(sys::read(fd, 0, &mut buf), Err(shared::sys::SysError::BadFd));

        println!("GOOD");
    }

    {
        let fd = sys::open("/test.txt", OpenFlags::empty()).unwrap();
        println!("file cursor test (fd {fd}): ");

        let mut buf = [0; 8];
        loop {
            match sys::read(fd, u64::MAX, &mut buf) {
                Ok(0) => break,
                Ok(n) => println!("Read {n} bytes: {:?}", core::str::from_utf8(&buf[..n])),
                Err(err) => panic!("Cursor file read error: {err:?}"),
            }
        }

        _ = sys::close(fd);
    }

    _ = sys::shutdown(false).unwrap();
}
