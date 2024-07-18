use core::sync::atomic::AtomicUsize;
use alloc::string::String;
use riscv::register::hstatus::{self, VirtualizationMode};
use rustsbi::spec::hsm::{EID_HSM, HART_STOP};
use spin::Once;

use crate::arch::{
    hypervisor_handle_ecall, ldst_guest_page_fault_handler, A0_NUM, A1_NUM, A2_NUM, A3_NUM, A4_NUM, A5_NUM, A6_NUM,
    A7_NUM,
};
use crate::kernel::{current_cpu, hvc_guest_handler, interrupt_handler};
use super::interface::ContextFrame;
use super::{init_ecall_handler, riscv_get_pending_irqs};
use riscv::register::{sstatus, vsstatus};

#[cfg(not(feature = "sbi_legacy"))]
use super::VmHart;

pub const INTR_CAUSE: [&str; 16] = [
    "Reserved",
    "Supervisor software interrupt",
    "Virtual Supervisor software interrupt",
    "Machine software interrupt",
    "Reserved",
    "Supervisor timer interrupt",
    "Virtual Supervisor timer interrupt",
    "Machine timer interrupt",
    "Reserved",
    "Supervisor external interrupt",
    "Virtual Supervisor external interrupt",
    "Machine external interrupt",
    "Supervisor guest external interrupt",
    "Reserved",
    "Reserved",
    "Reserved",
];

pub const EXCEPTION_CAUSE: [&str; 24] = [
    "Instruction address misaligned",
    "Instruction access fault",
    "Illegal instruction",
    "Breakpoint",
    "Load address misaligned",
    "Load access fault",
    "Store/AMO address misaligned",
    "Store/AMO access fault",
    "Environment call from U-mode or VU-mode",
    "Environment call from HS-mode",
    "Environment call from VS-mode",
    "Environment call from M-mode",
    "Instruction page fault",
    "Load page fault",
    "Reserved",
    "Store/AMO page fault",
    "Reserved",
    "Reserved",
    "Reserved",
    "Reserved",
    "Instruction guest-page fault",
    "Load guest-page fault",
    "Virtual instruction",
    "Store/AMO guest-page fault",
];

const INSTR_PAGE_FAULT: usize = 12;
const LOAD_PAGE_FAULT: usize = 13;
const STORE_PAGE_FAULT: usize = 15;

const ECALL_FROM_VS: usize = 10;
// spin::Once<Mutex<SmmuV2>> = spin::Once::new();
#[cfg(feature = "sbi_legacy")]
static SBI_VM_HART: Once = Once::new();
#[cfg(not(feature = "sbi_legacy"))]
static SBI_VM_HART: Once<VmHart> = Once::new();

fn ecall_handler(ctx: &mut ContextFrame) {
    let eid = ctx.gpr[A7_NUM];
    let fid = ctx.gpr[A6_NUM];

    let x0 = ctx.gpr[A0_NUM] as usize;
    let x1 = ctx.gpr[A1_NUM] as usize;
    let x2 = ctx.gpr[A2_NUM] as usize;
    let x3 = ctx.gpr[A3_NUM] as usize;
    let x4 = ctx.gpr[A4_NUM] as usize;
    let x5 = ctx.gpr[A5_NUM] as usize;

    let ret;

    // SBI spec defined this space as firmware specific extension space
    // we use this space for hvc call
    if (0x0A000000..=0x0AFFFFFF).contains(&eid) {
        let hvc_type = ((eid >> 8) & 0xff) as usize;
        let event = (eid & 0xff) as usize;
        match hvc_guest_handler(hvc_type, event, x0, x1, x2, x3, x4, x5, fid as usize) {
            Ok(val) => {
                current_cpu().set_gpr(A0_NUM, val);
            }
            Err(_) => {
                warn!("Failed to handle hvc request type 0x{:x} event 0x{:x}", hvc_type, event);
                current_cpu().set_gpr(A0_NUM, usize::MAX);
            }
        }
        return;
    }

    #[cfg(not(feature = "sbi_legacy"))]
    {
        ret =
            SBI_VM_HART
                .call_once(|| VmHart::new())
                .handle_ecall(eid as usize, fid as usize, [x0, x1, x2, x3, x4, x5]);
    }
    #[cfg(feature = "sbi_legacy")]
    {
        SBI_VM_HART.call_once(init_ecall_handler);
        ret = hypervisor_handle_ecall(eid as usize, fid as usize, [x0, x1, x2, x3, x4, x5]);
    }

    if eid == EID_HSM as u64 && fid == HART_STOP as u64 {
        // hart_stop，no need to move elr
        return;
    }

    // Set return value
    current_cpu().set_gpr(A0_NUM, ret.error);
    current_cpu().set_gpr(A1_NUM, ret.value);
}

fn get_previous_mode() -> String {
    let spv = hstatus::read_spv();
    let spp = sstatus::read().spp();
    if spv as usize == VirtualizationMode::Guest as usize {
        if spp == sstatus::SPP::User {
            String::from("VU")
        } else {
            String::from("VS")
        }
    } else if spp == sstatus::SPP::User {
        String::from("U")
    } else {
        String::from("HS")
    }
}

static TIMER_IRQ_COUNT: AtomicUsize = AtomicUsize::new(0);

#[no_mangle]
pub fn exception_rust_handler(ctx: &mut ContextFrame) {
    /// SAFETY: 'ctx' is a valid pointer to a ContextFrame, which was assigned by the assembly code.
    unsafe {
        current_cpu().set_ctx(ctx)
    }

    // The destination of the jump back is determined by the state of the previous CPU.
    // If you enter VS state from VS state, the return address is the address of VS state
    // If you enter VS state from HS state, the return address is the address of HS state.
    // There is no need to manually set the spv bit of hstatus
    let scause = ctx.scause;
    let sepc = ctx.sepc;

    let is_intr = ((scause >> 63) & 1) == 1;
    let cause = scause & 0xfff;

    if is_intr {
        if let Some(id) = riscv_get_pending_irqs(cause as usize) {
            interrupt_handler(id);
            // Clearing the PLIC interrupt may cause the VM to miss the interrupt signal, resulting in a freeze, so it is not recommended to clear the interrupt
            // but wait until the next time you return to VS, because the peripheral interrupt that was not handled before falls back into the Hypervisor
        }
    } else {
        match cause as usize {
            ECALL_FROM_VS => {
                ecall_handler(ctx);
                // Skip the ecall instruction that has already been executed (the instruction length is 4B)
                current_cpu().set_elr(current_cpu().get_elr() + 4);
            }
            21 | 23 => {
                // Load / Store guest-page fault
                ldst_guest_page_fault_handler(ctx);
            }
            2 => {
                // Note：Floating-point instructions can be executed only if both S and VS have the FS switch on the Status register
                info!(
                    "sstatus: {}, vsstatus: {:#x}, previous_mode = {}\n{}",
                    sstatus::read().fs() as usize,
                    vsstatus::read(),
                    get_previous_mode(),
                    ctx
                );
                panic!("illegal instruction: 0x{:08x}", sepc);
            }
            _ => {
                panic!(
                    "unhandled exception: id = {}, {}\ncpu_id = {}\n{}Previous mode = {}",
                    cause,
                    EXCEPTION_CAUSE[cause as usize],
                    current_cpu().id,
                    ctx,
                    get_previous_mode()
                );
            }
        }
    }

    // Note：Do not set the spp and spie bits of sstatus arbitrarily;
    // otherwise, when GuestOS VU-mode is trapped in Hypervisor, sret will crash.
    // A deep lesson!! Keep the original value

    current_cpu().clear_ctx();
}
