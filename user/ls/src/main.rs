#![no_std]
#![no_main]

use core::ffi::CStr;

use userstd::{io::OpenFlags, println, sys};

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

fn printdir(dir: impl AsRef<[u8]>, name: bool, all: bool) -> bool {
    let dir = dir.as_ref();
    let Ok(fd) = sys::open(dir, OpenFlags::empty()) else {
        println!("{}: doesn't exist", core::str::from_utf8(dir).unwrap());
        return false;
    };

    if name {
        println!("{}: ", core::str::from_utf8(dir).unwrap());
    }

    if let Ok(stat) = sys::stat(fd) {
        if !stat.directory {
            println!(
                ".r{}x {}  {}",
                if stat.readonly { "-" } else { "w" },
                Size(stat.size),
                core::str::from_utf8(dir).unwrap()
            );
            return true;
        }
    }

    while let Ok(Some(ent)) = sys::readdir(fd, usize::MAX) {
        let name = core::str::from_utf8(&ent.name[..ent.name_len]).unwrap();
        if name.starts_with(".") && !all {
            continue;
        }

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

    true
}

#[no_mangle]
extern "C" fn _start(argc: usize, argv: *const *const u8) {
    // open stdout
    _ = sys::open("/dev/uart0", OpenFlags::ReadWrite).unwrap();
    unsafe {
        let args = core::slice::from_raw_parts(argv, argc);
        let args = args[1..]
            .iter()
            .map(|arg| CStr::from_ptr(arg.cast()).to_bytes());

        let mut all = false;
        for arg in args.clone() {
            if !arg.starts_with(b"-") {
                continue;
            }

            if arg.contains(&b'a') {
                all = true;
            }
        }

        let mut printed = false;
        for path in args.clone() {
            if path.starts_with(b"-") {
                continue;
            }

            if printed {
                println!();
            }
            printdir(path, printed, all);
            printed = true;
        }
    }

    _ = sys::kill(sys::getpid());
    #[allow(deref_nullptr)]
    unsafe {
        *core::ptr::null_mut::<u8>() = 0;
    }
}
