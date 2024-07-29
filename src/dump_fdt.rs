// Adapted from: https://github.com/rs-embedded/fdtdump/blob/master/src/main.rs

use alloc::vec;
use fdt_rs::base::iters::StringPropIter;
use fdt_rs::base::DevTree;
use fdt_rs::error::Result as DevTreeResult;
use fdt_rs::index::{DevTreeIndex, DevTreeIndexNode, DevTreeIndexProp};
use fdt_rs::prelude::*;
use fdt_rs::spec::fdt_reserve_entry;

use crate::{print, println};

fn are_printable_strings(mut prop_iter: StringPropIter) -> bool {
    loop {
        match prop_iter.next() {
            Ok(Some(s_ref)) => {
                if s_ref.is_empty() {
                    return false;
                }
            }
            Ok(None) => return true,
            Err(_) => return false,
        }
    }
}

struct FdtDumper {
    indent: usize,
}

impl FdtDumper {
    fn push_indent(&mut self) {
        for _ in 0..self.indent {
            print!("    ");
        }
    }

    fn dump_node_name(&mut self, name: &str) {
        self.push_indent();
        println!("{name} {{");
    }

    fn dump_node(&mut self, node: &DevTreeIndexNode) -> DevTreeResult<()> {
        let mut name = node.name()?;
        if name.is_empty() {
            name = "/";
        } else {
            name = node.name()?;
        }
        self.dump_node_name(name);
        Ok(())
    }

    fn dump_property(&mut self, prop: DevTreeIndexProp) -> DevTreeResult<()> {
        self.push_indent();
        print!("{}", prop.name()?);

        if prop.length() == 0 {
            println!(";");
            return Ok(());
        }
        print!(" = ");

        // Unsafe Ok - we're reinterpreting the data as expected.
        unsafe {
            // First try to parse as an array of strings
            if are_printable_strings(prop.iter_str()) {
                let mut iter = prop.iter_str();
                let mut wrote = false;
                while let Some(s) = iter.next()? {
                    if wrote {
                        print!(", ");
                    }
                    print!("\"{s}\"");
                    wrote = true;
                }
            } else if prop.propbuf().len() % size_of::<u32>() == 0 {
                print!("<");
                let mut wrote = false;
                for val in prop.propbuf().chunks_exact(size_of::<u32>()) {
                    // We use read_unaligned
                    #[allow(clippy::cast_ptr_alignment)]
                    let v = (val.as_ptr() as *const u32).read_unaligned();
                    if wrote {
                        print!(" ");
                    }
                    print!("{:#010x}", u32::from_be(v));
                    wrote = true;
                }
                print!(">");
            } else {
                print!("[");
                let mut wrote = false;
                for val in prop.propbuf() {
                    if wrote {
                        print!(" ");
                    }
                    print!("{val:02x}");
                    wrote = true;
                }
                print!("]");
            }
        }

        println!(";");
        Ok(())
    }

    fn dump_level(&mut self, node: &DevTreeIndexNode) -> DevTreeResult<()> {
        self.dump_node(node)?;
        self.indent += 1;
        for prop in node.props() {
            self.dump_property(prop)?;
        }
        for child in node.children() {
            self.dump_level(&child)?;
        }
        self.indent -= 1;
        self.push_indent();
        println!("}};");
        Ok(())
    }

    fn dump_metadata(&mut self, index: &DevTreeIndex) {
        let fdt = index.fdt();
        println!("/dts-v1/;");
        println!("// magic:\t\t{:#x}", index.fdt().magic());
        let s = fdt.totalsize();
        println!("// totalsize:\t\t{s:#x} ({s})");
        println!("// off_dt_struct:\t{:#x}", fdt.off_dt_struct());
        println!("// off_dt_strings:\t{:#x}", fdt.off_dt_strings());
        println!("// off_mem_rsvmap:\t{:#x}", fdt.off_mem_rsvmap());
        println!("// version:\t\t{:}", fdt.version());
        println!("// last_comp_version:\t{:}", fdt.last_comp_version());
        println!("// boot_cpuid_phys:\t{:#x}", fdt.boot_cpuid_phys());
        println!("// size_dt_strings:\t{:#x}", fdt.size_dt_strings());
        println!("// size_dt_struct:\t{:#x}\n", fdt.size_dt_struct());

        for rsv in fdt.reserved_entries() {
            let rsv = unsafe {
                (*(&rsv as *const _ as *const *const fdt_reserve_entry)).read_unaligned()
            };

            println!(
                "/memreserve/ {:#x} {:#x};",
                u64::from(rsv.address),
                u64::from(rsv.size)
            );
        }

        println!();
    }
}

#[allow(unused)]
pub fn dump_tree(dt: DevTree<'_>) -> DevTreeResult<()> {
    let layout = DevTreeIndex::get_layout(&dt)?;
    let mut buf = vec![0; layout.size() + layout.align()];
    let index = DevTreeIndex::new(dt, &mut buf)?;
    let mut dumper = FdtDumper { indent: 0 };
    dumper.dump_metadata(&index);
    dumper.dump_level(&index.root())?;
    Ok(())
}
