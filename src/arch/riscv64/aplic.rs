use core::ptr::{read_volatile, write_volatile};

// S-mode interrupt delivery controller
// const APLIC_S_IDC: usize = 0xd00_4000;
// domaincfg
pub const APLIC_DOMAINCFG_BASE: usize = 0x0000;
pub const APLIC_DOMAINCFG_TOP: usize = 0x0003;
// sourcecfg
pub const APLIC_SOURCECFG_BASE: usize = 0x0004;
pub const APLIC_SOURCECFG_TOP: usize = 0x0FFF;
//smsiaddrcfg
pub const APLIC_S_MSIADDR_BASE: usize = 0x1BC8;
pub const APLIC_S_MSIADDR_TOP: usize = 0x1BCF;
// setip
pub const APLIC_SET_PENDING_BASE: usize = 0x1C00;
pub const APLIC_SET_PENDING_TOP: usize = 0x1C7F;
// setipnum
pub const APLIC_SET_PENDING_NUM_BASE: usize = 0x1CDC;
pub const APLIC_SET_PENDING_NUM_TOP: usize = 0x1CDF;
// inclrip
pub const APLIC_CLR_PENDING_BASE: usize = 0x1D00;
pub const APLIC_CLR_PENDING_TOP: usize = 0x1D7F;
// clripnum
pub const APLIC_CLR_PENDING_NUM_BASE: usize = 0x1DDC;
pub const APLIC_CLR_PENDING_NUM_TOP: usize = 0x1DDF;
// setie
pub const APLIC_SET_ENABLE_BASE: usize = 0x1E00;
pub const APLIC_SET_ENABLE_TOP: usize = 0x1E7F;
// setienum
pub const APLIC_SET_ENABLE_NUM_BASE: usize = 0x1EDC;
pub const APLIC_SET_ENABLE_NUM_TOP: usize = 0x1EDF;
// clrie
pub const APLIC_CLR_ENABLE_BASE: usize = 0x1F00;
pub const APLIC_CLR_ENABLE_TOP: usize = 0x1F7F;
// clrienum
pub const APLIC_CLR_ENABLE_NUM_BASE: usize = 0x1FDC;
pub const APLIC_CLR_ENABLE_NUM_TOP: usize = 0x1FDF;
// setipnum_le
pub const APLIC_SET_IPNUM_LE_BASE: usize = 0x2000;
pub const APLIC_SET_IPNUM_LE_TOP: usize = 0x2003;
// setipnum_be
pub const APLIC_SET_IPNUM_BE_BASE: usize = 0x2004;
pub const APLIC_SET_IPNUM_BE_TOP: usize = 0x2007;
// genmsi
pub const APLIC_GENMSI_BASE: usize = 0x3000;
pub const APLIC_GENMSI_TOP: usize = 0x3003;
// target
pub const APLIC_TARGET_BASE: usize = 0x3004;
pub const APLIC_TARGET_TOP: usize = 0x3FFF;
// IDC
pub const APLIC_IDC_BASE: usize = 0x4000;

#[repr(u32)]
#[allow(dead_code)]
#[derive(PartialEq, Clone, Copy, Debug)]
pub enum SourceModes {
    Inactive = 0,
    Detached = 1,
    RisingEdge = 4,
    FallingEdge = 5,
    LevelHigh = 6,
    LevelLow = 7,
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum APLICMode {
    Machine,
    Supervisor,
}

// offset size register name
// 0x0000 4 bytes domaincfg
// ----------------------------------
// 0x0004 4 bytes sourcecfg[1]
// 0x0008 4 bytes sourcecfg[2]
// . . . . . .
// 0x0FFC 4 bytes sourcecfg[1023]
// ----------------------------------
// 0x1BC0 4 bytes mmsiaddrcfg (machine-level interrupt domains only)
// 0x1BC4 4 bytes mmsiaddrcfgh ”
// 0x1BC8 4 bytes smsiaddrcfg ”
// 0x1BCC 4 bytes smsiaddrcfgh ”
// ----------------------------------
// 0x1C00 4 bytes setip[0]
// 0x1C04 4 bytes setip[1]
// . . . . . .
// 0x1C7C 4 bytes setip[31]
// ----------------------------------
// 0x1CDC 4 bytes setipnum
// ----------------------------------
// 0x1D00 4 bytes in clrip[0]
// 0x1D04 4 bytes in clrip[1]
// . . . . . .
// 0x1D7C 4 bytes in clrip[31]
// ----------------------------------
// 0x1DDC 4 bytes clripnum
// ----------------------------------
// 0x1E00 4 bytes setie[0]
// 0x1E04 4 bytes setie[1]
// . . . . . .
// 0x1E7C 4 bytes setie[31]
// ----------------------------------
// 0x1EDC 4 bytes setienum
// ----------------------------------
// 0x1F00 4 bytes clrie[0]
// 0x1F04 4 bytes clrie[1]
// . . . . . .
// 0x1F7C 4 bytes clrie[31]
// ----------------------------------
// 0x1FDC 4 bytes clrienum
// ----------------------------------
// 0x2000 4 bytes setipnum le
// 0x2004 4 bytes setipnum be
// ----------------------------------
// 0x3000 4 bytes genmsi
// ----------------------------------
// 0x3004 4 bytes target[1]
// 0x3008 4 bytes target[2]
// . . . . . .
// 0x3FFC 4 bytes target[1023]
// ----------------------------------

pub struct APLIC {
    base: usize,
    size: usize,
}

pub trait APLICTrait {
    fn set_domaincfg(&self, bigendian: bool, msimode: bool, enabled: bool);
    fn get_domaincfg(&self) -> u32;
    fn get_msimode(&self) -> bool;
    fn set_sourcecfg(&self, irq: u32, mode: SourceModes);
    fn set_sourcecfg_delegate(&self, irq: u32, child: u32);
    fn get_sourcecfg(&self, irq: u32) -> u32;
    fn set_msiaddr(&self, address: usize);
    fn get_pending(&self, irqidx: usize) -> u32;
    fn set_pending(&self, irqidx: usize, value: u32, pending: bool);
    fn set_pending_num(&self, value: u32);
    fn get_in_clrip(&self, irqidx: usize) -> u32;
    fn get_enable(&self, irqidx: usize) -> u32;
    fn get_clr_enable(&self, irqidx: usize) -> u32;
    fn set_enable(&self, irqidx: usize, value: u32, enabled: bool);
    fn set_enable_num(&self, value: u32);
    fn clr_enable_num(&self, value: u32);
    fn setipnum_le(&self, value: u32);
    fn set_target_msi(&self, irq: u32, hart: u32, guest: u32, eiid: u32);
    fn set_target_direct(&self, irq: u32, hart: u32, prio: u32);
}

#[allow(dead_code)]
impl APLIC {
    pub const fn new(base: usize, size: usize) -> APLIC {
        APLIC { base, size }
    }

    fn get_base_addr(&self) -> usize {
        self.base
    }
}
/// Interrupt Delivery Control is only used in 'direct' mode
#[repr(C)]
struct InterruptDeliveryControl {
    pub idelivery: u32,
    pub iforce: u32,
    pub ithreshold: u32,
    pub topi: u32,
    pub claimi: u32,
}

impl APLICTrait for APLIC {
    fn set_domaincfg(&self, bigendian: bool, msimode: bool, enabled: bool) {
        let enabled = u32::from(enabled);
        let msimode = u32::from(msimode);
        let bigendian = u32::from(bigendian);
        let addr = self.base + APLIC_DOMAINCFG_BASE;
        let src = (enabled << 8) | (msimode << 2) | bigendian;
        unsafe {
            write_volatile(addr as *mut u32, src);
        }
    }

    fn get_domaincfg(&self) -> u32 {
        let addr = self.base + APLIC_DOMAINCFG_BASE;
        unsafe { read_volatile(addr as *const u32) }
    }

    fn get_msimode(&self) -> bool {
        let addr = self.base + APLIC_DOMAINCFG_BASE;
        let value = unsafe { read_volatile(addr as *const u32) };
        ((value >> 2) & 0b11) != 0
    }

    fn set_sourcecfg(&self, irq: u32, mode: SourceModes) {
        assert!(irq > 0 && irq < 1024);
        let addr = self.base + APLIC_SOURCECFG_BASE + (irq as usize - 1) * 4;
        let src = mode as u32;
        unsafe {
            write_volatile(addr as *mut u32, src);
        }
    }

    fn set_sourcecfg_delegate(&self, irq: u32, child: u32) {
        assert!(irq > 0 && irq < 1024);
        let addr = self.base + APLIC_SOURCECFG_BASE + (irq as usize - 1) * 4;
        let src = 1 << 10 | child & 0x3ff;
        unsafe {
            write_volatile(addr as *mut u32, src);
        }
    }

    fn get_sourcecfg(&self, irq: u32) -> u32 {
        assert!(irq > 0 && irq < 1024);
        let addr = self.base + APLIC_SOURCECFG_BASE + (irq as usize - 1) * 4;
        unsafe { read_volatile(addr as *const u32) }
    }

    fn set_msiaddr(&self, address: usize) {
        let addr = self.base + APLIC_S_MSIADDR_BASE;
        let src = (address >> 12) as u32;
        unsafe {
            write_volatile(addr as *mut u32, src);
            write_volatile((addr + 4) as *mut u32, 0);
        }
    }

    fn get_pending(&self, irqidx: usize) -> u32 {
        assert!(irqidx < 32);
        let addr = self.base + APLIC_SET_PENDING_BASE + irqidx * 4;
        unsafe { read_volatile(addr as *const u32) }
    }

    fn set_pending(&self, irqidx: usize, value: u32, pending: bool) {
        assert!(irqidx < 32);
        let addr = self.base + APLIC_SET_PENDING_BASE + irqidx * 4;
        let clr_addr = self.base + APLIC_CLR_PENDING_BASE + irqidx * 4;
        if pending {
            unsafe {
                write_volatile(addr as *mut u32, value);
            }
        } else {
            unsafe {
                write_volatile(clr_addr as *mut u32, value);
            }
        }
    }

    fn set_pending_num(&self, value: u32) {
        let addr = self.base + APLIC_SET_PENDING_NUM_BASE;
        unsafe {
            write_volatile(addr as *mut u32, value);
        }
    }

    fn get_in_clrip(&self, irqidx: usize) -> u32 {
        assert!(irqidx < 32);
        let addr = self.base + APLIC_CLR_PENDING_BASE + irqidx * 4;
        unsafe { read_volatile(addr as *const u32) }
    }

    fn get_enable(&self, irqidx: usize) -> u32 {
        assert!(irqidx < 32);
        let addr = self.base + APLIC_SET_ENABLE_BASE + irqidx * 4;
        unsafe { read_volatile(addr as *const u32) }
    }

    fn get_clr_enable(&self, irqidx: usize) -> u32 {
        assert!(irqidx < 32);
        let addr = self.base + APLIC_CLR_ENABLE_BASE + irqidx * 4;
        unsafe { read_volatile(addr as *const u32) }
    }

    fn set_enable(&self, irqidx: usize, value: u32, enabled: bool) {
        assert!(irqidx < 32);
        let addr = self.base + APLIC_SET_ENABLE_BASE + irqidx * 4;
        let clr_addr = self.base + APLIC_CLR_ENABLE_BASE + irqidx * 4;
        if enabled {
            unsafe {
                write_volatile(addr as *mut u32, value);
            }
        } else {
            unsafe {
                write_volatile(clr_addr as *mut u32, value);
            }
        }
    }

    fn set_enable_num(&self, value: u32) {
        let addr = self.base + APLIC_SET_ENABLE_NUM_BASE;
        unsafe {
            write_volatile(addr as *mut u32, value);
        }
    }

    fn clr_enable_num(&self, value: u32) {
        let addr = self.base + APLIC_CLR_ENABLE_NUM_BASE;
        unsafe {
            write_volatile(addr as *mut u32, value);
        }
    }

    fn setipnum_le(&self, value: u32) {
        let addr = self.base + APLIC_SET_IPNUM_LE_BASE;
        unsafe {
            write_volatile(addr as *mut u32, value);
        }
    }

    fn set_target_msi(&self, irq: u32, hart: u32, guest: u32, eiid: u32) {
        let addr = self.base + APLIC_TARGET_BASE + (irq as usize - 1) * 4;
        let src = (hart << 18) | (guest << 12) | eiid;
        unsafe {
            write_volatile(addr as *mut u32, src);
        }
    }

    fn set_target_direct(&self, irq: u32, hart: u32, prio: u32) {
        let addr = self.base + APLIC_TARGET_BASE + (irq as usize - 1) * 4;
        let src = (hart << 18) | (prio & 0xFF);
        unsafe {
            write_volatile(addr as *mut u32, src);
        }
    }
}
