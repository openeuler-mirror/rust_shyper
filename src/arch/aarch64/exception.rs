// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use core::arch::global_asm;

use tock_registers::interfaces::*;

use crate::arch::{ContextFrameTrait, data_abort_handler, hvc_handler, smc_handler, sysreg_handler, isb, at, HPFAR_EL2};
use crate::arch::aarch64::regs::ReadableReg;
use crate::arch::{gicc_clear_current_irq, gicc_get_current_irq};
use crate::arch::ContextFrame;
use crate::kernel::{active_vm_id, current_cpu};
use crate::kernel::interrupt_handler;

global_asm!(include_str!("exception.S"));

#[inline(always)]
pub fn exception_esr() -> usize {
    cortex_a::registers::ESR_EL2.get() as usize
}

#[inline(always)]
pub fn exception_esr_el1() -> usize {
    cortex_a::registers::ESR_EL1.get() as usize
}

#[inline(always)]
fn exception_class() -> usize {
    (exception_esr() >> 26) & 0b111111
}

#[inline(always)]
fn exception_far() -> usize {
    cortex_a::registers::FAR_EL2.get() as usize
}

#[inline(always)]
fn exception_hpfar() -> usize {
    HPFAR_EL2::read() as usize
}

#[allow(non_upper_case_globals)]
const ESR_ELx_S1PTW_SHIFT: usize = 7;
#[allow(non_upper_case_globals)]
const ESR_ELx_S1PTW: usize = 1 << ESR_ELx_S1PTW_SHIFT;

fn translate_far_to_hpfar(far: usize) -> Result<usize, ()> {
    /*
     * We have
     *	PAR[PA_Shift - 1 : 12] = PA[PA_Shift - 1 : 12]
     *	HPFAR[PA_Shift - 9 : 4]  = FIPA[PA_Shift - 1 : 12]
     */
    // #define PAR_TO_HPFAR(par) (((par) & GENMASK_ULL(PHYS_MASK_SHIFT - 1, 12)) >> 8)
    fn par_to_far(par: u64) -> u64 {
        let mask = ((1 << (52 - 12)) - 1) << 12;
        (par & mask) >> 8
    }

    use cortex_a::registers::PAR_EL1;

    let par = PAR_EL1.get();
    at::s1e1r(far);
    isb();
    let tmp = PAR_EL1.get();
    PAR_EL1.set(par);
    if (tmp & PAR_EL1::F::TranslationAborted.value) != 0 {
        Err(())
    } else {
        Ok(par_to_far(tmp) as usize)
    }
}

// addr be ipa
#[inline(always)]
pub fn exception_fault_addr() -> usize {
    let far = exception_far();
    let hpfar = if (exception_esr() & ESR_ELx_S1PTW) == 0 && exception_data_abort_is_permission_fault() {
        translate_far_to_hpfar(far).unwrap_or_else(|_| {
            debug!("error happen in translate_far_to_hpfar");
            0
        })
    } else {
        exception_hpfar()
    };
    (far & 0xfff) | (hpfar << 8)
}

/// Get the length of the instruction that caused the exception
/// \return 1 means 32-bit instruction, 0 means 16-bit instruction
#[inline(always)]
fn exception_instruction_length() -> usize {
    (exception_esr() >> 25) & 1
}

#[inline(always)]
pub fn exception_next_instruction_step() -> usize {
    2 + 2 * exception_instruction_length()
}

#[inline(always)]
pub fn exception_iss() -> usize {
    exception_esr() & ((1 << 25) - 1)
}

#[inline(always)]
pub fn exception_data_abort_handleable() -> bool {
    (!(exception_iss() & (1 << 10)) | (exception_iss() & (1 << 24))) != 0
}

#[inline(always)]
pub fn exception_data_abort_is_translate_fault() -> bool {
    (exception_iss() & 0b111111 & (0xf << 2)) == 4
}

#[inline(always)]
pub fn exception_data_abort_is_permission_fault() -> bool {
    (exception_iss() & 0b111111 & (0xf << 2)) == 12
}

#[inline(always)]
pub fn exception_data_abort_access_width() -> usize {
    1 << ((exception_iss() >> 22) & 0b11)
}

#[inline(always)]
pub fn exception_data_abort_access_is_write() -> bool {
    (exception_iss() & (1 << 6)) != 0
}

#[inline(always)]
pub fn exception_data_abort_access_in_stage2() -> bool {
    (exception_iss() & (1 << 7)) != 0
}

#[inline(always)]
pub fn exception_data_abort_access_reg() -> usize {
    (exception_iss() >> 16) & 0b11111
}

#[inline(always)]
pub fn exception_data_abort_access_reg_width() -> usize {
    4 + 4 * ((exception_iss() >> 15) & 1)
}

#[inline(always)]
pub fn exception_data_abort_access_is_sign_ext() -> bool {
    ((exception_iss() >> 21) & 1) != 0
}

#[no_mangle]
extern "C" fn current_el_sp0_synchronous() {
    panic!("current_el_sp0_synchronous");
}

#[no_mangle]
extern "C" fn current_el_sp0_irq() {
    panic!("current_el_sp0_irq");
}

#[no_mangle]
extern "C" fn current_el_sp0_serror() {
    panic!("current_el0_serror");
}

#[no_mangle]
#[inline(never)]
extern "C" fn current_el_spx_synchronous() {
    panic!(
        "current_elx_synchronous core[{}] elr_el2 {:016x} sp_el0 {:016x}\n sp_el1 {:016x} sp_sel {:016x}\n",
        current_cpu().id,
        cortex_a::registers::ELR_EL2.get(),
        cortex_a::registers::SP_EL0.get(),
        cortex_a::registers::SP_EL1.get(),
        cortex_a::registers::SPSel.get(),
    );
}

#[no_mangle]
extern "C" fn current_el_spx_irq(ctx: &mut ContextFrame) {
    lower_aarch64_irq(ctx);
}

#[no_mangle]
extern "C" fn current_el_spx_serror() {
    panic!("current_elx_serror");
}

#[no_mangle]
extern "C" fn lower_aarch64_synchronous(ctx: &mut ContextFrame) {
    unsafe {
        current_cpu().set_ctx(ctx);
    }
    match exception_class() {
        0x24 => {
            data_abort_handler();
        }
        0x18 => {
            sysreg_handler(exception_iss() as u32);
        }
        0x17 => {
            smc_handler();
        }
        0x16 => {
            hvc_handler();
        }
        _ => {
            debug!(
                "x0 {:x}, x1 {:x}, x29 {:x}",
                (*ctx).gpr(0),
                (*ctx).gpr(1),
                (*ctx).gpr(29)
            );
            panic!(
                "core {} vm {}: handler not presents for EC_{:b} @ipa 0x{:x}, @pc 0x{:x}, @esr:0x{:x}",
                current_cpu().id,
                active_vm_id(),
                exception_class(),
                exception_fault_addr(),
                (*ctx).exception_pc(),
                exception_esr()
            );
        }
    }
    current_cpu().clear_ctx();
}

#[no_mangle]
extern "C" fn lower_aarch64_irq(ctx: &mut ContextFrame) {
    // SAFETY: ctx is a valid pointer
    unsafe {
        current_cpu().set_ctx(ctx);
    }
    if let Some(id) = gicc_get_current_irq() {
        if id >= 1022 {
            return;
        }
        // use crate::lib::time_current_us;
        // let begin = time_current_us();
        let handled_by_hypervisor = interrupt_handler(id);
        // let end = time_current_us();

        gicc_clear_current_irq(handled_by_hypervisor);
    }
    current_cpu().clear_ctx();
}

#[no_mangle]
extern "C" fn lower_aarch64_serror() {
    panic!("lower aarch64 serror");
}
