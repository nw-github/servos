#![no_std]
#![no_main]

use userstd::{
    io::OpenFlags,
    print, println,
    sys::{self, SysError},
};

static mut GLOBAL_STATIC: usize = 5;

#[link_section = ".bss"]
static ZEROED: [u8; 0x2000] = [0; 0x2000];

fn test_file_read() {
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

fn test_global_static() {
    unsafe {
        assert_eq!(GLOBAL_STATIC, 5);
        while GLOBAL_STATIC != 0 {
            GLOBAL_STATIC -= 1;
        }
        assert_eq!(GLOBAL_STATIC, 0);
    }
}

fn test_fd_cursor() {
    let fd = sys::open("test.txt", OpenFlags::empty()).unwrap();
    println!("file cursor test (fd {fd:?}): ");

    let mut buf = [0; 8];
    loop {
        match sys::read(fd, None, &mut buf) {
            Ok(n) => println!("Read {n} bytes: {:?}", core::str::from_utf8(&buf[..n])),
            Err(SysError::Eof) => break,
            Err(err) => panic!("Cursor file read error: {err:?}"),
        }
    }

    _ = sys::close(fd);
}

#[no_mangle]
fn main(_args: &[*const u8]) -> usize {
    test_global_static();
    test_file_read();
    test_fd_cursor();

    println!("testing sbrk: ");
    let brk = sys::sbrk(0).unwrap() as usize;
    {
        assert_eq!(sys::sbrk(10).unwrap() as usize - brk, 10);
        unsafe {
            ((brk + 5) as *mut u8).write(1);
        }
        assert_eq!(sys::sbrk(-10).unwrap() as usize - brk, 0);
    }

    {
        assert_eq!(sys::sbrk(0x1000).unwrap() as usize - brk, 0x1000);
        unsafe {
            ((brk + (0x1000 - 1)) as *mut u8).write(1);
        }
        assert_eq!(sys::sbrk(-0x1000).unwrap() as usize - brk, 0);
    }

    println!("testing sbrk actually unmapped (should crash): ");
    unsafe {
        ((brk + (0x1000 - 1)) as *mut u8).write(1);
    }

    0
}
