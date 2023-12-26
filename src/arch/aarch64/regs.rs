// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use alloc::fmt;

pub trait SysReg {
    // Register value type.
    type Value: Copy + Eq + fmt::LowerHex + TryFrom<u64>;
    // Register name.
    const NAME: &'static str;
}

// Writeable system register.
pub trait WriteableReg: SysReg {
    // Write value to the register..
    /// # Safety:
    /// The caller must have enough privileges to operate on the register and ensure the value is valid
    unsafe fn write(val: Self::Value);
}

// Readable system register.
pub trait ReadableReg: SysReg {
    // Read register value.
    // no need to guarantee safety
    fn read() -> Self::Value;
}

// Move to ARM register from system coprocessor register.
// MRS Xd, sysreg "Xd = sysreg"
// It is a safe macro.
// Because the read behavior of the system register can't trigger any side effects.
macro_rules! mrs {
    ($reg: ident) => {
        {
            let r;
            unsafe {
                // TODO: If use 'nomem' option, the system will staff may beacuse of no assign type here
                core::arch::asm!(concat!("mrs {0}, ", stringify!($reg)), out(reg) r, options(pure,readonly, nostack));
            }
            r
        }
    };
    ($reg: ident, "x") => {
        {
            let r: u32;
            unsafe {
                core::arch::asm!(concat!("mrs {0:x}, ", stringify!($reg)), out(reg) r, options(nomem, nostack));
            }
            r
        }
    };
    ($val: expr, $reg: ident, $asm_width:literal) => {
        unsafe {
            core::arch::asm!(concat!("mrs {0:", $asm_width, "}, ", stringify!($reg)), out(reg) $val, options(nomem, nostack));
        }
    };
    ($val: expr, $reg: ident) => {
        unsafe {
            core::arch::asm!(concat!("mrs {0}, ", stringify!($reg)), out(reg) $val, options(nomem, nostack));
        }
    };
}

// Move to system coprocessor register from ARM register.
// MSR sysreg, Xn "sysreg = Xn"
/// # Safety:
/// It is a unsafe marco. The caller must ensure that the value is valid for the register.
macro_rules! msr {
    ($reg: ident, $val: expr, $asm_width:tt) => {
        unsafe{core::arch::asm!(concat!("msr ", stringify!($reg), ", {0:", $asm_width, "}"), in(reg) $val, options(nomem, nostack));}
    };
    ($reg: ident, $val: expr) => {
        unsafe{core::arch::asm!(concat!("msr ", stringify!($reg), ", {0:x}"), in(reg) $val, options(nomem, nostack));}
    };
}

// Declare a system register.
macro_rules! sysreg_declare {
    ($name: ident, $type:ty) => {
        #[allow(non_camel_case_types)]
        pub struct $name;

        impl SysReg for $name {
            type Value = $type;
            const NAME: &'static str = stringify!($name);
        }
    };
}

// Only used for Readable system register.
macro_rules! sysreg_impl_read {
    ($definename:ident, $asmname:ident) => {
        impl ReadableReg for $definename {
            #[inline]
            fn read() -> Self::Value {
                mrs!($asmname)
            }
        }
    };
}

// Only used for Writeable system register.
macro_rules! sysreg_impl_write {
    ($definename:ident, $asmname:ident) => {
        impl WriteableReg for $definename {
            #[inline]
            unsafe fn write(val: Self::Value) {
                msr!($asmname, val)
            }
        }
    };
}

// Define a Read-only system register so that it will panic when written.
macro_rules! define_sysreg_ro {
    ($definename:ident, $type:ty,$asmname:ident) => {
        sysreg_declare!($definename, $type);
        sysreg_impl_read!($definename, $asmname);
    };
    ($definename:ident, $type:ty) => {
        define_sysreg_ro!($definename, $type, $definename);
    };
    ($definename:ident) => {
        define_sysreg_ro!($definename, usize);
    };
}

// Define a Write-only system register so that it will panic when reading.
macro_rules! define_sysreg_wo {
    ($definename:ident, $type:ty, $asmname:ident) => {
        sysreg_declare!($definename, $type);
        sysreg_impl_write!($definename, $asmname);
    };
    ($definename:ident, $type:ty) => {
        define_sysreg_wo!($definename, $type, $definename);
    };
    ($definename:ident) => {
        define_sysreg_wo!($definename, usize);
    };
}

macro_rules! define_sysreg {
    ($definename:ident, $type:ty, $asmname:ident) => {
        sysreg_declare!($definename, $type);
        sysreg_impl_read!($definename, $asmname);
        sysreg_impl_write!($definename, $asmname);
    };
    ($definename:ident, $type:ty) => {
        define_sysreg!($definename, $type, $definename);
    };
    ($definename:ident) => {
        define_sysreg!($definename, usize);
    };
}

macro_rules! sysop {
    // # Safety:
    // It is a unsafe marco.The caller must ensure that the behavior won't
    // trigger any side effects.
    ($instr:ident) => {
        core::arch::asm!(
            stringify!($instr),
            options(nomem, nostack)
        )
    };
    // # Safety:
    // It is a unsafe marco.The caller must ensure that the behavior won't
    // trigger any side effects.
    ($instr:ident, $mode:ident) => {
        core::arch::asm!(
            concat!(stringify!($instr), " ", stringify!($mode)),
            options(nomem, nostack)
        )
    };
    // # Safety:
    // It is a unsafe marco.The caller must ensure that the behavior won't
    // trigger any side effects.
    // And the args must be a valid value for the instruction.
    ($instr:ident, $mode:ident, $args:expr) => {
        core::arch::asm!(
            concat!(stringify!($instr), " ", stringify!($mode), ", {0} "),
            in(reg) $args,
            options(nomem, nostack)
        )
    };
}

macro_rules! define_sysop {
    // SAFETY:
    // sysop without any passed value can't trigger any side effects.
    ($terms:ident) => {
        #[doc=concat!("System operation: ", stringify!($terms))]
        pub fn $terms() {
            unsafe {
                sysop!($terms);
            }
        }
    };
}

// Data Store Barrier ('DSB') instructions.
// SAFETY:
// DSB can't trigger any side effects.
// So it is a safe marco.
pub mod dsb {
    macro_rules! define_dsb {
        ($mode:ident) => {
            pub fn $mode() {
                unsafe {
                    sysop!(dsb, $mode);
                }
            }
        };
    }

    define_dsb!(oshld);
    define_dsb!(oshst);
    define_dsb!(osh);
    define_dsb!(nshld);
    define_dsb!(nshst);
    define_dsb!(nsh);
    define_dsb!(ishld);
    define_dsb!(ishst);
    define_dsb!(ish);
    define_dsb!(ld);
    define_dsb!(st);
    define_dsb!(sy);
}

// Data Synchronization Barrier ('DSB') instructions.
// SAFETY:
// DSB can't trigger any side effects.
pub mod dmb {
    macro_rules! define_dmb {
        ($mode:ident) => {
            pub fn $mode() {
                unsafe {
                    sysop!(dmb, $mode);
                }
            }
        };
    }

    define_dmb!(oshld);
    define_dmb!(oshst);
    define_dmb!(osh);
    define_dmb!(nshld);
    define_dmb!(nshst);
    define_dmb!(nsh);
    define_dmb!(ishld);
    define_dmb!(ishst);
    define_dmb!(ish);
    define_dmb!(ld);
    define_dmb!(st);
    define_dmb!(sy);
}

// Address Translation ('AT') Instructions.
pub mod at {
    macro_rules! define_at {
        ($mode:ident) => {
            pub fn $mode(va: usize) {
                unsafe {
                    sysop!(at, $mode, va as u64);
                }
            }
        };
    }

    define_at!(s1e1r);
}

// Define a system with RW access.
define_sysreg!(DAIF, u64);
define_sysreg!(CNTVOFF_EL2, u64); //Counter-timer Virtual Offset register
define_sysreg!(CNTV_CVAL_EL0, u64); //Counter-timer Virtual Timer CompareValue register
define_sysreg!(CNTKCTL_EL1); //Counter-timer Kernel Control register
define_sysreg!(CNTP_CTL_EL0); // Counter-timer Physical Timer Control register
define_sysreg!(CNTV_CTL_EL0); // Counter-timer Virtual Timer Control register
define_sysreg!(CNTP_TVAL_EL0); // Counter-timer Physical Timer TimerValue register
define_sysreg!(CNTV_TVAL_EL0); // Counter-timer Virtual Timer TimerValue register
define_sysreg!(CNTVCT_EL0, u64); // Counter-timer Virtual Count register
define_sysreg!(VMPIDR_EL2, u64); // Virtualization Multiprocessor ID Register
define_sysreg!(SP_EL0, u64); // Stack Pointer EL0
define_sysreg!(SP_EL1, u64); // Stack Pointer EL1
define_sysreg!(ELR_EL1, u64); // Exception Link Register EL1
define_sysreg!(ELR_EL2, u64); // Exception Link Register EL2
define_sysreg!(SPSR_EL1); // Saved Program Status Register EL1
define_sysreg!(SPSR_EL2); // Saved Program Status Register EL2
define_sysreg!(SCTLR_EL1); // System Control Register EL1
define_sysreg!(CPACR_EL1); // Architectural Feature Access Control Register EL1
define_sysreg!(TTBR0_EL1, u64); // Translation Table Base Register 0 EL1
define_sysreg!(TTBR1_EL1, u64); // Translation Table Base Register 1 EL1
define_sysreg!(TCR_EL1, u64); // Translation Control Register EL1
define_sysreg!(ESR_EL1); // Exception Syndrome Register EL1
define_sysreg!(ESR_EL2); // Exception Syndrome Register EL2
define_sysreg!(FAR_EL1, u64); // Fault Address Register EL1
define_sysreg!(FAR_EL2, u64); // Fault Address Register EL2
define_sysreg!(MAIR_EL1, u64); // Memory Attribute Indirection Register EL1
define_sysreg!(MAIR_EL2, u64); // Memory Attribute Indirection Register EL2
define_sysreg!(AMAIR_EL1, u64); // Auxiliary Memory Attribute Indirection Register EL1
define_sysreg!(AMAIR_EL2, u64); // Auxiliary Memory Attribute Indirection Register EL2
define_sysreg!(VBAR_EL1, u64); // Vector Base Address Register EL1
define_sysreg!(VBAR_EL2, u64); // Vector Base Address Register EL2
define_sysreg!(PAR_EL1, u64); // Physical Address Register EL1
define_sysreg!(PAR_EL2, u64); // Physical Address Register EL2
define_sysreg!(TPIDR_EL0, u64); // Software Thread ID Register EL0
define_sysreg!(TPIDR_EL1, u64); // Software Thread ID Register EL1
define_sysreg!(TPIDR_EL2, u64); // Software Thread ID Register EL2
define_sysreg!(SPSEL, u64); // Stack Pointer Select
define_sysreg!(CONTEXTIDR_EL1); // Context ID Register EL1
define_sysreg!(PMCR_EL0, u64); // Performance Monitors Control Register EL0
define_sysreg!(VTCR_EL2, u64); // Virtualization Translation Control Register EL2
define_sysreg!(HCR_EL2, u64); // Hypervisor Configuration Register EL2
define_sysreg!(ACTLR_EL1, u64); // Auxiliary Control Register EL1
define_sysreg!(ACTLR_EL2, u64); // Auxiliary Control Register EL2
define_sysreg!(HPFAR_EL2, u64); // Hypervisor IPA Fault Address Register EL2
define_sysreg!(AFSR0_EL1, u64); // Auxiliary Fault Status Register 0 EL1
define_sysreg!(CNTHP_TVAL_EL2); // Counter-timer Hypervisor Physical Timer TimerValue register
define_sysreg!(CNTHP_CTL_EL2); // Counter-timer Hypervisor Physical Timer Control register
define_sysreg!(VTTBR_EL2); // Virtualization Translation Table Base Register EL2
                           // define GIC_V3 system register
define_sysreg!(ICH_HCR_EL2); // Interrupt Controller Hypervisor Control Register
define_sysreg!(ICC_SRE_EL2); // Interrupt Controller System Register Enable Register
define_sysreg!(ICC_SRE_EL1); // Interrupt Controller System Register Enable Register
define_sysreg!(ICH_VMCR_EL2); // Interrupt Controller Virtual Machine Control Register
define_sysreg!(ICH_AP0R2_EL2); // Interrupt Controller Active Priorities Group 0 Register 2
define_sysreg!(ICH_AP0R1_EL2); // Interrupt Controller Active Priorities Group 0 Register 1
define_sysreg!(ICH_AP0R0_EL2); // Interrupt Controller Active Priorities Group 0 Register 0
define_sysreg!(ICH_AP1R2_EL2); // Interrupt Controller Active Priorities Group 1 Register 2
define_sysreg!(ICH_AP1R1_EL2); // Interrupt Controller Active Priorities Group 1 Register 1
define_sysreg!(ICH_AP1R0_EL2); // Interrupt Controller Active Priorities Group 1 Register 0
define_sysreg!(ICC_PMR_EL1); // Interrupt Controller Interrupt Priority Mask Register
define_sysreg!(ICC_BPR1_EL1); // Interrupt Controller Binary Point Register 1
define_sysreg!(ICC_CTLR_EL1); // Interrupt Controller Control Register
define_sysreg!(ICC_IGRPEN1_EL1); // Interrupt Controller Interrupt Group 1 Enable register
define_sysreg!(ICC_EOIR1_EL1); // Interrupt Controller End Of Interrupt Register 1
define_sysreg!(ICC_DIR_EL1); // Interrupt Controller Deactivate Interrupt Register
define_sysreg!(ICH_ELRSR_EL2); // Interrupt Controller Empty List Register Status Register
define_sysreg!(ICH_LR0_EL2); // Interrupt Controller List Register 0
define_sysreg!(ICH_LR1_EL2); // Interrupt Controller List Register 1
define_sysreg!(ICH_LR2_EL2); // Interrupt Controller List Register 2
define_sysreg!(ICH_LR3_EL2); // Interrupt Controller List Register 3
define_sysreg!(ICH_LR4_EL2); // Interrupt Controller List Register 4
define_sysreg!(ICH_LR5_EL2); // Interrupt Controller List Register 5
define_sysreg!(ICH_LR6_EL2); // Interrupt Controller List Register 6
define_sysreg!(ICH_LR7_EL2); // Interrupt Controller List Register 7
define_sysreg!(ICH_LR8_EL2); // Interrupt Controller List Register 8
define_sysreg!(ICH_LR9_EL2); // Interrupt Controller List Register 9
define_sysreg!(ICH_LR10_EL2); // Interrupt Controller List Register 10
define_sysreg!(ICH_LR11_EL2); // Interrupt Controller List Register 11
define_sysreg!(ICH_LR12_EL2); // Interrupt Controller List Register 12
define_sysreg!(ICH_LR13_EL2); // Interrupt Controller List Register 13
define_sysreg!(ICH_LR14_EL2); // Interrupt Controller List Register 14
define_sysreg!(ICH_LR15_EL2); // Interrupt Controller List Register 15

// Define a system with RO access.
define_sysreg_ro!(TPIDRRO_EL0, u64); // Software Thread ID Register EL0 Read-Only
define_sysreg_ro!(MPIDR_EL1, u64); // Multiprocessor Affinity Register EL1
define_sysreg_ro!(ICC_IAR1_EL1); // Interrupt Controller Interrupt Acknowledge Register 1
define_sysreg_ro!(ICH_EISR_EL2); // Interrupt Controller Empty Interrupt Status Register
define_sysreg_ro!(ICH_MISR_EL2); // Interrupt Controller Maintenance Interrupt Status Register
define_sysreg_ro!(ICH_VTR_EL2); // Interrupt Controller Virtualization Type Register

// Define a system with WO access.
define_sysreg_wo!(OSLAR_EL1); // OS Lock Access Register EL1
define_sysreg_wo!(ICC_SGI1R_EL1, u64); // Interrupt Controller System Register

// Define a system operation.
define_sysop!(wfi);
define_sysop!(wfe);
define_sysop!(sev);
define_sysop!(sevl);
define_sysop!(isb);

#[inline(always)]
pub const fn sysreg_enc_addr(op0: usize, op1: usize, crn: usize, crm: usize, op2: usize) -> usize {
    (((op0) & 0x3) << 20) | (((op2) & 0x7) << 17) | (((op1) & 0x7) << 14) | (((crn) & 0xf) << 10) | (((crm) & 0xf) << 1)
}
