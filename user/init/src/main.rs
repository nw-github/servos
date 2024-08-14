#![no_std]
#![no_main]

use core::ffi::CStr;

use userstd::{
    io::OpenFlags,
    print, println,
    sys::{self, String},
};

static mut GLOBAL_STATIC: usize = 5;

#[link_section = ".bss"]
static ZEROED: [u8; 0x2000] = [0; 0x2000];

#[no_mangle]
extern "C" fn _start(argc: usize, argv: *const *const u8) {
    // open stdout and stdin
    _ = sys::open("/dev/uart0", OpenFlags::ReadWrite).unwrap();
    _ = sys::open("/dev/uart0", OpenFlags::empty()).unwrap();
    println!("\n\nHello world from the init process!");

    if argc != 0 {
        unsafe {
            let args = core::slice::from_raw_parts(argv, argc);
            for (i, arg) in args.iter().enumerate() {
                println!("ARG {i}: {:?}", CStr::from_ptr(arg.cast()));
            }
        }
    }

    unsafe {
        assert_eq!(GLOBAL_STATIC, 5);
        while GLOBAL_STATIC != 0 {
            GLOBAL_STATIC -= 1;
        }
        assert_eq!(GLOBAL_STATIC, 0);
    }

    {
        let fd = sys::open("1001_A.txt", OpenFlags::empty()).unwrap();
        print!("page boundary read cross test (fd {fd:?}): ");

        let mut buf = ZEROED;
        assert_eq!(buf.iter().map(|&p| p as usize).sum::<usize>(), 0);

        let read = sys::read(fd, 0, &mut buf).unwrap();
        assert_eq!(read, 0x1001);
        assert_eq!(
            buf.iter().map(|&p| p as usize).sum::<usize>(),
            b'A' as usize * 0x1001
        );

        _ = sys::close(fd);
        assert_eq!(
            sys::read(fd, 0, &mut buf),
            Err(userstd::sys::SysError::BadFd)
        );

        println!("GOOD");
    }

    {
        let fd = sys::open("test.txt", OpenFlags::empty()).unwrap();
        println!("file cursor test (fd {fd:?}): ");

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

    let pid0 = sys::spawn(
        "/bin/ls",
        &[
            String::from("/"),
            String::from("/dev"),
            String::from("bin"),
            String::from("."),
        ],
    )
    .unwrap();
    let pid1 = sys::spawn("/bin/ls", &[String::from("/bin/ls")]).unwrap();

    println!("Spawned PID {pid0} and {pid1}!");
    loop {}

    // _ = sys::shutdown(false).unwrap();
}
