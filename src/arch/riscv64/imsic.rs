use crate::alloc::string::ToString;
use crate::{csrr, csrw};
/* AIA Extension */
pub const CSR_VSISELECT: usize = 0x250;
pub const CSR_VSIREG: usize = 0x251;
pub const CSR_VSTOPI: usize = 0xEB0;
pub const CSR_VSTOPEI: usize = 0x25C;

pub const IMSIC_VS: usize = 0x2800_0000;
const IMSIC_VS_HART_STRIDE: usize = 0x4000;
const IMSIC_MMIO_PAGE_SIZE: usize = 0x1000;

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

// Calculate how many bits are needed to represent all interrupt files.
fn imsic_num_bits(count: u32) -> u32 {
    let mut ret = 0;
    while (1 << ret) < count {
        ret += 1;
    }
    ret
}

pub fn imsic_trigger(hart: u32, guest: u32, eiid: u32) {
    let mut aia_guests: u32 = 3;
    // get the value of aia-guests from environment variables
    let arg = env!("AIA_GUESTS").to_string();
    match arg.parse::<u32>() {
        Ok(num) => aia_guests = num,
        Err(e) => warn!("Failed to parse: {}", e),
    }
    let guest_bits = imsic_num_bits(aia_guests + 1);
    let hart_stride = (1 << guest_bits) * IMSIC_MMIO_PAGE_SIZE as u32;
    let addr_base = IMSIC_VS as u32 + hart * hart_stride;
    let addr = addr_base + 0x1000 * guest;
    unsafe {
        core::ptr::write_volatile(addr as *mut u32, eiid);
    }
}
