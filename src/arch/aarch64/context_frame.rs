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
use crate::arch::traits::InterruptContextTrait;
use core::fmt;
use crate::arch::timer_arch_get_counter;
use crate::arch::VmContextTrait;

use cortex_a::registers::*;

use crate::arch::{
    GICD, GicState, CNTVOFF_EL2, VMPIDR_EL2, SP_EL0, SP_EL1, ELR_EL1, SCTLR_EL1, CPACR_EL1, TTBR0_EL1, TTBR1_EL1,
    TCR_EL1, ESR_EL1, FAR_EL1, PAR_EL1, MAIR_EL1, AMAIR_EL1, VBAR_EL1, CONTEXTIDR_EL1, TPIDR_EL0, TPIDR_EL1, PMCR_EL0,
    HCR_EL2, ACTLR_EL1, CNTP_CTL_EL0, CNTKCTL_EL1, CNTV_CVAL_EL0, CNTV_TVAL_EL0, CNTV_CTL_EL0, TPIDRRO_EL0,
    CNTP_TVAL_EL0, CNTVCT_EL0,
};
use crate::arch::aarch64::regs::{WriteableReg, ReadableReg};

global_asm!(include_str!("fpsimd.S"));

extern "C" {
    /// Save the floating-point and SIMD registers.
    pub fn fpsimd_save_ctx(fpsimd_addr: usize);
    /// Restore the floating-point and SIMD registers.
    pub fn fpsimd_restore_ctx(fpsimd_addr: usize);
}

#[repr(C)]
#[derive(Copy, Clone)]
/// Context frame for AArch64.
pub struct Aarch64ContextFrame {
    gpr: [u64; 31],
    pub spsr: u64,
    elr: u64,
    sp: u64,
}

impl fmt::Display for Aarch64ContextFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        for i in 0..31 {
            write!(f, "x{:02}: {:016x}   ", i, self.gpr[i])?;
            if (i + 1) % 2 == 0 {
                writeln!(f)?;
            }
        }
        writeln!(f, "spsr:{:016x}", self.spsr)?;
        write!(f, "elr: {:016x}", self.elr)?;
        writeln!(f, "   sp:  {:016x}", self.sp)?;
        Ok(())
    }
}

impl fmt::Debug for Aarch64ContextFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "elr {:x} lr {:x}", self.elr, self.gpr[30])
    }
}

impl crate::arch::ContextFrameTrait for Aarch64ContextFrame {
    fn new(pc: usize, sp: usize, arg: usize) -> Self {
        let mut r = Aarch64ContextFrame {
            gpr: [0; 31],
            spsr: (SPSR_EL2::M::EL1h
                + SPSR_EL2::I::Masked
                + SPSR_EL2::F::Masked
                + SPSR_EL2::A::Masked
                + SPSR_EL2::D::Masked)
                .value,
            elr: pc as u64,
            sp: sp as u64,
        };
        r.set_argument(arg);
        r
    }

    fn exception_pc(&self) -> usize {
        self.elr as usize
    }

    fn set_exception_pc(&mut self, pc: usize) {
        self.elr = pc as u64;
    }

    fn stack_pointer(&self) -> usize {
        self.sp as usize
    }

    fn set_stack_pointer(&mut self, sp: usize) {
        self.sp = sp as u64;
    }

    fn set_argument(&mut self, arg: usize) {
        self.gpr[0] = arg as u64;
    }

    fn set_gpr(&mut self, index: usize, val: usize) {
        self.gpr[index] = val as u64;
    }

    fn gpr(&self, index: usize) -> usize {
        self.gpr[index] as usize
    }
}

impl Aarch64ContextFrame {
    pub fn default() -> Aarch64ContextFrame {
        Aarch64ContextFrame {
            gpr: [0; 31],
            spsr: (SPSR_EL2::M::EL1h
                + SPSR_EL2::I::Masked
                + SPSR_EL2::F::Masked
                + SPSR_EL2::A::Masked
                + SPSR_EL2::D::Masked)
                .value,
            elr: 0,
            sp: 0,
        }
    }
}

#[repr(C)]
#[repr(align(16))]
#[derive(Copy, Clone, Debug)]
/// Context frame for AArch64.
pub struct VmCtxFpsimd {
    fpsimd: [u64; 64],
    fpsr: u32,
    fpcr: u32,
}

impl VmCtxFpsimd {
    pub fn default() -> VmCtxFpsimd {
        VmCtxFpsimd {
            fpsimd: [0; 64],
            fpsr: 0,
            fpcr: 0,
        }
    }

    pub fn reset(&mut self) {
        self.fpsr = 0;
        self.fpcr = 0;
        self.fpsimd.iter_mut().for_each(|x| *x = 0);
    }
}

#[repr(C)]
#[repr(align(16))]
#[derive(Debug, Copy, Clone, Default)]
/// Arm GIC irq state struct
pub struct GicIrqState {
    pub id: u64,
    pub enable: u8,
    pub pend: u8,
    pub active: u8,
    pub priority: u8,
    pub target: u8,
}

#[repr(C)]
#[repr(align(16))]
#[derive(Debug, Copy, Clone, Default)]
/// Arm GIC context struct
pub struct GicContext {
    irq_num: usize,
    pub irq_state: [GicIrqState; 10],
    // hard code for vm irq num max
    gicv_ctlr: u32,
    gicv_pmr: u32,
}

impl GicContext {
    pub fn add_irq(&mut self, id: u64) {
        let idx = self.irq_num;
        self.irq_state[idx].id = id;
        self.irq_state[idx].enable = ((GICD.is_enabler(id as usize / 32) >> (id & 32)) & 1) as u8;
        self.irq_state[idx].priority = GICD.prio(id as usize) as u8;
        self.irq_state[idx].target = GICD.trgt(id as usize) as u8;
        self.irq_num += 1;
    }

    pub fn set_gicv_ctlr(&mut self, ctlr: u32) {
        self.gicv_ctlr = ctlr;
    }

    pub fn set_gicv_pmr(&mut self, pmr: u32) {
        self.gicv_pmr = pmr;
    }

    pub fn gicv_ctlr(&self) -> u32 {
        self.gicv_ctlr
    }

    pub fn gicv_pmr(&self) -> u32 {
        self.gicv_pmr
    }
}

#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
/// VM context struct
pub struct VmContext {
    // generic timer
    pub cntvoff_el2: u64,
    cntp_cval_el0: u64,
    cntv_cval_el0: u64,
    pub cntkctl_el1: u32,
    pub cntvct_el0: u64,
    cntp_ctl_el0: u32,
    cntv_ctl_el0: u32,
    cntp_tval_el0: u32,
    cntv_tval_el0: u32,

    // vpidr and vmpidr
    vpidr_el2: u32,
    pub vmpidr_el2: u64,

    // 64bit EL1/EL0 register
    sp_el0: u64,
    sp_el1: u64,
    elr_el1: u64,
    spsr_el1: u32,
    pub sctlr_el1: u32,
    actlr_el1: u64,
    cpacr_el1: u32,
    ttbr0_el1: u64,
    ttbr1_el1: u64,
    tcr_el1: u64,
    esr_el1: u32,
    far_el1: u64,
    par_el1: u64,
    mair_el1: u64,
    amair_el1: u64,
    vbar_el1: u64,
    contextidr_el1: u32,
    tpidr_el0: u64,
    tpidr_el1: u64,
    tpidrro_el0: u64,

    // hypervisor context
    pub hcr_el2: u64,
    cptr_el2: u64,
    hstr_el2: u64,
    pub pmcr_el0: u64,
    pub vtcr_el2: u64,

    // exception
    far_el2: u64,
    hpfar_el2: u64,
    fpsimd: VmCtxFpsimd,
    pub gic_state: GicState,
}

impl Default for VmContext {
    fn default() -> Self {
        Self {
            // generic timer
            cntvoff_el2: 0,
            cntp_cval_el0: 0,
            cntv_cval_el0: 0,
            cntkctl_el1: 0,
            cntvct_el0: 0,
            cntp_ctl_el0: 0,
            cntv_ctl_el0: 0,
            cntp_tval_el0: 0,
            cntv_tval_el0: 0,

            // vpidr and vmpidr
            vpidr_el2: 0,
            vmpidr_el2: 0,

            // 64bit EL1/EL0 register
            sp_el0: 0,
            sp_el1: 0,
            elr_el1: 0,
            spsr_el1: 0,
            sctlr_el1: 0x30C50830,
            actlr_el1: 0,
            cpacr_el1: 0,
            ttbr0_el1: 0,
            ttbr1_el1: 0,
            tcr_el1: 0,
            esr_el1: 0,
            far_el1: 0,
            par_el1: 0,
            mair_el1: 0,
            amair_el1: 0,
            vbar_el1: 0,
            contextidr_el1: 0,
            tpidr_el0: 0,
            tpidr_el1: 0,
            tpidrro_el0: 0,

            // hypervisor context
            hcr_el2: 0,
            cptr_el2: 0,
            hstr_el2: 0,

            // exception
            pmcr_el0: 0,
            vtcr_el2: if cfg!(feature = "lvl4") {
                (1 << 31)
                    + (VTCR_EL2::PS::PA_44B_16TB
                        + VTCR_EL2::TG0::Granule4KB
                        + VTCR_EL2::SH0::Inner
                        + VTCR_EL2::ORGN0::NormalWBRAWA
                        + VTCR_EL2::IRGN0::NormalWBRAWA
                        + VTCR_EL2::SL0.val(0b10) // 10: If TG0 is 00 (4KB granule), start at level 0.
                        + VTCR_EL2::T0SZ.val(64 - 44))
                    .value
            } else {
                0x8001355c
            },
            far_el2: 0,
            hpfar_el2: 0,
            fpsimd: VmCtxFpsimd::default(),
            gic_state: GicState::default(),
        }
    }
}

impl VmContextTrait for VmContext {
    fn reset(&mut self) {
        self.cntvoff_el2 = 0;
        self.cntp_cval_el0 = 0;
        self.cntv_cval_el0 = 0;
        self.cntp_tval_el0 = 0;
        self.cntv_tval_el0 = 0;
        self.cntkctl_el1 = 0;
        self.cntvct_el0 = 0;
        self.cntp_ctl_el0 = 0;
        self.vpidr_el2 = 0;
        self.vmpidr_el2 = 0;
        self.sp_el0 = 0;
        self.sp_el1 = 0;
        self.elr_el1 = 0;
        self.spsr_el1 = 0;
        self.sctlr_el1 = 0;
        self.actlr_el1 = 0;
        self.cpacr_el1 = 0;
        self.ttbr0_el1 = 0;
        self.ttbr1_el1 = 0;
        self.tcr_el1 = 0;
        self.esr_el1 = 0;
        self.far_el1 = 0;
        self.par_el1 = 0;
        self.mair_el1 = 0;
        self.amair_el1 = 0;
        self.vbar_el1 = 0;
        self.contextidr_el1 = 0;
        self.tpidr_el0 = 0;
        self.tpidr_el1 = 0;
        self.tpidrro_el0 = 0;
        self.hcr_el2 = 0;
        self.cptr_el2 = 0;
        self.hstr_el2 = 0;
        self.far_el2 = 0;
        self.hpfar_el2 = 0;
        self.fpsimd.reset();
    }

    fn ext_regs_store(&mut self) {
        self.cntvoff_el2 = CNTVOFF_EL2::read();
        self.cntv_cval_el0 = CNTV_CVAL_EL0::read();
        self.cntkctl_el1 = CNTKCTL_EL1::read() as u32;
        self.cntp_ctl_el0 = CNTP_CTL_EL0::read() as u32;
        self.cntv_ctl_el0 = CNTV_CTL_EL0::read() as u32;
        self.cntp_tval_el0 = CNTP_TVAL_EL0::read() as u32;
        self.cntv_tval_el0 = CNTV_TVAL_EL0::read() as u32;
        self.cntvct_el0 = CNTVCT_EL0::read();
        self.vmpidr_el2 = VMPIDR_EL2::read();
        self.sp_el0 = SP_EL0::read();
        self.sp_el1 = SP_EL1::read();
        self.elr_el1 = ELR_EL1::read();
        self.spsr_el1 = SPSR_EL1.get() as u32;
        self.sctlr_el1 = SCTLR_EL1::read() as u32;
        self.cpacr_el1 = CPACR_EL1::read() as u32;
        self.ttbr0_el1 = TTBR0_EL1::read();
        self.ttbr1_el1 = TTBR1_EL1::read();
        self.tcr_el1 = TCR_EL1::read();
        self.esr_el1 = ESR_EL1::read() as u32;
        self.far_el1 = FAR_EL1::read();
        self.par_el1 = PAR_EL1::read();
        self.mair_el1 = MAIR_EL1::read();
        self.amair_el1 = AMAIR_EL1::read();
        self.vbar_el1 = VBAR_EL1::read();
        self.contextidr_el1 = CONTEXTIDR_EL1::read() as u32;
        self.tpidr_el0 = TPIDR_EL0::read();
        self.tpidr_el1 = TPIDR_EL1::read();
        self.tpidrro_el0 = TPIDRRO_EL0::read();
        self.pmcr_el0 = PMCR_EL0::read();
        self.vtcr_el2 = VTCR_EL2.get();
        self.hcr_el2 = HCR_EL2::read();
        self.actlr_el1 = ACTLR_EL1::read();
    }

    fn ext_regs_restore(&self) {
        // SAFETY:
        // 1. The registers has defined as valid register
        // 2. The value is read from the register or
        //    the value is reset by the hypervisor correctly
        unsafe {
            CNTVOFF_EL2::write(self.cntvoff_el2);
            CNTV_CVAL_EL0::write(self.cntv_cval_el0);
            CNTKCTL_EL1::write(self.cntkctl_el1 as usize);
            CNTP_CTL_EL0::write(self.cntp_ctl_el0 as usize);
            VMPIDR_EL2::write(self.vmpidr_el2);
            SP_EL0::write(self.sp_el0);
            SP_EL1::write(self.sp_el1);
            ELR_EL1::write(self.elr_el1);
            SPSR_EL1.set(self.spsr_el1 as u64);
            SCTLR_EL1::write(self.sctlr_el1 as usize);
            CPACR_EL1::write(self.cpacr_el1 as usize);
            TTBR0_EL1::write(self.ttbr0_el1);
            TTBR1_EL1::write(self.ttbr1_el1);
            TCR_EL1::write(self.tcr_el1);
            ESR_EL1::write(self.esr_el1 as usize);
            FAR_EL1::write(self.far_el1);
            PAR_EL1::write(self.par_el1);
            MAIR_EL1::write(self.mair_el1);
            AMAIR_EL1::write(self.amair_el1);
            VBAR_EL1::write(self.vbar_el1);
            CONTEXTIDR_EL1::write(self.contextidr_el1 as usize);
            TPIDR_EL0::write(self.tpidr_el0);
            TPIDR_EL1::write(self.tpidr_el1);
            PMCR_EL0::write(self.pmcr_el0);
            VTCR_EL2.set(self.vtcr_el2);
            HCR_EL2::write(self.hcr_el2);
            ACTLR_EL1::write(self.actlr_el1);
        }
    }

    fn fpsimd_save_context(&mut self) {
        // SAFETY:
        // We use the address of fpsimd to save the all floating point register
        // eg. Q0-Q31, FPSR, FPCR
        // And the address is valid
        // And the value of fpsimd will be changed.
        unsafe {
            fpsimd_save_ctx(&self.fpsimd as *const _ as usize);
        }
    }

    fn fpsimd_restore_context(&self) {
        // SAFETY:
        // We use the address of fpsimd to restore the all floating point register
        // eg. Q0-Q31, FPSR, FPCR
        // And the address is valid
        unsafe {
            fpsimd_restore_ctx(&self.fpsimd as *const _ as usize);
        }
    }

    fn gic_save_state(&mut self) {
        self.gic_state.save_state();
    }

    fn gic_restore_state(&self) {
        self.gic_state.restore_state();
    }

    fn gic_ctx_reset(&mut self) {
        use crate::arch::gich_lrs_num;
        for i in 0..gich_lrs_num() {
            self.gic_state.lr[i] = 0;
        }
        self.gic_state.hcr |= 1 << 2; // init hcr
    }

    // Reset the el2 offset register
    // so that the virtual time of the virtual machine is consistent with the physical time
    fn reset_vtimer_offset(&mut self) {
        self.cntvoff_el2 = 0;
        let curpct = timer_arch_get_counter() as u64;
        self.cntvoff_el2 = curpct - self.cntvct_el0;
    }
}
