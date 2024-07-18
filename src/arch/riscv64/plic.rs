use core::ptr::{read_volatile, write_volatile};
use alloc::vec::Vec;

pub struct PLIC {
    base_addr: usize,
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum PLICMode {
    Machine,
    Supervisor,
}

pub const PLIC_MAX_IRQ: usize = 511;
pub const MAX_HARTS: usize = 8;

pub const PLIC_PRIO_BEGIN: usize = 0x0000;
pub const PLIC_PRIO_END: usize = 0x0FFF;

pub const PLIC_PENDING_BEGIN: usize = 0x1000;
pub const PLIC_PENDING_END: usize = 0x1FFF;

pub const PLIC_ENABLE_BEGIN: usize = 0x2000;
pub const PLIC_ENABLE_END: usize = 0x1f_ffff;

pub const PLIC_THRESHOLD_CLAIM_BEGIN: usize = 0x20_0000;
pub const PLIC_THRESHOLD_CLAIM_END: usize = 0x3f_ffff;

/// PLIC features list:
/// * Get the current Pending interrupt list
/// * claim an interrupt
/// * complete an interrupt
/// * Get enable information
/// * Get priority information
/// * Set enable information
/// * Set priority information
/// * Set threshold information

pub trait PLICTrait {
    fn get_priority(&self, irq: usize) -> usize;
    fn set_priority(&self, irq: usize, priority: usize);
    fn get_pending(&self, irq: usize) -> bool;
    fn get_enable(&self, irq: usize, mode: PLICMode, hart: usize) -> bool;
    fn set_enable(&self, irq: usize, mode: PLICMode, hart: usize);
    fn clear_enable(&self, irq: usize, mode: PLICMode, hart: usize);
    fn get_threshold(&self, mode: PLICMode, hart: usize) -> usize;
    fn set_threshold(&self, mode: PLICMode, hart: usize, threshold: usize);
    fn get_claim(&self, mode: PLICMode, hart: usize) -> usize;
    fn set_complete(&self, mode: PLICMode, hart: usize, irq: usize);

    fn get_pending_list(&self) -> Vec<usize> {
        let mut pending_list = Vec::new();
        for irq in 1..=PLIC_MAX_IRQ {
            if self.get_pending(irq) {
                pending_list.push(irq);
            }
        }
        pending_list
    }
}

impl PLIC {
    pub const fn new(base_addr: usize) -> PLIC {
        PLIC { base_addr }
    }

    fn get_base_addr(&self) -> usize {
        self.base_addr
    }

    #[inline(always)]
    fn get_enable_array_addr(&self, irq: usize, mode: PLICMode, hart: usize) -> Option<usize> {
        // TODO: assert hart < hart count of platform
        if !(irq <= PLIC_MAX_IRQ && irq > 0) {
            return None;
        }

        // hart 0 don't support Supervisor (in sifive U740)
        #[cfg(feature = "board_u740")]
        if mode == PLICMode::Supervisor && hart == 0 {
            return None;
        }

        // divided into u740 and virt
        #[cfg(feature = "board_u740")]
        let res = match hart {
            0 => self.base_addr + PLIC_ENABLE_BEGIN + 0x4 * (irq / 32),
            _ => match mode {
                PLICMode::Machine => self.base_addr + PLIC_ENABLE_BEGIN + 0x80 + 0x100 * (hart - 1) + 0x4 * (irq / 32),
                PLICMode::Supervisor => {
                    self.base_addr + PLIC_ENABLE_BEGIN + 0x100 + 0x100 * (hart - 1) + 0x4 * (irq / 32)
                }
            },
        };
        #[cfg(not(feature = "board_u740"))]
        let res = match mode {
            PLICMode::Machine => self.base_addr + PLIC_ENABLE_BEGIN + 0x80 + 0x100 * hart + 0x4 * (irq / 32),
            PLICMode::Supervisor => self.base_addr + PLIC_ENABLE_BEGIN + 0x100 + 0x100 * hart + 0x4 * (irq / 32),
        };
        Some(res)
    }

    #[inline(always)]
    fn get_prio_claim_baseaddr(&self, mode: PLICMode, hart: usize) -> Option<usize> {
        // TODO: assert hart < hart count of platform
        // hart 0 don't support Supervisor (in sifive U740)

        #[cfg(feature = "board_u740")]
        if mode == PLICMode::Supervisor && hart == 0 {
            return None;
        }

        // divided into u740 and virt
        #[cfg(feature = "board_u740")]
        let res = match hart {
            0 => self.base_addr + PLIC_THRESHOLD_CLAIM_BEGIN,
            _ => match mode {
                PLICMode::Machine => self.base_addr + PLIC_THRESHOLD_CLAIM_BEGIN + 0x1000 + 0x2000 * (hart - 1),
                PLICMode::Supervisor => self.base_addr + PLIC_THRESHOLD_CLAIM_BEGIN + 0x2000 + 0x2000 * (hart - 1),
            },
        };
        #[cfg(not(feature = "board_u740"))]
        let res = match mode {
            PLICMode::Machine => self.base_addr + PLIC_THRESHOLD_CLAIM_BEGIN + 0x1000 + 0x2000 * hart,
            PLICMode::Supervisor => self.base_addr + PLIC_THRESHOLD_CLAIM_BEGIN + 0x2000 + 0x2000 * hart,
        };
        Some(res)
    }
}

impl PLICTrait for PLIC {
    fn get_priority(&self, irq: usize) -> usize {
        if !(irq <= PLIC_MAX_IRQ && irq > 0) {
            return 0;
        }

        let addr = self.base_addr + 0x4 * irq;
        // SAFETY:
        // addr is a valid PLIC device address.
        unsafe { read_volatile((addr) as *const u32) as usize }
    }

    fn set_priority(&self, irq: usize, priority: usize) {
        if !(irq <= PLIC_MAX_IRQ && irq > 0) {
            return;
        }

        let addr = self.base_addr + 0x4 * irq;
        // SAFETY:
        // addr is a valid PLIC device address that allows to write.
        unsafe {
            write_volatile((addr) as *mut u32, priority as u32);
        }
    }

    // pending bits are read only
    fn get_pending(&self, irq: usize) -> bool {
        if !(irq <= PLIC_MAX_IRQ && irq > 0) {
            return false;
        }

        let addr = self.base_addr + PLIC_PENDING_BEGIN + 0x4 * (irq / 32);
        // SAFETY:
        // addr is a valid PLIC device address that allows to read.
        unsafe { (read_volatile((addr) as *const u32) & (1 << (irq % 32))) != 0 }
    }

    fn get_enable(&self, irq: usize, mode: PLICMode, hart: usize) -> bool {
        let addr = self.get_enable_array_addr(irq, mode, hart);
        match addr {
            None => false,
            // SAFETY:
            // addr is a valid PLIC device address.
            Some(addr) => unsafe { (read_volatile((addr) as *const u32) & (1 << (irq % 32))) != 0 },
        }
    }

    fn set_enable(&self, irq: usize, mode: PLICMode, hart: usize) {
        let addr = self.get_enable_array_addr(irq, mode, hart);
        if let Some(addr) = addr {
            // SAFETY:
            // addr is a valid PLIC device address.
            unsafe {
                write_volatile(
                    addr as *mut u32,
                    read_volatile(addr as *mut u32) | (1 << (irq % 32)) as u32,
                );
            }
        }
    }

    fn clear_enable(&self, irq: usize, mode: PLICMode, hart: usize) {
        let addr = self.get_enable_array_addr(irq, mode, hart);
        if let Some(addr) = addr {
            // SAFETY:
            // addr is a valid PLIC device address.
            unsafe {
                write_volatile(
                    (addr) as *mut u32,
                    read_volatile((addr) as *mut u32) & !((1 << (irq % 32)) as u32),
                );
            }
        }
    }

    fn get_threshold(&self, mode: PLICMode, hart: usize) -> usize {
        let addr = self.get_prio_claim_baseaddr(mode, hart);
        match addr {
            None => 0,
            // SAFETY:
            // addr is a valid PLIC device address.
            Some(addr) => unsafe { read_volatile((addr) as *const u32) as usize },
        }
    }

    fn set_threshold(&self, mode: PLICMode, hart: usize, threshold: usize) {
        let addr = self.get_prio_claim_baseaddr(mode, hart);
        if let Some(addr) = addr {
            // SAFETY:
            // addr is a valid PLIC device address.
            unsafe {
                write_volatile(addr as *mut u32, threshold as u32);
            }
        }
    }

    fn get_claim(&self, mode: PLICMode, hart: usize) -> usize {
        let addr = self.get_prio_claim_baseaddr(mode, hart);
        if let Some(addr) = addr {
            let addr = addr + 0x4;
            // SAFETY:
            // addr is a valid PLIC device address.
            unsafe { read_volatile(addr as *mut u32) as usize }
        } else {
            0
        }
    }

    fn set_complete(&self, mode: PLICMode, hart: usize, irq: usize) {
        let addr = self.get_prio_claim_baseaddr(mode, hart);
        if let Some(addr) = addr {
            let addr = addr + 0x4;
            // SAFETY:
            // addr is a valid PLIC device address.
            unsafe {
                write_volatile((addr) as *mut u32, irq as u32);
            }
        }
    }
}
