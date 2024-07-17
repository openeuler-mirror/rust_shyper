// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use spin::Mutex;

use crate::config::VmEmulatedDeviceConfig;
use crate::device::{
    EmuContext, virtio_blk_notify_handler, virtio_console_notify_handler, virtio_mediated_blk_notify_handler,
    virtio_net_handle_ctrl, virtio_net_notify_handler, EmuDev, EmuDeviceType,
};
use crate::device::VirtioDeviceType;
use crate::device::Virtq;
use crate::device::{VIRTQUEUE_BLK_MAX_SIZE, VIRTQUEUE_CONSOLE_MAX_SIZE, VIRTQUEUE_NET_MAX_SIZE};
use crate::device::VirtDev;
use crate::device::VIRTQ_READY;
use crate::kernel::{current_cpu, ipi_send_msg, IpiInnerMsg, IpiIntInjectMsg, IpiType, vm_ipa2pa};
use crate::kernel::{active_vm, active_vm_id};
use crate::kernel::Vm;

pub const VIRTIO_F_VERSION_1: usize = 1 << 32;
pub const VIRTIO_MMIO_MAGIC_VALUE: usize = 0x000;
pub const VIRTIO_MMIO_VERSION: usize = 0x004;
pub const VIRTIO_MMIO_DEVICE_ID: usize = 0x008;
pub const VIRTIO_MMIO_VENDOR_ID: usize = 0x00c;
pub const VIRTIO_MMIO_HOST_FEATURES: usize = 0x010;
pub const VIRTIO_MMIO_HOST_FEATURES_SEL: usize = 0x014;
pub const VIRTIO_MMIO_GUEST_FEATURES: usize = 0x020;
pub const VIRTIO_MMIO_GUEST_FEATURES_SEL: usize = 0x024;
pub const VIRTIO_MMIO_QUEUE_SEL: usize = 0x030;
pub const VIRTIO_MMIO_QUEUE_NUM_MAX: usize = 0x034;
pub const VIRTIO_MMIO_QUEUE_NUM: usize = 0x038;
pub const VIRTIO_MMIO_QUEUE_READY: usize = 0x044;
pub const VIRTIO_MMIO_QUEUE_NOTIFY: usize = 0x050;
pub const VIRTIO_MMIO_INTERRUPT_STATUS: usize = 0x060;
pub const VIRTIO_MMIO_INTERRUPT_ACK: usize = 0x064;
pub const VIRTIO_MMIO_STATUS: usize = 0x070;
pub const VIRTIO_MMIO_QUEUE_DESC_LOW: usize = 0x080;
pub const VIRTIO_MMIO_QUEUE_DESC_HIGH: usize = 0x084;
pub const VIRTIO_MMIO_QUEUE_AVAIL_LOW: usize = 0x090;
pub const VIRTIO_MMIO_QUEUE_AVAIL_HIGH: usize = 0x094;
pub const VIRTIO_MMIO_QUEUE_USED_LOW: usize = 0x0a0;
pub const VIRTIO_MMIO_QUEUE_USED_HIGH: usize = 0x0a4;
pub const VIRTIO_MMIO_CONFIG_GENERATION: usize = 0x0fc;
pub const VIRTIO_MMIO_CONFIG: usize = 0x100;
pub const VIRTIO_MMIO_REGS_END: usize = 0x200;

pub const VIRTIO_MMIO_INT_VRING: u32 = 1 << 0;
pub const VIRTIO_MMIO_INT_CONFIG: u32 = 1 << 1;

/// Represents the registers of a Virtio MMIO device.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct VirtMmioRegs {
    magic: u32,
    version: u32,
    device_id: u32,
    vendor_id: u32,
    dev_feature: u32,
    dev_feature_sel: u32,
    drv_feature: u32,
    drv_feature_sel: u32,
    q_sel: u32,
    q_num_max: u32,
    irt_stat: u32,
    irt_ack: u32,
    dev_stat: u32,
}

impl VirtMmioRegs {
    /// Creates a default instance of VirtMmioRegs.
    pub fn default() -> VirtMmioRegs {
        VirtMmioRegs {
            magic: 0,
            version: 0,
            device_id: 0,
            vendor_id: 0,
            dev_feature: 0,
            dev_feature_sel: 0,
            drv_feature: 0,
            drv_feature_sel: 0,
            q_sel: 0,
            q_num_max: 0,
            irt_stat: 0,
            irt_ack: 0,
            dev_stat: 0,
        }
    }

    /// Initializes the VirtMmioRegs with specified VirtioDeviceType.
    pub fn init(&mut self, id: VirtioDeviceType) {
        self.magic = 0x74726976;
        self.version = 0x2;
        self.vendor_id = 0x8888;
        self.device_id = id as u32;
        self.dev_feature = 0;
        self.drv_feature = 0;
        self.q_sel = 0;
    }
}

/// Represents a Virtio MMIO device const value part which will be unmuttable after initialaztion.
struct VirtioInnerConst {
    base: usize,
    length: usize,
    emu_type: EmuDeviceType,
    vq: Vec<Arc<Virtq>>,
    dev: VirtDev,
    vm: Weak<Vm>,
}

/// Represents a Virtio MMIO device.
pub struct VirtioMmio {
    inner_const: VirtioInnerConst,
    inner: Mutex<VirtioMmioInnerMut>,
}

impl VirtioMmio {
    /// Creates a new instance of VirtioMmio with the specified ID.
    pub fn new(vm: Weak<Vm>, dev_type: VirtioDeviceType, config: &VmEmulatedDeviceConfig) -> Self {
        Self {
            inner_const: VirtioInnerConst {
                base: config.base_ipa,
                length: config.length,
                emu_type: config.emu_type,
                vq: vec![],
                dev: VirtDev::new(dev_type, config),
                vm,
            },
            inner: Mutex::new(VirtioMmioInnerMut::new()),
        }
    }

    // Initializes the Virtio queue based on the specified VirtioDeviceType.
    fn init(&self, dev_type: VirtioDeviceType) {
        let mut inner = self.inner.lock();
        inner.regs.init(dev_type);
    }

    /// Sets the maximum queue number (Q_NUM_MAX) for the Virtio MMIO device.
    pub fn set_q_num_max(&self, q_num_max: u32) {
        let mut inner = self.inner.lock();
        inner.regs.q_num_max = q_num_max;
    }

    fn virtio_queue_init(&mut self, weak: &Weak<VirtioMmio>, dev_type: VirtioDeviceType) {
        match dev_type {
            VirtioDeviceType::Block => {
                self.set_q_num_max(VIRTQUEUE_BLK_MAX_SIZE as u32);
                let queue = if self.inner_const.dev.mediated() {
                    Virtq::new(0, weak.clone(), virtio_mediated_blk_notify_handler)
                } else {
                    Virtq::new(0, weak.clone(), virtio_blk_notify_handler)
                };
                self.inner_const.vq.push(queue);
            }
            VirtioDeviceType::Net => {
                self.set_q_num_max(VIRTQUEUE_NET_MAX_SIZE as u32);
                // Create two queues for the VirtioNet data device.
                for i in 0..2 {
                    let queue = Virtq::new(i, weak.clone(), virtio_net_notify_handler);
                    self.inner_const.vq.push(queue);
                }
                // Create a queue for the VirtioNet control device.
                let queue = Virtq::new(2, weak.clone(), virtio_net_handle_ctrl);
                self.inner_const.vq.push(queue);
            }
            VirtioDeviceType::Console => {
                self.set_q_num_max(VIRTQUEUE_CONSOLE_MAX_SIZE as u32);
                for i in 0..4 {
                    let queue = Virtq::new(i, weak.clone(), virtio_console_notify_handler);
                    self.inner_const.vq.push(queue);
                }
            }
            _ => {
                panic!("virtio_queue_init: unknown dev_type");
            }
        }
    }

    pub fn upper_vm(&self) -> Option<Arc<Vm>> {
        self.inner_const.vm.upgrade()
    }

    /// Notifies the specified VM about the configuration changes.
    pub fn notify_config(&self) {
        let mut inner = self.inner.lock();
        inner.regs.irt_stat |= VIRTIO_MMIO_INT_CONFIG;
        drop(inner);
        let vm = self.upper_vm().unwrap();
        let int_id = self.inner_const.dev.int_id();
        let target_vcpu = vm.vcpu(0).unwrap();
        use crate::kernel::interrupt_vm_inject;
        if target_vcpu.phys_id() == current_cpu().id {
            interrupt_vm_inject(&vm, target_vcpu, int_id);
        } else {
            let m = IpiIntInjectMsg { vm_id: vm.id(), int_id };
            if !ipi_send_msg(
                target_vcpu.phys_id(),
                IpiType::IpiTIntInject,
                IpiInnerMsg::IntInjectMsg(m),
            ) {
                error!("notify_config: failed to send ipi to Core {}", target_vcpu.phys_id());
            }
        }
    }

    /// Notifies the specified VM.
    pub fn notify(&self) {
        let mut inner = self.inner.lock();
        inner.regs.irt_stat |= VIRTIO_MMIO_INT_VRING;
        drop(inner);
        let vm = self.upper_vm().unwrap();
        let int_id = self.inner_const.dev.int_id();
        let target_vcpu = vm.vcpu(0).unwrap();
        use crate::kernel::interrupt_vm_inject;
        if target_vcpu.phys_id() == current_cpu().id {
            interrupt_vm_inject(&vm, target_vcpu, int_id);
        } else {
            let m = IpiIntInjectMsg { vm_id: vm.id(), int_id };
            if !ipi_send_msg(
                target_vcpu.phys_id(),
                IpiType::IpiTIntInject,
                IpiInnerMsg::IntInjectMsg(m),
            ) {
                error!("notify_config: failed to send ipi to Core {}", target_vcpu.phys_id());
            }
        }
    }

    /// virtio_dev_reset
    pub fn dev_reset(&self) {
        let mut inner = self.inner.lock();
        inner.regs.dev_stat = 0;
        inner.regs.irt_stat = 0;
        let idx = inner.regs.q_sel as usize;
        let vq = &self.inner_const.vq;
        vq[idx].set_ready(0);
        for virtq in vq.iter() {
            virtq.reset();
        }
        self.dev().set_activated(false);
    }

    /// Sets the Interrupt Status (IRT_STAT) for the Virtio MMIO device.
    pub fn set_irt_stat(&self, irt_stat: u32) {
        let mut inner = self.inner.lock();
        inner.regs.irt_stat = irt_stat;
    }

    /// Sets the Interrupt Acknowledge (IRT_ACK) for the Virtio MMIO device.
    pub fn set_irt_ack(&self, irt_ack: u32) {
        let mut inner = self.inner.lock();
        inner.regs.irt_ack = irt_ack;
    }

    /// Sets the selected queue index (Q_SEL) for the Virtio MMIO device.
    pub fn set_q_sel(&self, q_sel: u32) {
        let mut inner = self.inner.lock();
        inner.regs.q_sel = q_sel;
    }

    /// Sets the Device Status (DEV_STAT) for the Virtio MMIO device.
    pub fn set_dev_stat(&self, dev_stat: u32) {
        let mut inner = self.inner.lock();
        inner.regs.dev_stat = dev_stat;
    }

    /// Sets the Device Features (DEV_FEATURE) for the Virtio MMIO device.
    pub fn set_dev_feature(&self, dev_feature: u32) {
        let mut inner = self.inner.lock();
        inner.regs.dev_feature = dev_feature;
    }

    /// Sets the Device Features Selector (DEV_FEATURE_SEL) for the Virtio MMIO device.
    pub fn set_dev_feature_sel(&self, dev_feature_sel: u32) {
        let mut inner = self.inner.lock();
        inner.regs.dev_feature_sel = dev_feature_sel;
    }

    /// Sets the Driver Features (DRV_FEATURE) for the Virtio MMIO device.
    pub fn set_drv_feature(&self, drv_feature: u32) {
        let mut inner = self.inner.lock();
        inner.regs.drv_feature = drv_feature;
    }

    /// Sets the Driver Features Selector (DRV_FEATURE_SEL) for the Virtio MMIO device.
    pub fn set_drv_feature_sel(&self, drv_feature_sel: u32) {
        let mut inner = self.inner.lock();
        inner.regs.drv_feature = drv_feature_sel;
    }

    /// Performs a bitwise OR operation on the Driver Features (DRV_FEATURE) with the specified value.
    pub fn or_driver_feature(&self, driver_features: usize) {
        let mut inner = self.inner.lock();
        inner.driver_features |= driver_features;
    }

    /// Retrieves a reference of the VirtioDev associated with the Virtio MMIO device.
    pub fn dev(&self) -> &VirtDev {
        &self.inner_const.dev
    }

    /// Retrieves the selected queue index (Q_SEL) for the Virtio MMIO device.
    pub fn q_sel(&self) -> u32 {
        let inner = self.inner.lock();
        inner.regs.q_sel
    }

    /// Retrieves the magic value from the Virtio MMIO registers.
    pub fn magic(&self) -> u32 {
        let inner = self.inner.lock();
        inner.regs.magic
    }

    /// Retrieves the version value from the Virtio MMIO registers.
    pub fn version(&self) -> u32 {
        let inner = self.inner.lock();
        inner.regs.version
    }

    /// Retrieves the device ID from the Virtio MMIO registers.
    pub fn device_id(&self) -> u32 {
        let inner = self.inner.lock();
        inner.regs.device_id
    }

    /// Retrieves the vendor ID from the Virtio MMIO registers.
    pub fn vendor_id(&self) -> u32 {
        let inner = self.inner.lock();
        inner.regs.vendor_id
    }

    /// Retrieves the device status (DEV_STAT) from the Virtio MMIO registers.
    pub fn dev_stat(&self) -> u32 {
        let inner = self.inner.lock();
        inner.regs.dev_stat
    }

    /// Retrieves the Device Features Selector (DEV_FEATURE_SEL) from the Virtio MMIO registers.
    pub fn dev_feature_sel(&self) -> u32 {
        let inner = self.inner.lock();
        inner.regs.dev_feature_sel
    }

    /// Retrieves the Driver Features Selector (DRV_FEATURE_SEL) from the Virtio MMIO registers.
    pub fn drv_feature_sel(&self) -> u32 {
        let inner = self.inner.lock();
        inner.regs.drv_feature_sel
    }

    /// Retrieves the maximum queue number (Q_NUM_MAX) from the Virtio MMIO registers.
    pub fn q_num_max(&self) -> u32 {
        let inner = self.inner.lock();
        inner.regs.q_num_max
    }

    pub fn irt_stat(&self) -> u32 {
        let inner = self.inner.lock();
        inner.regs.irt_stat
    }

    /// Retrieves a clone of the Virtq associated with the specified index, wrapped in a Result.
    /// Returns an Err variant if the index is out of bounds.
    pub fn vq(&self, idx: usize) -> Result<&Virtq, ()> {
        match self.inner_const.vq.get(idx) {
            Some(vq) => Ok(vq),
            None => Err(()),
        }
    }

    #[inline]
    /// Retrieves the Base of the Virtio MMIO device.
    pub fn base(&self) -> usize {
        self.inner_const.base
    }
}

/// Represents the inner mutable data structure of VirtioMmio.
struct VirtioMmioInnerMut {
    driver_features: usize,
    driver_status: usize,
    regs: VirtMmioRegs,
}

impl VirtioMmioInnerMut {
    fn new() -> VirtioMmioInnerMut {
        VirtioMmioInnerMut {
            driver_features: 0,
            driver_status: 0,
            regs: VirtMmioRegs::default(),
        }
    }
}

/// Handles prologue access to Virtio MMIO registers.
fn virtio_mmio_prologue_access(mmio: &VirtioMmio, emu_ctx: &EmuContext, offset: usize, write: bool) {
    if !write {
        let value = match offset {
            VIRTIO_MMIO_MAGIC_VALUE => mmio.magic(),
            VIRTIO_MMIO_VERSION => mmio.version(),
            VIRTIO_MMIO_DEVICE_ID => mmio.device_id(),
            VIRTIO_MMIO_VENDOR_ID => mmio.vendor_id(),
            VIRTIO_MMIO_HOST_FEATURES => {
                let value = if mmio.dev_feature_sel() != 0 {
                    (mmio.dev().features() >> 32) as u32
                } else {
                    mmio.dev().features() as u32
                };
                mmio.set_dev_feature(value);
                value
            }
            VIRTIO_MMIO_STATUS => mmio.dev_stat(),
            _ => {
                error!("virtio_be_init_handler wrong reg_read, address=0x{:x}", emu_ctx.address);
                return;
            }
        };
        let idx = emu_ctx.reg;
        let val = value as usize;
        current_cpu().set_gpr(idx, val);
    } else {
        let idx = emu_ctx.reg;
        let value = current_cpu().get_gpr(idx) as u32;
        match offset {
            VIRTIO_MMIO_HOST_FEATURES_SEL => {
                mmio.set_dev_feature_sel(value);
            }
            VIRTIO_MMIO_GUEST_FEATURES => {
                mmio.set_drv_feature(value);
                if mmio.drv_feature_sel() != 0 {
                    mmio.or_driver_feature((value as usize) << 32);
                } else {
                    mmio.or_driver_feature(value as usize);
                }
            }
            VIRTIO_MMIO_GUEST_FEATURES_SEL => {
                mmio.set_drv_feature_sel(value);
            }
            VIRTIO_MMIO_STATUS => {
                mmio.set_dev_stat(value);
                if mmio.dev_stat() == 0 {
                    mmio.dev_reset();
                    info!("VM {} virtio device {:#x} is reset", active_vm_id(), mmio.base());
                } else if mmio.dev_stat() == 0xf {
                    mmio.dev().set_activated(true);
                    info!("VM {} virtio device {:#x} init ok", active_vm_id(), mmio.base());
                }
            }
            _ => {
                error!("virtio_mmio_prologue_access: wrong reg write 0x{:x}", emu_ctx.address);
            }
        }
    }
}

/// Handles queue access to Virtio MMIO registers.
fn virtio_mmio_queue_access(mmio: &VirtioMmio, emu_ctx: &EmuContext, offset: usize, write: bool) {
    if !write {
        let value;
        match offset {
            VIRTIO_MMIO_QUEUE_NUM_MAX => value = mmio.q_num_max(),
            VIRTIO_MMIO_QUEUE_READY => {
                let idx = mmio.q_sel() as usize;
                match mmio.vq(idx) {
                    Ok(virtq) => {
                        value = virtq.ready() as u32;
                    }
                    Err(_) => {
                        panic!(
                            "virtio_mmio_queue_access: wrong q_sel {:x} in read VIRTIO_MMIO_QUEUE_READY",
                            idx
                        );
                        // return;
                    }
                }
            }
            _ => {
                error!(
                    "virtio_mmio_queue_access: wrong reg_read, address {:x}",
                    emu_ctx.address
                );
                return;
            }
        }
        let idx = emu_ctx.reg;
        let val = value as usize;
        current_cpu().set_gpr(idx, val);
    } else {
        let idx = emu_ctx.reg;
        let value = current_cpu().get_gpr(idx);
        let q_sel = mmio.q_sel() as usize;
        match offset {
            VIRTIO_MMIO_QUEUE_SEL => mmio.set_q_sel(value as u32),
            VIRTIO_MMIO_QUEUE_NUM => {
                match mmio.vq(q_sel) {
                    Ok(virtq) => {
                        virtq.set_num(value);
                    }
                    Err(_) => {
                        panic!(
                            "virtio_mmio_queue_access: wrong q_sel {:x} in write VIRTIO_MMIO_QUEUE_NUM",
                            q_sel
                        );
                        // return;
                    }
                }
            }
            VIRTIO_MMIO_QUEUE_READY => match mmio.vq(q_sel) {
                Ok(virtq) => {
                    virtq.set_ready(value);
                    if value == VIRTQ_READY {
                        info!(
                            "VM {} virtio device {:#x} queue {:#x} ready",
                            active_vm_id(),
                            mmio.base(),
                            q_sel
                        );
                    } else {
                        warn!(
                            "VM {} virtio device {:#x} queue {:#x} init failed",
                            active_vm_id(),
                            mmio.base(),
                            q_sel
                        );
                    }
                }
                Err(_) => {
                    panic!(
                        "virtio_mmio_queue_access: wrong q_sel {:x} in write VIRTIO_MMIO_QUEUE_READY",
                        q_sel
                    );
                }
            },
            VIRTIO_MMIO_QUEUE_DESC_LOW => match mmio.vq(q_sel) {
                Ok(virtq) => {
                    virtq.or_desc_table_addr(value & u32::MAX as usize);
                }
                Err(_) => {
                    panic!(
                        "virtio_mmio_queue_access: wrong q_sel {:x} in write VIRTIO_MMIO_QUEUE_DESC_LOW",
                        q_sel
                    );
                }
            },
            VIRTIO_MMIO_QUEUE_DESC_HIGH => match mmio.vq(q_sel) {
                Ok(virtq) => {
                    virtq.or_desc_table_addr(value << 32);
                    let desc_table_addr = vm_ipa2pa(&active_vm().unwrap(), virtq.desc_table_addr());
                    if desc_table_addr == 0 {
                        error!("virtio_mmio_queue_access: invalid desc_table_addr");
                        return;
                    }
                    // SAFETY:
                    // The 'desc_table_addr' is valid MMIO address of virtio-blk config
                    // And it is checked by vm_ipa2pa to gurantee that it is in the range of vm config memory
                    unsafe {
                        virtq.set_desc_table(desc_table_addr);
                    }
                }
                Err(_) => {
                    panic!(
                        "virtio_mmio_queue_access: wrong q_sel {:x} in write VIRTIO_MMIO_QUEUE_DESC_HIGH",
                        q_sel
                    );
                }
            },
            VIRTIO_MMIO_QUEUE_AVAIL_LOW => match mmio.vq(q_sel) {
                Ok(virtq) => {
                    virtq.or_avail_addr(value & u32::MAX as usize);
                }
                Err(_) => {
                    panic!(
                        "virtio_mmio_queue_access: wrong q_sel {:x} in write VIRTIO_MMIO_QUEUE_AVAIL_LOW",
                        q_sel
                    );
                }
            },
            VIRTIO_MMIO_QUEUE_AVAIL_HIGH => match mmio.vq(q_sel) {
                Ok(virtq) => {
                    virtq.or_avail_addr(value << 32);
                    let avail_addr = vm_ipa2pa(&active_vm().unwrap(), virtq.avail_addr());
                    if avail_addr == 0 {
                        error!("virtio_mmio_queue_access: invalid avail_addr");
                        return;
                    }
                    // SAFETY:
                    // The 'avail_addr' is valid MMIO address of virtio-blk config
                    // And it is checked by vm_ipa2pa to gurantee that it is in the range of vm config memory
                    unsafe {
                        virtq.set_avail(avail_addr);
                    }
                }
                Err(_) => {
                    panic!(
                        "virtio_mmio_queue_access: wrong q_sel {:x} in write VIRTIO_MMIO_QUEUE_AVAIL_HIGH",
                        q_sel
                    );
                }
            },
            VIRTIO_MMIO_QUEUE_USED_LOW => match mmio.vq(q_sel) {
                Ok(virtq) => {
                    virtq.or_used_addr(value & u32::MAX as usize);
                }
                Err(_) => {
                    panic!(
                        "virtio_mmio_queue_access: wrong q_sel {:x} in write VIRTIO_MMIO_QUEUE_USED_LOW",
                        q_sel
                    );
                }
            },
            VIRTIO_MMIO_QUEUE_USED_HIGH => match mmio.vq(q_sel) {
                Ok(virtq) => {
                    virtq.or_used_addr(value << 32);
                    let used_addr = vm_ipa2pa(&active_vm().unwrap(), virtq.used_addr());
                    if used_addr == 0 {
                        error!("virtio_mmio_queue_access: invalid used_addr");
                        return;
                    }
                    // SAFETY:
                    // The 'used_addr' is valid MMIO address of virtio-blk config
                    // And it is checked by vm_ipa2pa to gurantee that it is in the range of vm config memory
                    unsafe {
                        virtq.set_used(used_addr);
                    }
                }
                Err(_) => {
                    panic!(
                        "virtio_mmio_queue_access: wrong q_sel {:x} in write VIRTIO_MMIO_QUEUE_USED_HIGH",
                        q_sel
                    );
                }
            },
            _ => {
                error!("virtio_mmio_queue_access: wrong reg write 0x{:x}", emu_ctx.address);
            }
        }
    }
}

/// Handles config space access to Virtio MMIO registers.
fn virtio_mmio_cfg_access(mmio: &VirtioMmio, emu_ctx: &EmuContext, offset: usize, write: bool) {
    if !write {
        let width = emu_ctx.width;
        let value = match offset {
            VIRTIO_MMIO_CONFIG_GENERATION => mmio.dev().generation(),
            VIRTIO_MMIO_CONFIG..=0x1ff => match mmio.dev().desc() {
                super::DevDesc::BlkDesc(blk_desc) => {
                    // SAFETY: Offset is between VIRTIO_MMIO_CONFIG..VIRTIO_MMIO_REGS_END ,so is valid
                    unsafe { blk_desc.offset_data(offset - VIRTIO_MMIO_CONFIG, width) }
                }
                super::DevDesc::NetDesc(net_desc) => {
                    // SAFETY: Offset is between VIRTIO_MMIO_CONFIG..VIRTIO_MMIO_REGS_END ,so is valid
                    unsafe { net_desc.offset_data(offset - VIRTIO_MMIO_CONFIG, width) }
                }
                _ => {
                    panic!("unknow desc type");
                }
            },
            _ => {
                error!("virtio_mmio_cfg_access: wrong reg write 0x{:x}", emu_ctx.address);
                return;
            }
        };
        let idx = emu_ctx.reg;
        current_cpu().set_gpr(idx, value);
    } else {
        error!("virtio_mmio_cfg_access: wrong reg write 0x{:x}", emu_ctx.address);
    }
}

/// Initializes the Virtio MMIO device for a virtual machine.
pub fn emu_virtio_mmio_init(vm: Weak<Vm>, emu_cfg: &VmEmulatedDeviceConfig) -> Result<Arc<dyn EmuDev>, ()> {
    let virt_dev_type = match emu_cfg.emu_type {
        EmuDeviceType::EmuDeviceTVirtioBlk => VirtioDeviceType::Block,
        EmuDeviceType::EmuDeviceTVirtioNet => VirtioDeviceType::Net,
        EmuDeviceType::EmuDeviceTVirtioConsole => VirtioDeviceType::Console,
        _ => {
            error!("emu_virtio_mmio_init: unknown emulated device type");
            return Err(());
        }
    };

    let mmio = Arc::new_cyclic(|weak| {
        let mut mmio = VirtioMmio::new(vm, virt_dev_type, emu_cfg);
        mmio.init(virt_dev_type);
        mmio.virtio_queue_init(weak, virt_dev_type);
        mmio
    });

    if emu_cfg.emu_type == EmuDeviceType::EmuDeviceTVirtioNet {
        let nic = mmio.clone();
        let mac = emu_cfg.cfg_list.iter().take(6).map(|&x| x as u8).collect::<Vec<_>>();
        super::mac::set_mac_info(&mac, nic);
    }
    Ok(mmio)
}

impl EmuDev for VirtioMmio {
    fn emu_type(&self) -> EmuDeviceType {
        self.inner_const.emu_type
    }

    fn address_range(&self) -> core::ops::Range<usize> {
        self.inner_const.base..(self.inner_const.base + self.inner_const.length)
    }

    /// Handles Virtio MMIO events for the specified emulated device.
    fn handler(&self, emu_ctx: &EmuContext) -> bool {
        let addr = emu_ctx.address;
        let offset = addr - self.base();
        let write = emu_ctx.write;

        if offset == VIRTIO_MMIO_QUEUE_NOTIFY && write {
            self.set_irt_stat(VIRTIO_MMIO_INT_VRING);

            let idx = current_cpu().get_gpr(emu_ctx.reg);
            if !self.inner_const.vq[idx].call_notify_handler() {
                error!("Failed to handle virtio mmio request!");
            }
        } else if offset == VIRTIO_MMIO_INTERRUPT_STATUS && !write {
            let idx = emu_ctx.reg;
            let val = self.irt_stat() as usize;
            current_cpu().set_gpr(idx, val);
        } else if offset == VIRTIO_MMIO_INTERRUPT_ACK && write {
            let idx = emu_ctx.reg;
            let val = self.irt_stat();
            self.set_irt_stat(val & !(current_cpu().get_gpr(idx) as u32));
            self.set_irt_ack(current_cpu().get_gpr(idx) as u32);
        } else if (VIRTIO_MMIO_MAGIC_VALUE..=VIRTIO_MMIO_GUEST_FEATURES_SEL).contains(&offset)
            || offset == VIRTIO_MMIO_STATUS
        {
            virtio_mmio_prologue_access(self, emu_ctx, offset, write);
        } else if (VIRTIO_MMIO_QUEUE_SEL..=VIRTIO_MMIO_QUEUE_USED_HIGH).contains(&offset) {
            virtio_mmio_queue_access(self, emu_ctx, offset, write);
        } else if (VIRTIO_MMIO_CONFIG_GENERATION..=VIRTIO_MMIO_REGS_END).contains(&offset) {
            virtio_mmio_cfg_access(self, emu_ctx, offset, write);
        } else {
            error!(
                "emu_virtio_mmio_handler: regs wrong {}, address 0x{:x}, offset 0x{:x}",
                if write { "write" } else { "read" },
                addr,
                offset
            );
            return false;
        }
        true
    }
}
