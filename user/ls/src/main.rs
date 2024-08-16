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

        let [kb, mb, gb] = [
            self.0 as f64 / KB as f64,
            self.0 as f64 / MB as f64,
            self.0 as f64 / GB as f64,
        ];
        if self.0 < KB {
            write!(f, "{:>4}", self.0)?;
        } else if self.0 < MB {
            if self.0 > 9 * KB {
                write!(f, "{kb:>4.0}k")?;
            } else {
                write!(f, "{kb:>1.1}k")?;
            }
        } else if self.0 < GB {
            if self.0 > 9 * MB {
                write!(f, "{mb:>4.0}M")?;
            } else {
                write!(f, "{mb:>1.1}M")?;
            }
        } else if self.0 > 9 * GB {
            write!(f, "{gb:>4.0}G")?;
        } else {
            write!(f, "{gb:>1.1}G")?;
        }

        Ok(())
    }
}

fn printdir(dir: impl AsRef<[u8]>, name: bool, all: bool) -> bool {
    let dir = dir.as_ref();
    let Ok(fd) = sys::open(dir, OpenFlags::empty()) else {
        println!("'{}': doesn't exist", core::str::from_utf8(dir).unwrap());
        return false;
    };

    if name {
        println!("'{}': ", core::str::from_utf8(dir).unwrap());
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

    while let Ok(Some(ent)) = sys::readdir(fd, None) {
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
fn main(args: &[*const u8]) -> usize {
    let args = args[1..]
        .iter()
        .map(|arg| unsafe { CStr::from_ptr(arg.cast()).to_bytes() });

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
    let mut ecode = 0;
    for path in args.clone() {
        if path.starts_with(b"-") {
            continue;
        }

        if printed {
            println!();
        }
        if !printdir(path, printed, all) {
            ecode = 1;
        }
        printed = true;
    }

    ecode
}
