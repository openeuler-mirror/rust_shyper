use core::{arch::riscv64::hlvx_hu, panic};

use crate::{
    arch::print_vs_regs,
    device::{emu_handler, EmuContext},
    kernel::{active_vm, current_cpu},
};

use super::ContextFrame;

// load guest page fault, need to parse instructions, access the address, access width and other information

#[inline(always)]
fn inst_width(funct3: u32) -> usize {
    match funct3 & 3 {
        0 => 1,
        1 => 2,
        2 => 4,
        _ => 8,
    }
}

pub fn read_inst_from_guest_mem(addr: u64) -> u32 {
    if addr & 0x1 == 1 {
        panic!("read_inst_from_guest_mem: unaligned access");
    }

    // SAFETY:
    // 1. Read instruction memory
    // 2. The memory is executable but not nessarily readable
    // 3. The address is sepc, which is a valid instruction address
    // 4. The address is 16bit aligned
    let mut inst: u32 = unsafe { hlvx_hu(addr as *const u16) } as u32;
    // if inst is ending with 0b11, it means the inst is not a compressed inst
    if (inst & 0b11) == 0b11 {
        // SAFETY:
        // Read the second half of the compressed instruction, which is 16bit aligned
        let inst2: u16 = unsafe { hlvx_hu((addr + 2) as *const u16) };
        inst |= (inst2 as u32) << 16;
    }
    inst
}

#[inline(always)]
fn get_ins_size(inst: u32) -> usize {
    if (inst & 0b11) == 0b11 {
        4
    } else {
        // compressed
        2
    }
}

#[inline(always)]
fn is_compressed(inst: u32) -> bool {
    (inst & 0b11) != 0b11
}

const TINST_PSEUDO_STORE: u32 = 0x3020;
const TINST_PSEUDO_LOAD: u32 = 0x3000;

#[inline(always)]
fn is_pseudo_ins(inst: u32) -> bool {
    // memory page fault in implicitly memory access of VS-stage memory translation
    inst == TINST_PSEUDO_LOAD || inst == TINST_PSEUDO_STORE
}

#[inline(always)]
fn transformed_inst_size(inst: u32) -> usize {
    if ((inst) & 0x2) == 0 {
        2
    } else {
        4
    }
}

#[inline(always)]
fn is_compressed_ldst(inst: u32) -> bool {
    // load, store
    (inst & 0xe003) == 0x4000 || (inst & 0xe003) == 0xc000
}

#[inline(always)]
fn is_compressed_st(inst: u32) -> bool {
    (inst & 0xe003) == 0xc000
}

#[inline(always)]
fn is_normal_ldst(inst: u32) -> bool {
    // load, store
    (inst & 0x7f) == 0x03 || (inst & 0x7f) == 0x23
}

#[inline(always)]
fn is_normal_st(inst: u32) -> bool {
    (inst & 0x7f) == 0x23
}

#[inline(always)]
fn ins_compressed_rd_rs2(inst: u32) -> u32 {
    (inst >> 2) & 0x7
}

const MATCH_LOAD: u32 = 0x03;
const MATCH_STORE: u32 = 0x23;

// decode load and store instruction, and return the EmuContext
// if the instruction is not a load or store instruction, return None
#[inline(always)]
fn inst_ldst_decode(inst: u32) -> Option<EmuContext> {
    // decode load and store instruction in load(store)_guest_page_fault_handler
    if is_compressed(inst) {
        if !is_compressed_ldst(inst) {
            None
        } else {
            Some(EmuContext {
                address: 0,
                width: 4,
                write: is_compressed_st(inst),
                sign_ext: true,
                reg: (ins_compressed_rd_rs2(inst) + 8) as usize,
                reg_width: 8,
            })
        }
    } else if !is_normal_ldst(inst) {
        None
    } else {
        let func3 = (inst >> 12) & 0x7;
        let reg = if is_normal_st(inst) {
            (inst >> 20) & 0x1f // rs2
        } else {
            (inst >> 7) & 0x1f // rd
        };
        Some(EmuContext {
            address: 0,
            width: inst_width(func3),
            write: is_normal_st(inst),
            sign_ext: (func3 & 0x4) == 0,
            reg: reg as usize,
            reg_width: 8,
        })
    }
}

pub fn ldst_guest_page_fault_handler(ctx: &mut ContextFrame) {
    // Faulting address's least-significant two bits are usually stval's least-significant two bits
    let addr = (riscv::register::htval::read() << 2) + (riscv::register::stval::read() & 0x3);
    let mut inst: u32 = riscv::register::htinst::read() as u32;
    let inst_size;

    if inst == 0 {
        // if inst does not provide info about the trap, we must read the instruction from the guest memory
        // and decode it
        let ins_addr = ctx.sepc;
        inst = read_inst_from_guest_mem(ins_addr);
        inst_size = get_ins_size(inst);
    } else if is_pseudo_ins(inst) {
        // TODO: we should reinject this in the guest as a fault access
        panic!("memory access fault on 1st stage(VS-stage) page table walk");
    } else {
        inst_size = transformed_inst_size(inst);
        inst |= 0b10;
    }

    // decode the instruction
    let decoded_emu_ctx = inst_ldst_decode(inst);

    if decoded_emu_ctx.is_none() {
        panic!("ldst_guest_page_fault_handler: unknown instruction\nctx: {}", ctx);
    }

    let mut emu_ctx = decoded_emu_ctx.unwrap();
    emu_ctx.address = addr;

    // find a handler to handle this mmio access
    if !emu_handler(&emu_ctx) {
        active_vm().unwrap().show_pagetable(emu_ctx.address);
        debug!(
            "write {}, width {}, reg width {}, addr {:x}, reg idx {}, reg val 0x{:x}",
            emu_ctx.write,
            emu_ctx.width,
            emu_ctx.reg_width,
            emu_ctx.address,
            emu_ctx.reg,
            current_cpu().get_gpr(emu_ctx.reg),
        );
        print_vs_regs();
        panic!(
            "data_abort_handler: Failed to handler emul device request, ipa 0x{:x}, sepc 0x{:x}\n{}",
            emu_ctx.address, ctx.sepc, ctx
        );
    }

    let val = ctx.sepc + inst_size as u64;
    current_cpu().set_elr(val as usize);
}
