#[macro_export]
// Read register's value to memory
macro_rules! csrr {
    ($reg: expr) => {
        {
            let r: u64;
            unsafe {
                core::arch::asm!(concat!("csrr {0}, ", stringify!($reg)), out(reg) r, options(nomem, nostack));
            }
            r
        }
    };
    ($val: expr, $reg: expr, $asm_width:tt) => {
        unsafe {
            core::arch::asm!(concat!("csrr {0:", $asm_width, "}, ", stringify!($reg)), out(reg) $val, options(nomem, nostack));
        }
    };
    ($val: expr, $reg: expr) => {
        unsafe {
            core::arch::asm!(concat!("csrr {0}, ", stringify!($reg)), out(reg) $val, options(nomem, nostack));
        }
    };
}

#[macro_export]
macro_rules! csrw {
    ($reg: expr, $val: expr, $asm_width:tt) => {
        unsafe {
            core::arch::asm!(concat!("csrw ", stringify!($reg), ", {0:", $asm_width, "}"), in(reg) $val, options(nomem, nostack));
        }
    };
    ($reg: expr, $val: expr) => {
        unsafe {
            core::arch::asm!(concat!("csrw ", stringify!($reg), ", {0}"), in(reg) $val, options(nomem, nostack));
        }
    };
}

pub const RISCV_REG_NAME: [&str; 32] = [
    "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "fp", "s1", "a0", "a1", "a2", "a3", "a4", "a5", "a6", "a7", "s2",
    "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "t3", "t4", "t5", "t6",
];

pub const A0_NUM: usize = 10;
pub const A1_NUM: usize = 11;
pub const A2_NUM: usize = 12;
pub const A3_NUM: usize = 13;
pub const A4_NUM: usize = 14;
pub const A5_NUM: usize = 15;
pub const A6_NUM: usize = 16;
pub const A7_NUM: usize = 17;

pub const SP_NUM: usize = 2;
pub const TP_NUM: usize = 4;

#[inline(always)]
pub fn get_index_by_regname(reg_name: &str) -> usize {
    for i in 0..RISCV_REG_NAME.len() {
        if RISCV_REG_NAME[i] == reg_name {
            return i;
        }
    }
    panic!("invalid reg name: {}", reg_name);
}

pub const SSTATUS_SUM: u64 = 1 << 18;
pub const SSTATUS_FS: u64 = 0x00006000;
// TODO: Introduces dynamic storage of floating point and vector registers, using FS and VS flags to decide whether to save these registers
// Initially turn off the floating point function, and then turn on the VM after it uses the floating point function
pub const SSTATUS_VS: u64 = 3 << 9;

pub const SSTATUS_SD: u64 = 1 << 63;

pub const SSTATUS_SPP: u64 = 1 << 8;
pub const SSTATUS_SPIE: u64 = 1 << 5;
pub const SSTATUS_SIE: u64 = 1 << 1;
