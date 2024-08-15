#![no_std]
#![no_main]

use userstd::{
    alloc::vec::Vec,
    print, println,
    sys::{self, KString, RawFd},
};

fn read_buf(buf: &mut [u8]) -> usize {
    loop {
        match sys::read(RawFd(1), 0, buf) {
            Ok(n) if n != 0 => return n,
            _ => continue,
        }
    }
}

fn parse_cmd(raw: &str, cmd: &[&[u8]]) {
    if cmd[0] == b"cd" {
        if cmd.len() > 2 {
            println!("cd: too many arguments");
        } else {
            match sys::chdir(cmd[1]) {
                Ok(()) => {}
                Err(err) => println!("cd: error: {err:?}"),
            }
        }
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

        match sys::spawn(cmd[0], &args) {
            Ok(pid) if !bg => _ = sys::waitpid(pid),
            Err(err) => println!("spawn error for '{raw}': {err:?}"),
            _ => {}
        }
    }
}

#[no_mangle]
fn main(_args: &[*const u8]) -> usize {
    let mut buf = [0; 0x1000];
    loop {
        print!("$ ");
        let n = read_buf(&mut buf);
        for cmd in buf[..n].split(|&c| c == b'\n') {
            let args: Vec<&[u8]> = cmd
                .split(|&c| c == b' ')
                .filter(|a| !a.is_empty())
                .collect();
            if args.is_empty() {
                continue;
            }
            parse_cmd(core::str::from_utf8(cmd).unwrap(), &args);
        }
    }
}
