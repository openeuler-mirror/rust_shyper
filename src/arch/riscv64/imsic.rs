use crate::{ csrr, csrw};
/* AIA Extension */
pub const CSR_VSISELECT: usize = 0x250;
pub const CSR_VSIREG: usize = 0x251;
pub const CSR_VSTOPI: usize = 0xEB0;
pub const CSR_VSTOPEI: usize = 0x25C;

pub const IMSIC_VS: usize = 0x2800_1000;
const IMSIC_VS_HART_STRIDE: usize = 0x2000;

const XLEN: usize = usize::BITS as usize;
const XLEN_STRIDE: usize = XLEN / 32;

const EIP: usize = 0x80;

pub const fn imsic_vs(hart: usize) -> usize {
    IMSIC_VS + IMSIC_VS_HART_STRIDE * hart
}
pub fn imsic_write(reg: usize, val: usize) {
    #[allow(unused_unsafe)]
    unsafe {
        match reg {
            CSR_VSISELECT => csrw!(0x250, val),
            CSR_VSIREG => csrw!(0x251, val),
            CSR_VSTOPI => csrw!(0xEB0, val),
            CSR_VSTOPEI => csrw!(0x25C, val),
            _ => panic!("Unknown CSR {}", reg),
        }
    }
}

// Read from an IMSIC CSR

fn imsic_read(reg: usize) -> u64 {
    let ret: u64;
    #[allow(unused_unsafe)]
    unsafe {
        ret = match reg {
            CSR_VSISELECT => csrr!(0x250),
            CSR_VSIREG => csrr!(0x251),
            CSR_VSTOPI => csrr!(0xEB0),
            CSR_VSTOPEI => csrr!(0x25C),
            _ => panic!("Unknown CSR {}", reg),
        }
    }
    ret
}
// VS-Mode IMSIC CSRs


pub fn imsic_trigger(hart: u32, guest: u32, eiid: u32) {
    if guest == 1{
        unsafe {
            core::ptr::write_volatile(imsic_vs(hart as usize) as *mut u32, eiid);
        }
    } else {
        panic!(
            "Unknown imsic set hart {} guest {} eiid {}",
            hart, guest, eiid
        );
    }
}

