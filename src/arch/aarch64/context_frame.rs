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
use crate::arch::traits::InterruptContextTriat;
use core::fmt;

use cortex_a::registers::*;

use crate::arch::{GICD, GicState};

global_asm!(include_str!("fpsimd.S"));

extern "C" {
    pub fn fpsimd_save_ctx(fpsimd_addr: usize);
    pub fn fpsimd_restore_ctx(fpsimd_addr: usize);
}

#[repr(C)]
#[derive(Copy, Clone)]
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
            spsr: (SPSR_EL1::M::EL1h
                + SPSR_EL1::I::Masked
                + SPSR_EL1::F::Masked
                + SPSR_EL1::A::Masked
                + SPSR_EL1::D::Masked)
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
            spsr: (SPSR_EL1::M::EL1h
                + SPSR_EL1::I::Masked
                + SPSR_EL1::F::Masked
                + SPSR_EL1::A::Masked
                + SPSR_EL1::D::Masked)
                .value,
            elr: 0,
            sp: 0,
        }
    }
}

#[repr(C)]
#[repr(align(16))]
#[derive(Copy, Clone, Debug)]
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
        // self.irq_state[idx].pend = id;
        // self.irq_state[idx].active = id;
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

impl VmContext {
    pub fn reset(&mut self) {
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

    pub fn ext_regs_store(&mut self) {
        mrs!(self.cntvoff_el2, CNTVOFF_EL2);
        // MRS!(self.cntp_cval_el0, CNTP_CVAL_EL0);
        mrs!(self.cntv_cval_el0, CNTV_CVAL_EL0);
        mrs!(self.cntkctl_el1, CNTKCTL_EL1, "x");
        mrs!(self.cntp_ctl_el0, CNTP_CTL_EL0, "x");
        mrs!(self.cntv_ctl_el0, CNTV_CTL_EL0, "x");
        mrs!(self.cntp_tval_el0, CNTP_TVAL_EL0, "x");
        mrs!(self.cntv_tval_el0, CNTV_TVAL_EL0, "x");
        mrs!(self.cntvct_el0, CNTVCT_EL0);
        // MRS!("self.vpidr_el2, VPIDR_EL2, "x");
        mrs!(self.vmpidr_el2, VMPIDR_EL2);

        mrs!(self.sp_el0, SP_EL0);
        mrs!(self.sp_el1, SP_EL1);
        mrs!(self.elr_el1, ELR_EL1);
        mrs!(self.spsr_el1, SPSR_EL1, "x");
        mrs!(self.sctlr_el1, SCTLR_EL1, "x");
        mrs!(self.cpacr_el1, CPACR_EL1, "x");
        mrs!(self.ttbr0_el1, TTBR0_EL1);
        mrs!(self.ttbr1_el1, TTBR1_EL1);
        mrs!(self.tcr_el1, TCR_EL1);
        mrs!(self.esr_el1, ESR_EL1, "x");
        mrs!(self.far_el1, FAR_EL1);
        mrs!(self.par_el1, PAR_EL1);
        mrs!(self.mair_el1, MAIR_EL1);
        mrs!(self.amair_el1, AMAIR_EL1);
        mrs!(self.vbar_el1, VBAR_EL1);
        mrs!(self.contextidr_el1, CONTEXTIDR_EL1, "x");
        mrs!(self.tpidr_el0, TPIDR_EL0);
        mrs!(self.tpidr_el1, TPIDR_EL1);
        mrs!(self.tpidrro_el0, TPIDRRO_EL0);

        mrs!(self.pmcr_el0, PMCR_EL0);
        mrs!(self.vtcr_el2, VTCR_EL2);
        mrs!(self.hcr_el2, HCR_EL2);
        // MRS!(self.cptr_el2, CPTR_EL2);
        // MRS!(self.hstr_el2, HSTR_EL2);
        // MRS!(self.far_el2, FAR_EL2);
        // MRS!(self.hpfar_el2, HPFAR_EL2);
        mrs!(self.actlr_el1, ACTLR_EL1);
        // println!("save sctlr {:x}", self.sctlr_el1);
    }

    pub fn ext_regs_restore(&self) {
        // println!("restore CNTV_CTL_EL0 {:x}", self.cntv_ctl_el0);
        // println!("restore CNTV_CVAL_EL0 {:x}", self.cntv_cval_el0);
        msr!(CNTVOFF_EL2, self.cntvoff_el2);
        // MSR!(CNTP_CVAL_EL0, self.cntp_cval_el0);
        msr!(CNTV_CVAL_EL0, self.cntv_cval_el0);
        msr!(CNTKCTL_EL1, self.cntkctl_el1, "x");
        // MSR!(CNTP_CTL_EL0, self.cntp_ctl_el0, "x");
        msr!(CNTV_CTL_EL0, self.cntv_ctl_el0, "x");
        // MSR!(CNTP_TVAL_EL0, {0:x}", in(reg) self.cntp_tval_el0, "x");
        // MSR!(CNTV_TVAL_EL0, {0:x}", in(reg) self.cntv_tval_el0, "x");

        // MSR!(VPIDR_EL2, self.vpidr_el2, "x");
        // println!("vmpidr_el2:{:#x}", self.vmpidr_el2);
        msr!(VMPIDR_EL2, self.vmpidr_el2);

        msr!(SP_EL0, self.sp_el0);
        msr!(SP_EL1, self.sp_el1);
        msr!(ELR_EL1, self.elr_el1);
        msr!(SPSR_EL1, self.spsr_el1, "x");
        msr!(SCTLR_EL1, self.sctlr_el1, "x");
        msr!(CPACR_EL1, self.cpacr_el1, "x");
        msr!(TTBR0_EL1, self.ttbr0_el1);
        msr!(TTBR1_EL1, self.ttbr1_el1);
        msr!(TCR_EL1, self.tcr_el1);
        msr!(ESR_EL1, self.esr_el1, "x");
        msr!(FAR_EL1, self.far_el1);
        msr!(PAR_EL1, self.par_el1);
        msr!(MAIR_EL1, self.mair_el1);
        msr!(AMAIR_EL1, self.amair_el1);
        msr!(VBAR_EL1, self.vbar_el1);
        msr!(CONTEXTIDR_EL1, self.contextidr_el1, "x");
        msr!(TPIDR_EL0, self.tpidr_el0);
        msr!(TPIDR_EL1, self.tpidr_el1);
        msr!(TPIDRRO_EL0, self.tpidrro_el0);

        msr!(PMCR_EL0, self.pmcr_el0);
        msr!(VTCR_EL2, self.vtcr_el2);
        msr!(HCR_EL2, self.hcr_el2);
        // MSR!(CPTR_EL2, self.cptr_el2);
        // MSR!(HSTR_EL2, self.hstr_el2);
        // MSR!(FAR_EL2, self.far_el2);
        // MSR!(HPFAR_EL2, self.hpfar_el2);
        msr!(ACTLR_EL1, self.actlr_el1);
    }

    pub fn fpsimd_save_context(&self) {
        unsafe {
            fpsimd_save_ctx(&self.fpsimd as *const _ as usize);
        }
    }

    pub fn fpsimd_restore_context(&self) {
        unsafe {
            fpsimd_restore_ctx(&self.fpsimd as *const _ as usize);
        }
    }

    pub fn gic_save_state(&mut self) {
        self.gic_state.save_state();
    }

    pub fn gic_restore_state(&self) {
        self.gic_state.restore_state();
    }
}
