// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

// Move to ARM register from system coprocessor register.
// MRS Xd, sysreg "Xd = sysreg"
macro_rules! mrs {
    ($reg: expr) => {
        {
            let r: u64;
            unsafe {
                core::arch::asm!(concat!("mrs {0}, ", stringify!($reg)), out(reg) r, options(nomem, nostack));
            }
            r
        }
    };
    ($reg: expr, "x") => {
        {
            let r: u32;
            unsafe {
                core::arch::asm!(concat!("mrs {0:x}, ", stringify!($reg)), out(reg) r, options(nomem, nostack));
            }
            r
        }
    };
    ($val: expr, $reg: expr, $asm_width:literal) => {
        unsafe {
            core::arch::asm!(concat!("mrs {0:", $asm_width, "}, ", stringify!($reg)), out(reg) $val, options(nomem, nostack));
        }
    };
    ($val: expr, $reg: expr) => {
        unsafe {
            core::arch::asm!(concat!("mrs {0}, ", stringify!($reg)), out(reg) $val, options(nomem, nostack));
        }
    };
}

// Move to system coprocessor register from ARM register.
// MSR sysreg, Xn "sysreg = Xn"
macro_rules! msr {
    ($reg: expr, $val: expr, $asm_width:tt) => {
        unsafe {
            core::arch::asm!(concat!("msr ", stringify!($reg), ", {0:", $asm_width, "}"), in(reg) $val, options(nomem, nostack));
        }
    };
    ($reg: expr, $val: expr) => {
        unsafe {
            core::arch::asm!(concat!("msr ", stringify!($reg), ", {0}"), in(reg) $val, options(nomem, nostack));
        }
    };
}

#[inline(always)]
pub const fn sysreg_enc_addr(op0: usize, op1: usize, crn: usize, crm: usize, op2: usize) -> usize {
    (((op0) & 0x3) << 20) | (((op2) & 0x7) << 17) | (((op1) & 0x7) << 14) | (((crn) & 0xf) << 10) | (((crm) & 0xf) << 1)
}

#[macro_export]
macro_rules! arm_at {
    ($at_op:expr, $addr:expr) => {
        unsafe {
            core::arch::asm!(concat!("AT ", $at_op, ", {0}"), in(reg) $addr, options(nomem, nostack));
            core::arch::asm!("isb");
        }
    };
}
