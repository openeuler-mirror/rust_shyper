// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use alloc::vec::Vec;
use core::fmt::{Display, Formatter};
use core::ops::Range;
use core::ptr;

use spin::RwLock;

use crate::kernel::{active_vm, current_cpu};
use crate::utils::downcast::DowncastSync;

pub trait EmuDev: DowncastSync {
    /// emulated device type
    fn emu_type(&self) -> EmuDeviceType;
    /// emulated device address range
    fn address_range(&self) -> Range<usize>;
    /// emulated device handler
    fn handler(&self, emu_ctx: &EmuContext) -> bool;
}

#[derive(Debug, Clone, Copy)]
pub struct EmuContext {
    pub address: usize,
    pub width: usize,
    pub write: bool,
    pub sign_ext: bool,
    pub reg: usize,
    pub reg_width: usize,
}

impl EmuContext {
    /// Reads from the specified emulator context.
    /// # Safety:
    /// The address must be readable.
    pub unsafe fn read(&self) -> usize {
        match self.width {
            1 => ptr::read_volatile(self.address as *const u8) as usize,
            2 => ptr::read_volatile(self.address as *const u16) as usize,
            4 => ptr::read_volatile(self.address as *const u32) as usize,
            8 => ptr::read_volatile(self.address as *const u64) as usize,
            _ => panic!("unexpected read width {} at {:x}", self.width, self.address),
        }
    }

    /// Writes to the specified emulator context.
    /// # Safety:
    /// The address must be writable.
    pub unsafe fn write(&self, val: usize) {
        match self.width {
            1 => ptr::write_volatile(self.address as *mut u8, val as u8),
            2 => ptr::write_volatile(self.address as *mut u16, val as u16),
            4 => ptr::write_volatile(self.address as *mut u32, val as u32),
            8 => ptr::write_volatile(self.address as *mut u64, val as u64),
            _ => panic!("unexpected write width {} at {:x}", self.width, self.address),
        }
    }
}

/// Struct representing an emulator device entry.
pub struct EmuDevEntry {
    /// The type of the emulator device.
    pub emu_type: EmuDeviceType,
    /// The virtual machine ID associated with the emulator device.
    pub vm_id: usize,
    /// The ID of the emulator device.
    pub id: usize,
    /// The inmediate physical address associated with the emulator device.
    pub ipa: usize,
    /// The size of the emulator device.
    pub size: usize,
    /// The handler function for the emulator device.
    pub handler: EmuDevHandler,
}

/// Enumeration representing the type of emulator devices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EmuDeviceType {
    // Variants representing different emulator device types.
    EmuDeviceTConsole = 0,
    EmuDeviceTGicd = 1,
    EmuDeviceTGPPT = 2,
    EmuDeviceTVirtioBlk = 3,
    EmuDeviceTVirtioNet = 4,
    EmuDeviceTVirtioConsole = 5,
    EmuDeviceTShyper = 6,
    EmuDeviceTVirtioBlkMediated = 7,
    EmuDeviceTIOMMU = 8,
    EmuDeviceTGICR = 11,
    EmuDeviceTMeta = 12,
    EmuDeviceTPlic = 13,
}

impl Display for EmuDeviceType {
    // Implementation of the Display trait for EmuDeviceType.
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            EmuDeviceType::EmuDeviceTConsole => write!(f, "console"),
            EmuDeviceType::EmuDeviceTGicd => write!(f, "interrupt controller"),
            EmuDeviceType::EmuDeviceTGPPT => write!(f, "partial passthrough interrupt controller"),
            EmuDeviceType::EmuDeviceTVirtioBlk => write!(f, "virtio block"),
            EmuDeviceType::EmuDeviceTVirtioNet => write!(f, "virtio net"),
            EmuDeviceType::EmuDeviceTVirtioConsole => write!(f, "virtio console"),
            EmuDeviceType::EmuDeviceTShyper => write!(f, "device shyper"),
            EmuDeviceType::EmuDeviceTVirtioBlkMediated => write!(f, "medaited virtio block"),
            EmuDeviceType::EmuDeviceTIOMMU => write!(f, "IOMMU"),
            EmuDeviceType::EmuDeviceTGICR => write!(f, "interrupt controller gicr"),
            EmuDeviceType::EmuDeviceTMeta => write!(f, "meta device"),
            EmuDeviceType::EmuDeviceTPlic => write!(f, "platform level interrupt controller"),
        }
    }
}

/// Implementation of methods for EmuDeviceType.
impl EmuDeviceType {
    // Implementation of methods for EmuDeviceType.
    pub fn removable(&self) -> bool {
        matches!(
            *self,
            EmuDeviceType::EmuDeviceTGicd
                | EmuDeviceType::EmuDeviceTGPPT
                | EmuDeviceType::EmuDeviceTVirtioBlk
                | EmuDeviceType::EmuDeviceTVirtioNet
                | EmuDeviceType::EmuDeviceTGICR
                | EmuDeviceType::EmuDeviceTVirtioConsole
        )
    }
}

impl EmuDeviceType {
    pub fn from_usize(value: usize) -> EmuDeviceType {
        match value {
            0 => EmuDeviceType::EmuDeviceTConsole,
            1 => EmuDeviceType::EmuDeviceTGicd,
            2 => EmuDeviceType::EmuDeviceTGPPT,
            3 => EmuDeviceType::EmuDeviceTVirtioBlk,
            4 => EmuDeviceType::EmuDeviceTVirtioNet,
            5 => EmuDeviceType::EmuDeviceTVirtioConsole,
            6 => EmuDeviceType::EmuDeviceTShyper,
            7 => EmuDeviceType::EmuDeviceTVirtioBlkMediated,
            8 => EmuDeviceType::EmuDeviceTIOMMU,
            11 => EmuDeviceType::EmuDeviceTGICR,
            12 => EmuDeviceType::EmuDeviceTMeta,
            13 => EmuDeviceType::EmuDeviceTPlic,
            _ => panic!("Unknown  EmuDeviceType value: {}", value),
        }
    }
}

pub type EmuDevHandler = fn(usize, &EmuContext) -> bool;

/// Function to handle emulator operations based on the emulator context.
// TO CHECK
pub fn emu_handler(emu_ctx: &EmuContext) -> bool {
    let ipa = emu_ctx.address;

    if let Some(emu_dev) = active_vm().unwrap().find_emu_dev(ipa) {
        return emu_dev.handler(emu_ctx);
    }

    error!(
        "emu_handler: no emul handler for Core {} data abort ipa 0x{:x}\nctx: {:?}",
        current_cpu().id,
        ipa,
        emu_ctx,
    );
    false
}

/// Static RwLock containing a vector of EmuRegEntry instances, representing emulator registers.
static EMU_REGS_LIST: RwLock<Vec<EmuRegEntry>> = RwLock::new(Vec::new());

/// Handles emulator register operations based on the provided EmuContext.
pub fn emu_reg_handler(emu_ctx: &EmuContext) -> bool {
    let address = emu_ctx.address;
    let active_vcpu = current_cpu().active_vcpu.as_ref().unwrap();
    let vm_id = active_vcpu.vm_id();

    let emu_regs_list = EMU_REGS_LIST.read();
    for emu_reg in emu_regs_list.iter() {
        if emu_reg.addr == address {
            let handler = emu_reg.handler;
            drop(emu_regs_list);
            return handler(vm_id, emu_ctx);
        }
    }
    error!(
        "emu_reg_handler: no handler for Core{} {} reg ({:#x})",
        current_cpu().id,
        if emu_ctx.write { "write" } else { "read" },
        address
    );
    false
}

/// Registers a new emulator register with the specified type, address, and handler function.
pub fn emu_register_reg(emu_type: EmuRegType, address: usize, handler: EmuRegHandler) {
    let mut emu_regs_list = EMU_REGS_LIST.write();

    for emu_reg in emu_regs_list.iter() {
        if address == emu_reg.addr {
            warn!(
                "emu_register_reg: duplicated emul reg addr: prev address {:#x}",
                address
            );
            return;
        }
    }

    emu_regs_list.push(EmuRegEntry {
        emu_type,
        addr: address,
        handler,
    });
}

/// Type alias for the handler function of emulator registers.
type EmuRegHandler = EmuDevHandler;

/// Struct representing an entry in the emulator register list.
pub struct EmuRegEntry {
    /// The type of the emulator register.
    pub emu_type: EmuRegType,
    /// The address associated with the emulator register.
    pub addr: usize,
    /// The handler function for the emulator register.
    pub handler: EmuRegHandler,
}

/// Enumeration representing the type of emulator registers.
pub enum EmuRegType {
    /// System register type for emulator registers.
    SysReg,
}
