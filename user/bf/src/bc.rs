use userstd::{
    alloc::vec::Vec,
    print, println,
    sys::{self, RawFd, SysError},
};

#[repr(u8)]
#[derive(strum::FromRepr)]
enum Opcode {
    Halt,
    Inc,
    Dec,
    Left,
    Right,
    Out,
    In,
    Jz,
    Jnz,
}

#[derive(Debug)]
pub enum CompileError {
    UnmatchedR,
    UnmatchedL,
}

pub enum StepResult {
    Halt,
    Continue,
    Error(usize),
    SysError(SysError),
}

#[derive(Default)]
pub struct Program {
    code: Vec<u8>,
}

impl Program {
    pub fn compile(program: &[u8]) -> Result<Self, CompileError> {
        let mut code = Vec::new();
        let mut jumps = Vec::new();
        for byte in program {
            match byte {
                b'>' => code.push(Opcode::Right as u8),
                b'<' => code.push(Opcode::Left as u8),
                b'+' => code.push(Opcode::Inc as u8),
                b'-' => code.push(Opcode::Dec as u8),
                b'.' => code.push(Opcode::Out as u8),
                b',' => code.push(Opcode::In as u8),
                b'[' => {
                    code.push(Opcode::Jz as u8);
                    jumps.push(code.len() as u32);
                    code.resize(code.len() + 4, 0);
                }
                b']' => {
                    let Some(last) = jumps.pop() else {
                        return Err(CompileError::UnmatchedR);
                    };

                    code.push(Opcode::Jnz as u8);
                    code.extend((last + 4).to_le_bytes());

                    let here = code.len() as u32;
                    code[last as usize..][..4].copy_from_slice(&here.to_le_bytes());
                }
                _ => {}
            }
        }

        if !jumps.is_empty() {
            return Err(CompileError::UnmatchedL);
        }

        code.push(Opcode::Halt as u8);
        Ok(Self { code })
    }

    fn read_jmp(&self, ip: usize) -> usize {
        u32::from_le_bytes(self.code[ip..][..4].try_into().unwrap()) as usize
    }

    pub fn dump_at(&self, ip: &mut usize) {
        print!("{ip:#04x}  ");
        let Some(&byte) = self.code.get(*ip) else {
            return print!("EOF      ");
        };
        *ip += 1;
        let Some(opcode) = Opcode::from_repr(byte) else {
            return print!("UNK  {byte:#02x}");
        };

        match opcode {
            Opcode::Halt => print!("HLT      "),
            Opcode::Inc => print!("INC      "),
            Opcode::Dec => print!("DEC      "),
            Opcode::Left => print!("L        "),
            Opcode::Right => print!("R        "),
            Opcode::Out => print!("OUT      "),
            Opcode::In => print!("IN       "),
            Opcode::Jz => {
                print!("JZ   {:#04x}", self.read_jmp(*ip));
                *ip += 4;
            }
            Opcode::Jnz => {
                print!("JNZ  {:#04x}", self.read_jmp(*ip));
                *ip += 4;
            }
        }
    }

    pub fn dump(&self) {
        let mut ip = 0;
        while ip < self.code.len() {
            self.dump_at(&mut ip);
            println!();
        }
    }
}

pub struct Vm {
    bp: usize,
    ip: usize,
    data: [u8; 30000],
    program: Program,
}

impl Vm {
    pub fn new() -> Self {
        Self {
            bp: 0,
            ip: 0,
            data: [0; 30000],
            program: Default::default(),
        }
    }

    pub fn load(&mut self, program: Program) {
        self.ip = 0;
        self.program = program;
    }

    pub fn reset(&mut self) {
        self.load(Default::default());
        self.bp = 0;
        self.data.fill(0);
    }

    pub fn run_for(&mut self, steps: Option<usize>) -> StepResult {
        let mut count = 0;
        while steps.map_or(true, |steps| count < steps) {
            count += 1;
            match self.step() {
                StepResult::Continue => continue,
                err => return err,
            }
        }

        StepResult::Continue
    }

    pub fn step(&mut self) -> StepResult {
        let Some(instr) = self
            .program
            .code
            .get(self.ip)
            .copied()
            .and_then(Opcode::from_repr)
        else {
            return StepResult::Error(self.ip);
        };

        self.ip += 1;
        match instr {
            Opcode::Halt => return StepResult::Halt,
            Opcode::Inc => self.data[self.bp] = self.data[self.bp].wrapping_add(1),
            Opcode::Dec => self.data[self.bp] = self.data[self.bp].wrapping_sub(1),
            Opcode::Right => self.bp += 1,
            Opcode::Left => self.bp -= 1,
            Opcode::Out => {
                if let Err(err) = sys::write(RawFd(0), None, &self.data[self.bp..][..1]) {
                    return StepResult::SysError(err);
                }
            }
            Opcode::In => {
                self.data[self.bp] = loop {
                    let mut buf = 0;
                    match sys::read(RawFd(1), None, core::slice::from_mut(&mut buf)) {
                        Ok(0) => continue,
                        Ok(_) => break buf,
                        Err(SysError::Eof) => break 0,
                        Err(err) => return StepResult::SysError(err),
                    }
                };
            }
            Opcode::Jz => {
                if self.data[self.bp] == 0 {
                    self.ip = self.program.read_jmp(self.ip);
                } else {
                    self.ip += 4;
                }
            }
            Opcode::Jnz => {
                if self.data[self.bp] != 0 {
                    self.ip = self.program.read_jmp(self.ip);
                } else {
                    self.ip += 4;
                }
            }
        }

        StepResult::Continue
    }

    pub fn dump(&self) {
        self.program.dump_at(&mut self.ip.clone());
        print!("\t\t | BP: {:#04x} [ ", self.bp);

        const RANGE: usize = 5;
        for (i, byte) in self.data[self.bp.saturating_sub(RANGE)..][..RANGE + 1]
            .iter()
            .enumerate()
        {
            if i == self.bp {
                print!("\x1b[31;1;4m{byte:#02x}\x1b[0m ");
            } else {
                print!("{byte:#02x} ");
            }
        }
        println!("]");
    }
}

impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}
