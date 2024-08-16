#![no_std]
#![no_main]

use userstd::{
    alloc::vec::Vec,
    print, println,
    sys::{self, KString, RawFd, SysError},
};

static PATH: &[&[u8]] = &[b"/bin", b"/sbin"];

fn read_buf(buf: &mut [u8]) -> usize {
    loop {
        match sys::read(RawFd(1), None, buf) {
            Ok(n) if n != 0 => return n,
            _ => continue,
        }
    }
}

fn try_spawn_in_path(path: &[&[u8]], cmd: &[u8], args: &[KString]) -> Option<u32> {
    for dir in path {
        let mut buf = dir.to_vec();
        buf.push(b'/');
        buf.extend(cmd);
        if let Ok(pid) = sys::spawn(buf, args) {
            return Some(pid);
        }
    }

    None
}

fn parse_cmd(raw: &str, cmd: &[&[u8]]) -> usize {
    if cmd[0] == b"cd" {
        if cmd.len() > 2 {
            println!("cd: too many arguments");
        } else {
            match sys::chdir(cmd[1]) {
                Ok(()) => {}
                Err(err) => println!("cd: error: {err:?}"),
            }
        }

        0
    } else {
        let mut args = Vec::new();
        let mut bg = false;
        for arg in cmd[1..].iter() {
            if arg == b"&" {
                bg = true;
            } else {
                args.push(KString::new(arg));
            }
        }

        let pid = match sys::spawn(cmd[0], &args) {
            Ok(pid) => pid,
            Err(err @ SysError::PathNotFound) if !cmd[0].contains(&b'/') => {
                if let Some(pid) = try_spawn_in_path(PATH, cmd[0], &args) {
                    pid
                } else {
                    println!("spawn error for '{raw}': {err:?}");
                    return 0;
                }
            }
            Err(err) => {
                println!("spawn error for '{raw}': {err:?}");
                return 0;
            }
        };

        if bg {
            println!("spawned background task with PID {pid}");
            0
        } else {
            sys::waitpid(pid).unwrap_or(0)
        }
    }
}

#[no_mangle]
fn main(_args: &[*const u8]) -> usize {
    let mut buf = [0; 0x1000];
    let mut last = 0;
    loop {
        match last {
            usize::MAX => print!("[\x1b[1;31m-1\x1b[0m] "),
            n if n != 0 => print!("[\x1b[1;31m{n}\x1b[0m] "),
            _ => {}
        }
        print!("\x1b[1;32m$ \x1b[0m");
        let n = read_buf(&mut buf);
        for cmd in buf[..n].split(|&c| c == b'\n') {
            let args: Vec<&[u8]> = cmd
                .split(|&c| c == b' ')
                .filter(|a| !a.is_empty())
                .collect();
            if args.is_empty() {
                continue;
            }
            last = parse_cmd(core::str::from_utf8(cmd).unwrap(), &args);
        }
    }
}
