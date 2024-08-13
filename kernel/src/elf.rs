use core::{ffi::CStr, mem::size_of};

#[repr(C)]
#[derive(Debug)]
pub struct EHdr {
    pub ident: [u8; 16],
    pub typ: u16,
    pub machine: u16,
    pub version: u32,
    pub entry: u64,
    pub phoff: u64,
    pub shoff: u64,
    pub flags: u32,
    pub ehsize: u16,
    pub phentsize: u16,
    pub phnum: u16,
    pub shentsize: u16,
    pub shnum: u16,
    pub shstrndx: u16,
}

#[repr(C)]
#[derive(Debug)]
pub struct Shdr {
    pub name: u32,
    pub typ: u32,
    pub flags: u32,
    pub addr: u64,
    pub offset: u64,
    pub size: u32,
    pub link: u32,
    pub info: u32,
    pub addralign: u32,
    pub entsize: u32,
}

#[repr(C)]
#[derive(Debug)]
pub struct Phdr {
    pub typ: u32,
    pub flags: u32,
    pub offset: u64,
    pub vaddr: u64,
    pub paddr: u64,
    pub filesz: u64,
    pub memsz: u64,
    pub align: u64,
}

#[repr(C)]
#[derive(Debug)]
pub struct Sym {
    pub name: u32,
    pub info: u8,
    pub other: u8,
    pub shndx: u16,
    pub value: u64,
    pub size: u64,
}

pub enum ShType {
    Null = 0,
    Progbits = 1,
    Symtab = 2,
    Strtab = 3,
    Rela = 4,
    Nobits = 8,
    Rel = 9,
}

pub enum ShAttributes {
    Write = 1,
    Alloc = 2,
}

pub const EI_MAG0: usize = 0;
pub const EI_MAG3: usize = 3;
pub const EI_CLASS: usize = 4;
pub const EI_DATA: usize = 5;
pub const EI_VERSION: usize = 6;
pub const EI_OSABI: usize = 7;

pub const PN_XNUM: u16 = 0xffff;

pub const SHN_LORESERVE: u16 = 0xff00;
pub const SHN_XINDEX: u16 = 0xffff;
pub const SHN_UNDEF: u16 = 0;

pub const PT_LOAD: u32 = 1;

pub const PF_R: u32 = 1;
pub const PF_W: u32 = 2;
pub const PF_X: u32 = 4;

pub struct ElfFile<'a> {
    pub ehdr: &'a EHdr,
    pub sheaders: &'a [Shdr],
    pub pheaders: &'a [Phdr],
    pub strtable: Option<&'a [u8]>,
    pub raw: &'a [u8],
}

impl<'a> ElfFile<'a> {
    pub fn new(file: &'a [u8]) -> Option<Self> {
        let ehdr = as_struct::<EHdr>(file)?;
        if !matches!(&ehdr.ident[EI_MAG0..=EI_MAG3], b"\x7fELF")
            || ehdr.ident[EI_CLASS] != 2 // is class 32 bit
            || ehdr.ident[EI_DATA] != 1 // 2s complement, little endian
            || ehdr.ident[EI_VERSION] != 1
            || ehdr.typ != 2  // ET_EXEC
            || ehdr.machine != 243 // EM_RISCV
            || ehdr.version != 1
        {
            return None;
        }

        let sh_zero = file
            .get(ehdr.shoff as usize..)
            .and_then(as_struct::<Shdr>)?;
        let phnum = if ehdr.phnum == PN_XNUM {
            sh_zero.info as usize
        } else {
            ehdr.phnum as usize
        };
        let shnum = if ehdr.shnum >= SHN_LORESERVE {
            sh_zero.size as usize
        } else {
            ehdr.shnum as usize
        };

        let sheaders = as_slice(file.get(ehdr.shoff as usize..)?, shnum)?;
        Some(Self {
            raw: file,
            ehdr,
            sheaders,
            pheaders: as_slice(file.get(ehdr.phoff as usize..)?, phnum)?,
            strtable: if ehdr.shstrndx == SHN_UNDEF {
                None
            } else {
                let idx = if ehdr.shstrndx == SHN_XINDEX {
                    sh_zero.link as usize
                } else {
                    ehdr.shstrndx as usize
                };

                Some(file.get(sheaders.get(idx)?.offset as usize..)?)
            },
        })
    }
}

impl Shdr {
    pub fn name<'a>(&self, file: &'a ElfFile) -> Option<&'a CStr> {
        file.strtable
            .filter(|_| self.name != SHN_UNDEF as u32)
            .and_then(|strtable| CStr::from_bytes_until_nul(&strtable[self.name as usize..]).ok())
    }
}

fn as_struct<T>(data: &[u8]) -> Option<&T> {
    if data.len() < size_of::<T>() {
        return None;
    }

    assert!(data.as_ptr().is_aligned_to(core::mem::align_of::<T>()));
    Some(unsafe { &*data.as_ptr().cast() })
}

fn as_slice<T>(data: &[u8], count: usize) -> Option<&[T]> {
    if data.len() < size_of::<T>() * count {
        return None;
    }

    assert!(data.as_ptr().is_aligned_to(core::mem::align_of::<T>()));
    unsafe { Some(core::slice::from_raw_parts(data.as_ptr().cast(), count)) }
}
