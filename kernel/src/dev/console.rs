use core::mem::MaybeUninit;

use servos::lock::SpinLocked;

use crate::{fs::FsResult, print};

use super::Device;

/*


[hello world..........]
    ^---r
      ^---w
      ^---we
    ^---re
*/

struct Buffer {
    buffer: [u8; 256],
    read: usize,
    write: usize,
    wend: usize,
    rend: usize,
}

impl Buffer {
    const fn new() -> Self {
        Self {
            buffer: [0; 256],
            read: 0,
            write: 0,
            wend: 0,
            rend: 0,
        }
    }

    fn put(&mut self, ch: u8) -> bool {
        match ch {
            b'\r' => {
                if !self.push_ch(b'\n') {
                    return false;
                }

                self.write = self.wend;
                self.rend = self.wend;
                print!("\r\n");
            }
            0x7f => {
                if self.write > self.rend {
                    self.write -= 1;
                    self.wend -= 1;
                    print!("\x08 \x08");
                }
            }
            ch => {
                if !self.push_ch(ch) {
                    return false;
                }

                print!("{}", ch as char);
            }
        }

        true
    }

    fn read<'a>(&mut self, buf: &'a mut [MaybeUninit<u8>]) -> &'a mut [u8] {
        let count = (self.rend - self.read).min(buf.len());
        let slice = &mut buf[..count];
        for (i, ch) in slice.iter_mut().enumerate() {
            *ch = MaybeUninit::new(self.buffer[(self.read + i) % self.buffer.len()]);
        }

        self.read += count;
        unsafe { MaybeUninit::slice_assume_init_mut(slice) }
    }

    fn push_ch(&mut self, ch: u8) -> bool {
        if self.wend - self.read == self.buffer.len() {
            return false;
        }

        self.buffer[self.write % self.buffer.len()] = ch;
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
