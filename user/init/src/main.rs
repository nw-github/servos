#![no_std]
#![no_main]

use core::ffi::CStr;

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

struct Size(usize);

impl core::fmt::Display for Size {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        const KB: usize = 1024;
        const MB: usize = KB * 1024;
        const GB: usize = MB * 1024;

        let [kb, mb, gb] = [self.0 / KB, self.0 / MB, self.0 / GB];
        if kb == 0 {
            write!(f, "{:>4}", self.0)?;
        } else if mb == 0 {
            if kb > 9 {
                write!(f, "{kb:>4}k")?;
            } else {
                write!(f, "{kb:>1}.{}k", (self.0 % KB) / 100)?;
            }
        } else if gb == 0 {
            if mb > 9 {
                write!(f, "{mb:>4}M")?;
            } else {
                write!(f, "{mb:>1}.{}M", (self.0 % MB) / 100000)?;
            }
        } else if gb > 9 {
            write!(f, "{gb:>4}G")?;
        } else {
            write!(f, "{gb:>1}.{}G", (self.0 % GB) / 100000000)?;
        }

        Ok(())
    }
}

fn printdir(dir: impl AsRef<[u8]>, mut tabs: usize) {
    let dir = dir.as_ref();
    let Ok(fd) = sys::open(dir, OpenFlags::empty()) else {
        return;
    };

    (0..tabs).for_each(|_| print!("  "));
    tabs += 1;

    println!("{}", core::str::from_utf8(dir).unwrap());
    while let Ok(Some(ent)) = sys::readdir(fd, usize::MAX) {
        (0..tabs).for_each(|_| print!("  "));
        let name = core::str::from_utf8(&ent.name[..ent.name_len]).unwrap();
        if ent.stat.directory {
            println!("dr-- {:>4}  {name}", "-");
        } else {
            println!(
                ".r{}x {}  {name}",
                if ent.stat.readonly { "-" } else { "w" },
                Size(ent.stat.size)
            );
        }
    }
}

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
            Err(shared::sys::SysError::BadFd)
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

    printdir("/", 0);
    printdir("/dev", 1);
    printdir("bin", 1);

    sys::chdir("/bin").unwrap();
    printdir(".", 0);

    _ = sys::shutdown(false).unwrap();
}
