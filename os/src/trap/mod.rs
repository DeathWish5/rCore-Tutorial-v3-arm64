mod context;

pub use context::TrapContext;

use core::arch::global_asm;

use cortex_a::registers::{ESR_EL1, FAR_EL1, VBAR_EL1};
use tock_registers::interfaces::{Readable, Writeable};

use crate::{batch::run_next_app, syscall::syscall};

global_asm!(include_str!("trap.S"));

#[allow(clippy::fn_to_numeric_cast)]
pub fn init() {
    extern "C" {
        fn exception_vector_base();
    }
    VBAR_EL1.set(exception_vector_base as u64);
}

#[repr(u8)]
#[derive(Debug)]
#[allow(dead_code)]
enum TrapKind {
    Synchronous = 0,
    Irq = 1,
    Fiq = 2,
    SError = 3,
}

#[repr(u8)]
#[derive(Debug)]
#[allow(dead_code)]
enum TrapSource {
    CurrentSpEl0 = 0,
    CurrentSpElx = 1,
    LowerAArch64 = 2,
    LowerAArch32 = 3,
}

#[no_mangle]
fn invalid_exception(cx: &mut TrapContext, kind: TrapKind, source: TrapSource) {
    panic!(
        "Invalid exception {:?} from {:?}:\n{:#x?}",
        kind, source, cx
    );
}

#[no_mangle]
fn handle_sync_exception(cx: &mut TrapContext) {
    let esr = ESR_EL1.extract();
    match esr.read_as_enum(ESR_EL1::EC) {
        Some(ESR_EL1::EC::Value::Unknown) => {
            println!("[kernel] Unknown exception @ {:#x}", cx.elr);
            run_next_app();
        }
        Some(ESR_EL1::EC::Value::SVC64) => {
            cx.x[0] = syscall(cx.x[8], [cx.x[0], cx.x[1], cx.x[2]]) as usize
        }
        Some(ESR_EL1::EC::Value::DataAbortLowerEL)
        | Some(ESR_EL1::EC::Value::DataAbortCurrentEL) => {
            let iss = esr.read(ESR_EL1::ISS);
            println!(
                "[kernel] Data Abort @ {:#x}, FAR = {:#x}, ISS = {:#x}",
                cx.elr,
                FAR_EL1.get(),
                iss
            );
            run_next_app();
        }
        Some(ESR_EL1::EC::Value::InstrAbortLowerEL)
        | Some(ESR_EL1::EC::Value::InstrAbortCurrentEL) => {
            let iss = esr.read(ESR_EL1::ISS);
            println!(
                "[kernel] Instruction Abort @ {:#x}, FAR = {:#x}, ISS = {:#x}",
                cx.elr,
                FAR_EL1.get(),
                iss
            );
            run_next_app();
        }
        _ => {
            panic!(
                "Unsupported synchronous exception @ {:#x}: ESR = {:#x} (EC {:#08b}, ISS {:#x})",
                cx.elr,
                esr.get(),
                esr.read(ESR_EL1::EC),
                esr.read(ESR_EL1::ISS),
            );
        }
    }
}

#[no_mangle]
fn handle_irq_exception(_cx: &mut TrapContext) -> ! {
    panic!("Unsupported IRQ exception!");
}
