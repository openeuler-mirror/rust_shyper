use core::fmt;
use crate::{arch::VmContextTrait, csrr, csrw};
use super::{regs::RISCV_REG_NAME, A0_NUM, SP_NUM, SSTATUS_FS, SSTATUS_SD, SSTATUS_VS};

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct Riscv64ContextFrame {
    pub gpr: [u64; 32], // including sp
    pub sepc: u64,
    pub scause: u64,
    pub stval: u64,
    pub sstatus: u64,
    pub sscratch: u64,
}

pub fn print_vs_regs() {
    let mut ctx = VmContext::default();

    csrr!(ctx.vsstatus, vsstatus);
    csrr!(ctx.vsip, vsip);
    csrr!(ctx.vsie, vsie);
    csrr!(ctx.vstvec, vstvec);
    csrr!(ctx.vsscratch, vsscratch);
    csrr!(ctx.vsepc, vsepc);
    csrr!(ctx.vscause, vscause);
    csrr!(ctx.vstval, vstval);
    csrr!(ctx.vsatp, vsatp);

    info!("vsstatus: {:016x}", ctx.vsstatus);
    info!("vsip:     {:016x}", ctx.vsip);
    info!("vsie:     {:016x}", ctx.vsie);
    info!("vstvec:   {:016x}", ctx.vstvec);
    info!("vsscratch:{:016x}", ctx.vsscratch);
    info!("vsepc:    {:016x}", ctx.vsepc);
    info!("vscause:  {:016x}", ctx.vscause);
    info!("vstval:   {:016x}", ctx.vstval);
    info!("vsatp:    {:016x}", ctx.vsatp);
}

impl fmt::Display for Riscv64ContextFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        for i in 0..31 {
            write!(f, "${:03}: {:016x}   ", RISCV_REG_NAME[i + 1], self.gpr[i + 1])?;
            if (i + 1) % 2 == 0 {
                writeln!(f)?;
            }
        }
        writeln!(f, "sepc: {:016x}", self.sepc)?;
        write!(f, "scause: {:016x}", self.scause)?;
        writeln!(f, "   stval:  {:016x}", self.stval)?;
        writeln!(f, "sstatus: {:016x}", self.sstatus)?;
        writeln!(f, "sscratch: {:016x}", self.sscratch)?;
        Ok(())
    }
}

impl crate::arch::ContextFrameTrait for Riscv64ContextFrame {
    #[allow(unused_variables)]
    fn new(pc: usize, sp: usize, arg: usize) -> Self {
        let mut r = Riscv64ContextFrame {
            gpr: [0; 32],
            sepc: pc as u64,
            scause: 0,
            stval: 0,
            sstatus: 0,
            sscratch: 0,
        };
        r.set_argument(arg);
        r
    }

    fn exception_pc(&self) -> usize {
        self.sepc as usize
    }

    fn set_exception_pc(&mut self, pc: usize) {
        self.sepc = pc as u64;
    }

    fn stack_pointer(&self) -> usize {
        self.gpr[SP_NUM] as usize
    }

    fn set_stack_pointer(&mut self, sp: usize) {
        self.gpr[SP_NUM] = sp as u64;
    }

    fn set_argument(&mut self, arg: usize) {
        self.gpr[A0_NUM] = arg as u64
    }

    fn set_gpr(&mut self, index: usize, val: usize) {
        self.gpr[index] = val as u64;
    }

    fn gpr(&self, index: usize) -> usize {
        self.gpr[index] as usize
    }
}

impl Riscv64ContextFrame {
    pub fn default() -> Riscv64ContextFrame {
        Riscv64ContextFrame {
            gpr: [0; 32],
            sepc: 0,
            scause: 0,
            stval: 0,
            sstatus: 0,
            sscratch: 0,
        }
    }

    pub fn print_scause() {}
}

// represent as C struct format
#[repr(C)]
#[repr(align(16))]
#[derive(Copy, Clone, Debug, Default)]
pub struct VmCtxFpsimd {
    fp: [u64; 32],
    // TODO: save simd regs like vector
}

impl VmCtxFpsimd {
    pub fn reset(&mut self) {
        self.fp.iter_mut().for_each(|x| *x = 0);
    }
}

pub struct VmContext {
    fpsimd: VmCtxFpsimd,

    // hvip needs to be placed in the VmContext because interrupt injection is different for different VMs
    pub hvip: u64,
    // TODO: hstatus also needs to be placed here because the configuration of VTSR, VTW, SPV, etc. varies on different VMs
    pub hstatus: u64,

    // VS-mode registers
    pub vsstatus: u64,
    pub vsip: u64,
    pub vsie: u64,
    pub vstvec: u64,
    pub vsscratch: u64,
    pub vsepc: u64,
    pub vscause: u64,
    pub vstval: u64,
    pub vsatp: u64,

    // vcpu's cpuid
    pub cpuid: u64,

    pub next_timer_intr: u64,
}

const HSTATUS_SPV: u64 = 1 << 7;

impl Default for VmContext {
    fn default() -> Self {
        let hstatus_mem: u64;
        csrr!(hstatus_mem, hstatus);

        // Set **initial values** for each privilege level register
        // of the VM to prevent Linux startup errors
        Self {
            fpsimd: VmCtxFpsimd::default(),
            hvip: 0,
            hstatus: hstatus_mem | HSTATUS_SPV,
            vsstatus: SSTATUS_FS | SSTATUS_VS | SSTATUS_SD,
            vsip: 0,
            vsie: 0,
            vstvec: 0,
            vsscratch: 0,
            vsepc: 0,
            vscause: 0,
            vstval: 0,
            vsatp: 0,
            cpuid: 0,
            next_timer_intr: 0xffffffffffff,
        }
    }
}

impl VmContextTrait for VmContext {
    // Initialize all registers of the VmContext to 0
    fn reset(&mut self) {
        let hstatus_mem: u64;
        csrr!(hstatus_mem, hstatus);

        self.fpsimd.reset();

        self.hvip = 0;
        self.hstatus = hstatus_mem | HSTATUS_SPV;

        // clear all privilege regs
        self.vsstatus = SSTATUS_FS | SSTATUS_VS | SSTATUS_SD;
        self.vsip = 0;
        self.vsie = 0;
        self.vstvec = 0;
        self.vsscratch = 0;
        self.vsepc = 0;
        self.vscause = 0;
        self.vstval = 0;
        self.vsatp = 0;

        self.cpuid = 0;

        // Set it to a large value so that it does not trigger a timer interrupt
        self.next_timer_intr = 0xffffffffffff;
    }

    // Save some VS-mode privilege regs
    fn ext_regs_store(&mut self) {
        csrr!(self.vsstatus, vsstatus);
        csrr!(self.vsip, vsip);
        csrr!(self.vsie, vsie);
        csrr!(self.vstvec, vstvec);
        csrr!(self.vsscratch, vsscratch);
        csrr!(self.vsepc, vsepc);
        csrr!(self.vscause, vscause);
        csrr!(self.vstval, vstval);
        csrr!(self.vsatp, vsatp);

        csrr!(self.hvip, hvip);
        csrr!(self.hstatus, hstatus);
    }

    // Restore some VS-mode privilege level registers
    fn ext_regs_restore(&self) {
        csrw!(vsstatus, self.vsstatus);
        csrw!(vsip, self.vsip);
        csrw!(vsie, self.vsie);
        csrw!(vstvec, self.vstvec);
        csrw!(vsscratch, self.vsscratch);
        csrw!(vsepc, self.vsepc);
        csrw!(vscause, self.vscause);
        csrw!(vstval, self.vstval);
        csrw!(vsatp, self.vsatp);

        csrw!(hvip, self.hvip);
        csrw!(hstatus, self.hstatus);
    }

    // Save fp, simd regs
    fn fpsimd_save_context(&mut self) {
        // todo!()
    }

    // Restore fp, simd regs
    fn fpsimd_restore_context(&self) {
        // TODO:
        // todo!()
    }

    fn gic_save_state(&mut self) {}

    fn gic_restore_state(&self) {}

    fn gic_ctx_reset(&mut self) {}

    fn reset_vtimer_offset(&mut self) {
        todo!()
    }
}
