use core::mem::MaybeUninit;

use servos::lock::SpinLocked;

use crate::{fs::FsResult, print};

use super::Device;

/*


[hllo...........]
 ^---r
  ^---w
     ^---we
 ^---re
*/

struct Buffer {
    buf: [u8; 256],
    read: usize,
    write: usize,
    wend: usize,
    rend: usize,
    esc: Option<u8>,
}

impl Buffer {
    const fn new() -> Self {
        Self {
            buf: [0; 256],
            read: 0,
            write: 0,
            wend: 0,
            rend: 0,
            esc: None,
        }
    }

    fn put(&mut self, ch: u8) -> bool {
        if let Some(esc) = self.esc {
            self.handle_esc(esc, ch);
            return true;
        }

        match ch {
            0x1b => self.esc = Some(ch),
            b'\r' => {
                if self.wend - self.read == self.buf.len() {
                    return false;
                }

                self.write = self.wend;
                self.push_ch(b'\n');
                self.rend = self.wend;
                print!("\r\n");
            }
            0x7f => {
                if self.write > self.rend {
                    self.write -= 1;
                    self.wend -= 1;

                    if self.wend == self.write {
                        print!("\x08 \x08");
                    } else {
                        print!("\x08");
                        for i in self.write..self.wend {
                            self.buf[i % self.buf.len()] = self.buf[(i + 1) % self.buf.len()];
                            print!("{}", self.buf[i % self.buf.len()] as char);
                        }
                        print!(" ");
                        for _ in self.write..=self.wend {
                            print!("\x08");
                        }
                    }
                }
            }
            ch => {
                if !self.push_ch(ch) {
                    return false;
                }

                print!("{}", ch as char);
                for i in self.write..self.wend {
                    print!("{}", self.buf[i % self.buf.len()] as char);
                }
                for _ in self.write..self.wend {
                    print!("\x08");
                }
            }
        }

        true
    }

    fn read<'a>(&mut self, buf: &'a mut [MaybeUninit<u8>]) -> &'a mut [u8] {
        let count = (self.rend - self.read).min(buf.len());
        let slice = &mut buf[..count];
        for (i, ch) in slice.iter_mut().enumerate() {
            *ch = MaybeUninit::new(self.buf[(self.read + i) % self.buf.len()]);
        }

        self.read += count;
        unsafe { MaybeUninit::slice_assume_init_mut(slice) }
    }

    fn handle_esc(&mut self, esc: u8, ch: u8) {
        // maybe makes sense to let applications handle escape sequences but whatever for now
        match (esc, ch) {
            (0x1b, b'[') | (b'[', b'1') | (b'1', b';') | (b';', b'5') => {
                return self.esc = Some(ch);
            }
            (b'[', b'D') => {
                // LARROW
                if self.write > self.rend {
                    self.write -= 1;
                    print!("\x1b[D");
                }
            }
            (b'5', b'D') => {
                // CTRL + LARROW
            }
            (b'[', b'C') => {
                // RARROW
                if self.write < self.wend {
                    self.write += 1;
                    print!("\x1b[C");
                }
            }
            (b'5', b'C') => {
                // CTRL + RARROW
            }
            _ => {}
        }

        self.esc = None;
    }

    fn push_ch(&mut self, ch: u8) -> bool {
        if self.wend - self.read == self.buf.len() {
            return false;
        }

        for i in (self.write..self.wend).rev() {
            self.buf[(i + 1) % self.buf.len()] = self.buf[i % self.buf.len()];
        }

        self.buf[self.write % self.buf.len()] = ch;
        self.write += 1;
        self.wend += 1;
        true
    }
}

pub struct Console(SpinLocked<Buffer>);

impl Console {
    pub const fn new() -> Self {
        Self(SpinLocked::new(Buffer::new()))
    }

    pub fn put(&self, ch: u8) -> bool {
        self.0.lock().put(ch)
    }
}

impl Device for Console {
    fn read<'a>(&self, _pos: u64, buf: &'a mut [MaybeUninit<u8>]) -> FsResult<&'a mut [u8]> {
        Ok(self.0.lock().read(buf))
    }

    fn write(&self, _pos: u64, buf: &[u8]) -> FsResult<usize> {
        let mut cons = crate::uart::CONS.lock();
        buf.iter().for_each(|&b| cons.put(b));
        Ok(buf.len())
    }
}
