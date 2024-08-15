#![no_std]
#![no_main]

use core::ffi::CStr;

use bc::{Program, StepResult, Vm};
use userstd::println;

mod bc;

// fn repl() -> usize {
//     let mut buffer = String::new();
//
//     let mut vm = Vm::new();
//     let mut lbl = false;
//     loop {
//         println!(">> ");
//         let count = stdin.read_to_string(&mut buffer)?;
//         buffer.truncate(count);
//
//         match &buffer[..] {
//             "quit" => break Ok(()),
//             "reset" => vm.reset(),
//             "dump" => vm.dump(),
//             "lbl" => {
//                 lbl = !lbl;
//                 println!(
//                     "Line-by-line disassemble is {}",
//                     if lbl { "enabled" } else { "disabled " }
//                 );
//             }
//             code => match Program::compile(code.as_bytes()) {
//                 Ok(code) => {
//                     vm.load(code);
//                     vm.run_for(None);
//                 }
//                 Err(err) => println!("Compile error: {err:?}"),
//             },
//         }
//     }
// }

#[no_mangle]
fn main(args: &[*const u8]) -> usize {
    let mut _line_disassemble = false;
    let mut dump = false;
    let mut program = None;
    for &arg in args.iter() {
        let arg = unsafe { CStr::from_ptr(arg.cast()) }.to_bytes();
        if arg.starts_with(b"-") {
            if arg.contains(&b'd') {
                dump = true;
            } else if arg.contains(&b'l') {
                _line_disassemble = true;
            }
        } else {
            program = Some(arg);
        }
    }

    let Some(program) = program else {
        // return repl();
        let name = unsafe { CStr::from_ptr(args[0].cast()) };
        println!("usage: {name:?} [-ld] <program>");
        return 1;
    };

    let buf = match userstd::io::read_file(program) {
        Ok(buf) => buf,
        Err(err) => {
            println!("error reading file: {err:?}");
            return 1;
        }
    };

    let program = match Program::compile(&buf) {
        Ok(program) => program,
        Err(err) => {
            println!("error compiling program: {err:?}");
            return 1;
        }
    };

    if dump {
        program.dump();
        return 0;
    }

    let mut vm = Vm::new();
    vm.load(program);

    match vm.run_for(None) {
        StepResult::Error(err) => {
            println!("Attempt to execute at {err:#04x}: ");
            vm.dump();
            return 1;
        }
        StepResult::SysError(err) => {
            println!("error compiling program: {err:?}");
            return 1;
        }
        _ => {}
    }

    0
}
