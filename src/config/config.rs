// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

/// This module contains configurations for the virtual machine.
/// It defines structures and functions to manage VM configurations.
/// Each VM has its own configuration, and this module provides
/// functions to manipulate VM configurations, such as adding memory regions,
/// setting CPU configurations, and adding emulated or passthrough devices.
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::ffi::CStr;

use spin::Mutex;

// use crate::board::*;
use crate::device::{EmuDeviceType, mediated_blk_free, mediated_blk_request};
use crate::kernel::{active_vm, vm, Vm, vm_ipa2pa, VM_NUM_MAX, VmType};
use crate::utils::{BitAlloc, BitAlloc16, memcpy};
use crate::vmm::vmm_init_gvm;

/// The maximum length of a VM name.
pub const NAME_MAX_LEN: usize = 32;
const CFG_MAX_NUM: usize = 0x10;
const IRQ_MAX_NUM: usize = 0x40;
const PASSTHROUGH_DEV_MAX_NUM: usize = 128;
const EMULATED_DEV_MAX_NUM: usize = 16;

/// Represents the type of a device in the device tree.
#[derive(Clone, Copy, PartialEq)]
pub enum DtbDevType {
    DevSerial = 0,
    DevGicd = 1,
    DevGicc = 2,
    DevGicr = 3,
    DevPlic = 4,
}

/// Convert a `usize` value to a `DtbDevType`.
impl From<usize> for DtbDevType {
    fn from(value: usize) -> Self {
        match value {
            0 => DtbDevType::DevSerial,
            1 => DtbDevType::DevGicd,
            2 => DtbDevType::DevGicc,
            3 => DtbDevType::DevGicr,
            4 => DtbDevType::DevPlic,
            _ => panic!("Unknown DtbDevType value: {}", value),
        }
    }
}

//！ Represents the configuration of an emulated device for a virtual machine.
#[derive(Clone, Debug)]
pub struct VmEmulatedDeviceConfig {
    /// The name of the emulated device.
    pub name: String,
    /// The base IPA (Intermediate Physical Address) of the device.
    pub base_ipa: usize,
    /// The length of the device.
    pub length: usize,
    /// The IRQ (Interrupt Request) ID of the device.
    pub irq_id: usize,
    /// List of configuration values for the device.
    pub cfg_list: Vec<usize>,
    /// The type of emulated device.
    pub emu_type: EmuDeviceType,
    /// Indicates whether the device is mediated.
    pub mediated: bool,
}

/// Represents a list of emulated device configurations for a virtual machine.
#[derive(Clone, Default)]
pub struct VmEmulatedDeviceConfigList {
    /// List of emulated device configurations.
    pub emu_dev_list: Vec<VmEmulatedDeviceConfig>,
}

/// Represents the configuration of a passthrough region.
#[derive(Default, Clone, Copy, Debug, Eq)]
pub struct PassthroughRegion {
    /// The IPA (Intermediate Physical Address) of the passthrough region.
    pub ipa: usize,
    /// The PA (Physical Address) of the passthrough region.
    pub pa: usize,
    /// The length of the passthrough region.
    pub length: usize,
    /// Indicates whether the region has device properties.
    pub dev_property: bool,
}

impl PartialEq for PassthroughRegion {
    fn eq(&self, other: &Self) -> bool {
        self.ipa == other.ipa && self.pa == other.pa && self.length == other.length
    }
}

/// Represents the configuration of a passthrough device for a virtual machine.
#[derive(Default, Clone)]
pub struct VmPassthroughDeviceConfig {
    /// List of passthrough regions.
    pub regions: Vec<PassthroughRegion>,
    /// List of IRQs (Interrupt Requests) for the passthrough device.
    pub irqs: Vec<usize>,
    /// List of stream IDs for the passthrough device.
    pub streams_ids: Vec<usize>,
}

/// Represents a memory region configuration for a virtual machine.
#[derive(Clone, Debug)]
pub struct VmRegion {
    /// The starting IPA (Intermediate Physical Address) of the memory region.
    pub ipa_start: usize,
    /// The length of the memory region.
    pub length: usize,
}

/// Clone implementation for VmMemoryConfig struct.
#[derive(Default, Clone)]
pub struct VmMemoryConfig {
    pub region: Vec<VmRegion>,
}

/// Clone, Copy, and Default implementations for VmImageConfig struct.
#[derive(Clone, Copy, Default)]
pub struct VmImageConfig {
    pub kernel_img_name: Option<&'static str>,
    pub kernel_load_ipa: usize,
    pub kernel_entry_point: usize,
    pub device_tree_load_ipa: usize,
    pub ramdisk_load_ipa: usize,
    pub mediated_block_index: Option<usize>,
}

impl VmImageConfig {
    /// Constructor for VmImageConfig with essential parameters.
    pub fn new(kernel_load_ipa: usize, device_tree_load_ipa: usize, ramdisk_load_ipa: usize) -> VmImageConfig {
        VmImageConfig {
            kernel_img_name: None,
            kernel_load_ipa,
            kernel_entry_point: kernel_load_ipa,
            device_tree_load_ipa,
            ramdisk_load_ipa,
            mediated_block_index: None,
        }
    }
}

/// Configuration for VmCpu (Virtual Machine CPU).
#[derive(Clone, Default)]
pub struct VmCpuConfig {
    pub num: usize,
    pub allocate_bitmap: u32,
    pub master: Option<usize>,
}

impl VmCpuConfig {
    /// Constructor for VmCpuConfig with specified parameters.
    fn new(num: usize, allocate_bitmap: usize, master: usize) -> Self {
        /// Adjust num and allocate_bitmap based on the given values.
        /// Ensure allocate_bitmap and num match, accepting the lower bitmap by the given CPU num.
        /// This is a complex process of bit manipulation to synchronize allocate_bitmap and num.
        /// The resulting values are stored in a new VmCpuConfig instance.
        let num = usize::min(num, allocate_bitmap.count_ones() as usize);
        // make sure `allocate_bitmap` and `num` matches
        let allocate_bitmap = {
            // only accept the lower bitmap by given cpu num
            let mut index = 1 << allocate_bitmap.trailing_zeros();
            let mut remain = num;
            while remain > 0 && index <= allocate_bitmap {
                if allocate_bitmap & index != 0 {
                    remain -= 1;
                }
                index <<= 1;
            }
            allocate_bitmap & (index - 1)
        } as u32;
        // make sure `master` is in `allocate_bitmap`, So the master can't be -1
        let master = if allocate_bitmap & (1 << master) != 0 {
            Some(master)
        } else {
            None
        };
        Self {
            num,
            allocate_bitmap,
            master,
        }
    }
}

/// Configuration for VmDtbDev (Device Tree Device in Virtual Machine).
#[derive(Clone)]
pub struct VmDtbDevConfig {
    pub name: String,
    pub dev_type: DtbDevType,
    pub irqs: Vec<usize>,
    pub addr_region: VmRegion,
}

/// Configuration for VMDtbDevConfigList (List of Device Tree Devices in Virtual Machine).
#[derive(Clone, Default)]
pub struct VMDtbDevConfigList {
    pub dtb_device_list: Vec<VmDtbDevConfig>,
}

/// Configuration for VmConfigEntry (Virtual Machine Configuration Entry).
#[derive(Clone, Default)]
pub struct VmConfigEntry {
    // VM id, generate inside hypervisor.
    pub id: usize,
    // Following configs are not intended to be modified during configuration.
    pub name: String,
    pub os_type: VmType,
    pub cmdline: String,
    pub image: VmImageConfig,
    // Following config can be modified during configuration.
    pub memory: VmMemoryConfig,
    pub cpu: VmCpuConfig,
    pub vm_emu_dev_confg: VmEmulatedDeviceConfigList,
    pub vm_pt_dev_confg: VmPassthroughDeviceConfig,
    pub vm_dtb_devs: VMDtbDevConfigList,
    pub fdt_overlay: Vec<u8>,
}

/// Additional methods for VmConfigEntry.
impl VmConfigEntry {
    /// Creates a new VmConfigEntry with the specified parameters.
    pub fn new(
        name: String,
        cmdline: String,
        vm_type: usize,
        kernel_load_ipa: usize,
        device_tree_load_ipa: usize,
        ramdisk_load_ipa: usize,
    ) -> VmConfigEntry {
        VmConfigEntry {
            name,
            os_type: VmType::from(vm_type),
            cmdline,
            image: VmImageConfig::new(kernel_load_ipa, device_tree_load_ipa, ramdisk_load_ipa),
            ..Default::default()
        }
    }

    /// Returns the ID of the VmConfigEntry.
    pub fn id(&self) -> usize {
        self.id
    }

    /// Sets the ID of the VmConfigEntry.
    pub fn set_id(&mut self, id: usize) {
        self.id = id;
    }

    /// Returns the name of the virtual machine.
    pub fn vm_name(&self) -> String {
        self.name.clone()
    }

    /// Returns the index of the mediated block, if any.
    pub fn mediated_block_index(&self) -> Option<usize> {
        self.image.mediated_block_index
    }

    /// Sets the mediated block index.
    pub fn set_mediated_block_index(&mut self, med_blk_id: usize) {
        self.image.mediated_block_index = Some(med_blk_id);
    }

    /// Returns the name of the kernel image, if any.
    pub fn kernel_img_name(&self) -> Option<&'static str> {
        self.image.kernel_img_name
    }

    /// Returns the IPA (Physical Address) of the kernel load address.
    pub fn kernel_load_ipa(&self) -> usize {
        self.image.kernel_load_ipa
    }

    /// Returns the entry point of the kernel.
    pub fn kernel_entry_point(&self) -> usize {
        self.image.kernel_entry_point
    }

    /// Returns the IPA (Physical Address) of the device tree load address.
    pub fn device_tree_load_ipa(&self) -> usize {
        self.image.device_tree_load_ipa
    }

    /// Returns the IPA (Physical Address) of the ramdisk load address.
    pub fn ramdisk_load_ipa(&self) -> usize {
        self.image.ramdisk_load_ipa
    }

    /// Returns the memory regions configured for the virtual machine.
    pub fn memory_region(&self) -> &[VmRegion] {
        &self.memory.region
    }

    /// Adds a memory configuration with the specified IPA start and length.
    pub fn add_memory_cfg(&mut self, ipa_start: usize, length: usize) {
        self.memory.region.push(VmRegion { ipa_start, length });
    }

    /// Returns the number of CPUs configured for the virtual machine.
    pub fn cpu_num(&self) -> usize {
        self.cpu.num
    }

    /// Returns the CPU allocate bitmap for the virtual machine.
    pub fn cpu_allocated_bitmap(&self) -> u32 {
        self.cpu.allocate_bitmap
    }

    /// Returns the master CPU ID for the virtual machine.
    pub fn cpu_master(&self) -> Option<usize> {
        self.cpu.master
    }

    /// Sets the CPU configuration with the specified number, allocate bitmap, and master CPU ID.
    pub fn set_cpu_cfg(&mut self, num: usize, allocate_bitmap: usize, master: usize) {
        self.cpu = VmCpuConfig::new(num, allocate_bitmap, master);
    }

    /// Returns the list of emulated devices configured for the virtual machine.
    pub fn emulated_device_list(&self) -> &[VmEmulatedDeviceConfig] {
        &self.vm_emu_dev_confg.emu_dev_list
    }

    /// Adds an emulated device configuration to the virtual machine.
    pub fn add_emulated_device_cfg(&mut self, cfg: VmEmulatedDeviceConfig) {
        self.vm_emu_dev_confg.emu_dev_list.push(cfg);
    }

    /// Returns the list of passthrough device regions configured for the virtual machine.
    pub fn passthrough_device_regions(&self) -> &[PassthroughRegion] {
        &self.vm_pt_dev_confg.regions
    }

    /// Returns the list of passthrough device IRQs configured for the virtual machine.
    pub fn passthrough_device_irqs(&self) -> &[usize] {
        &self.vm_pt_dev_confg.irqs
    }

    /// Returns the list of passthrough device stream IDs configured for the virtual machine.
    pub fn passthrough_device_stread_ids(&self) -> &[usize] {
        &self.vm_pt_dev_confg.streams_ids
    }

    /// Adds a passthrough device region with the specified IPA start, PA start, and length.
    pub fn add_passthrough_device_region(&mut self, base_ipa: usize, base_pa: usize, length: usize) {
        self.vm_pt_dev_confg.regions.push(PassthroughRegion {
            ipa: base_ipa,
            pa: base_pa,
            length,
            dev_property: true,
        })
    }

    /// Adds passthrough device IRQs to the virtual machine configuration.
    pub fn add_passthrough_device_irqs(&mut self, irqs: &mut Vec<usize>) {
        self.vm_pt_dev_confg.irqs.append(irqs);
    }

    /// Adds passthrough device stream IDs to the virtual machine configuration.
    pub fn add_passthrough_device_streams_ids(&mut self, streams_ids: &mut Vec<usize>) {
        self.vm_pt_dev_confg.streams_ids.append(streams_ids);
    }

    /// Returns the list of DTB (Device Tree Blob) devices configured for the virtual machine.
    pub fn dtb_device_list(&self) -> &[VmDtbDevConfig] {
        &self.vm_dtb_devs.dtb_device_list
    }

    /// Adds a DTB device configuration to the virtual machine.
    pub fn add_dtb_device(&mut self, cfg: VmDtbDevConfig) {
        self.vm_dtb_devs.dtb_device_list.push(cfg);
    }

    /// Returns the IPA of the GICC (Generic Interrupt Controller - CPU Interface) device.
    pub fn gicc_addr(&self) -> usize {
        for dev in &self.vm_dtb_devs.dtb_device_list {
            if let DtbDevType::DevGicc = dev.dev_type {
                return dev.addr_region.ipa_start;
            }
        }
        0
    }

    /// Returns the IPA of the GICD (Generic Interrupt Controller Distributor) device.
    pub fn gicd_addr(&self) -> usize {
        for dev in &self.vm_dtb_devs.dtb_device_list {
            if let DtbDevType::DevGicd = dev.dev_type {
                return dev.addr_region.ipa_start;
            }
        }
        0
    }

    /// Returns the IPA of the GICR (Generic Interrupt Controller Redistributor) device.
    pub fn gicr_addr(&self) -> usize {
        for dev in &self.vm_dtb_devs.dtb_device_list {
            if let DtbDevType::DevGicr = dev.dev_type {
                return dev.addr_region.ipa_start;
            }
        }
        0
    }
}

/// Represents the configuration table for virtual machines.
#[derive(Clone)]
pub struct VmConfigTable {
    pub name: Option<&'static str>,
    pub vm_bitmap: BitAlloc16,
    pub vm_num: usize,
    pub entries: Vec<VmConfigEntry>,
}

/// Additional methods for VmConfigTable.
impl VmConfigTable {
    /// Creates a new VmConfigTable.
    const fn new() -> VmConfigTable {
        VmConfigTable {
            name: None,
            vm_bitmap: BitAlloc16::default(),
            vm_num: 0,
            entries: Vec::new(),
        }
    }

    /// Generates a new VM ID and returns it.
    pub fn generate_vm_id(&mut self) -> Result<usize, ()> {
        for i in 0..VM_NUM_MAX {
            if self.vm_bitmap.get(i) == 0 {
                self.vm_bitmap.set(i);
                return Ok(i);
            }
        }
        Err(())
    }

    /// Removes a VM ID from the bitmap.
    pub fn remove_vm_id(&mut self, vm_id: usize) {
        if vm_id >= VM_NUM_MAX || self.vm_bitmap.get(vm_id) == 0 {
            error!("illegal vm id {}", vm_id);
        }
        self.vm_bitmap.clear(vm_id);
    }
}

/// Static instance of the default VM configuration table.
pub static DEF_VM_CONFIG_TABLE: Mutex<VmConfigTable> = Mutex::new(VmConfigTable::new());

/// Sets the configuration name for the default VM configuration table.
pub fn vm_cfg_set_config_name(name: &'static str) {
    let mut vm_config = DEF_VM_CONFIG_TABLE.lock();
    vm_config.name = Some(name);
}

/// Returns the number of configured virtual machines.
pub fn vm_num() -> usize {
    let vm_config = DEF_VM_CONFIG_TABLE.lock();
    vm_config.entries.len()
}

/// Returns the type of the virtual machine with the specified ID.
pub fn vm_type(vmid: usize) -> VmType {
    let vm_config = DEF_VM_CONFIG_TABLE.lock();
    for vm_cfg_entry in vm_config.entries.iter() {
        if vm_cfg_entry.id == vmid {
            return vm_cfg_entry.os_type;
        }
    }
    error!("failed to find VM[{}] in vm cfg entry list", vmid);
    VmType::VmTOs
}

/// Returns a list of IDs for all configured virtual machines.
pub fn vm_id_list() -> Vec<usize> {
    let vm_config = DEF_VM_CONFIG_TABLE.lock();
    let mut id_list: Vec<usize> = Vec::new();
    for vm_cfg_entry in vm_config.entries.iter() {
        id_list.push(vm_cfg_entry.id)
    }
    id_list
}

/// Returns the configuration entry for the virtual machine with the specified ID.
pub fn vm_cfg_entry(vmid: usize) -> Option<VmConfigEntry> {
    let vm_config = DEF_VM_CONFIG_TABLE.lock();
    for vm_cfg_entry in vm_config.entries.iter() {
        if vm_cfg_entry.id == vmid {
            return Some(vm_cfg_entry.clone());
        }
    }
    error!("failed to find VM[{}] in vm cfg entry list", vmid);
    None
}

fn vm_cfg_editor<F>(vmid: usize, editor: F) -> Result<usize, ()>
where
    F: FnOnce(&mut VmConfigEntry) -> Result<usize, ()>,
{
    let mut vm_config = DEF_VM_CONFIG_TABLE.lock();
    for vm_cfg_entry in vm_config.entries.iter_mut() {
        if vm_cfg_entry.id == vmid {
            return editor(vm_cfg_entry);
        }
    }
    error!("failed to find VM[{}] in vm cfg entry list", vmid);
    Err(())
}

/// Adds a virtual machine configuration entry to DEF_VM_CONFIG_TABLE.
pub fn vm_cfg_add_vm_entry(mut vm_cfg_entry: VmConfigEntry) -> Result<usize, ()> {
    let mut vm_config = DEF_VM_CONFIG_TABLE.lock();
    match vm_config.generate_vm_id() {
        Ok(vm_id) => {
            if vm_id == 0 && (!vm_config.entries.is_empty() || vm_config.vm_num > 0) {
                panic!("error in mvm config init, the def vm config table is not empty");
            }
            vm_cfg_entry.set_id(vm_id);
            vm_config.vm_num += 1;
            vm_config.entries.push(vm_cfg_entry.clone());
            info!(
                "\nSuccessfully add {}[{}] name {:?}, currently vm_num {}",
                if vm_id == 0 { "MVM" } else { "GVM" },
                vm_cfg_entry.id(),
                vm_cfg_entry.name,
                vm_config.vm_num
            );

            Ok(vm_id)
        }
        Err(_) => {
            error!("vm_cfg_add_vm_entry, vm num reached max value");
            Err(())
        }
    }
}

/// Adds a virtual machine configuration entry to DEF_VM_CONFIG_TABLE.
pub fn vm_cfg_remove_vm_entry(vm_id: usize) {
    let mut vm_config = DEF_VM_CONFIG_TABLE.lock();
    for (idx, vm_cfg_entry) in vm_config.entries.iter().enumerate() {
        if vm_cfg_entry.id == vm_id {
            vm_config.vm_num -= 1;
            vm_config.remove_vm_id(vm_id);
            match vm_config.entries[idx].mediated_block_index() {
                None => {}
                Some(block_idx) => {
                    mediated_blk_free(block_idx);
                }
            }
            vm_config.entries.remove(idx);
            // println!("remove VM[{}] config from vm-config-table", vm_id);
            return;
        }
    }
    error!("VM[{}] config not found in vm-config-table", vm_id);
}

/// Generates a new VM Config Entry and sets basic values.
pub fn vm_cfg_add_vm(config_ipa: usize) -> Result<usize, ()> {
    let config_pa = vm_ipa2pa(&active_vm().unwrap(), config_ipa);
    // SAFETY: config_pa is from user space, it is checked by shyper.ko firstly.
    // And in the function, vm_ipa2pa, it is checked whether the config_pa is in the memory region of the VM.
    let [vm_name_ipa, _vm_name_length, vm_type, cmdline_ipa, _cmdline_length, kernel_load_ipa, device_tree_load_ipa, ramdisk_load_ipa] =
        unsafe { *(config_pa as *const _) };
    info!("\n\nStart to prepare configuration for new VM");

    // Copy VM name from user ipa.
    let vm_name_pa = vm_ipa2pa(&active_vm().unwrap(), vm_name_ipa);
    if vm_name_pa == 0 {
        error!("illegal vm_name_ipa {:x}", vm_name_ipa);
        return Err(());
    }
    // SAFETY: We have checked the vm_name_pa is in the memory region of the VM by vm_ipa2pa.
    let vm_name_str = unsafe { CStr::from_ptr(vm_name_pa as *const _) }
        .to_string_lossy()
        .to_string();

    // Copy VM cmdline from user ipa.
    let cmdline_pa = vm_ipa2pa(&active_vm().unwrap(), cmdline_ipa);
    if cmdline_pa == 0 {
        error!("illegal cmdline_ipa {:x}", cmdline_ipa);
        return Err(());
    }
    // SAFETY: We have checked the cmdline_pa is in the memory region of the VM by vm_ipa2pa.
    let cmdline_str = unsafe { CStr::from_ptr(cmdline_pa as *const _) }
        .to_string_lossy()
        .to_string();

    // Generate a new VM config entry.
    let new_vm_cfg = VmConfigEntry::new(
        vm_name_str,
        cmdline_str,
        vm_type,
        kernel_load_ipa,
        device_tree_load_ipa,
        ramdisk_load_ipa,
    );

    debug!("      VM name is [{:?}]", new_vm_cfg.name);
    debug!("      cmdline is [{:?}]", new_vm_cfg.cmdline);
    debug!("      ramdisk is [0x{:x}]", new_vm_cfg.ramdisk_load_ipa());
    vm_cfg_add_vm_entry(new_vm_cfg)
}

/// Deletes a VM config entry.
pub fn vm_cfg_del_vm(vmid: usize) -> Result<usize, ()> {
    info!("VM[{}] delete config entry", vmid);
    vm_cfg_remove_vm_entry(vmid);
    Ok(0)
}

/// Add VM memory region according to VM id.
pub fn vm_cfg_add_mem_region(vmid: usize, ipa_start: usize, length: usize) -> Result<usize, ()> {
    vm_cfg_editor(vmid, |vm_cfg| {
        vm_cfg.add_memory_cfg(ipa_start, length);
        info!(
            "\nVM[{}] vm_cfg_add_mem_region: add region start_ipa {:x} length {:x}",
            vmid, ipa_start, length
        );
        Ok(0)
    })
}

/// Set VM CPU config according to VM id.
pub fn vm_cfg_set_cpu(vmid: usize, num: usize, allocate_bitmap: usize, master: usize) -> Result<usize, ()> {
    vm_cfg_editor(vmid, |vm_cfg| {
        vm_cfg.set_cpu_cfg(num, allocate_bitmap, master);

        info!(
            "\nVM[{}] vm_cfg_set_cpu: num {} allocate_bitmap {} master {:?}",
            vmid,
            vm_cfg.cpu_num(),
            vm_cfg.cpu_allocated_bitmap(),
            vm_cfg.cpu_master()
        );

        Ok(0)
    })
}

/// Add emulated device config for VM.
pub fn vm_cfg_add_emu_dev(
    vmid: usize,
    name_ipa: usize,
    base_ipa: usize,
    length: usize,
    irq_id: usize,
    cfg_list_ipa: usize,
    emu_type: usize,
) -> Result<usize, ()> {
    vm_cfg_editor(vmid, |vm_cfg| {
        let emu_cfg_list = vm_cfg.emulated_device_list();

        // Copy emu device name from user ipa.
        let name_pa = vm_ipa2pa(&active_vm().unwrap(), name_ipa);
        if name_pa == 0 {
            error!("illegal emulated device name_ipa {:x}", name_ipa);
            return Err(());
        }
        // SAFETY:
        // We have checked the name_pa is in the memory region of the VM by vm_ipa2pa.
        let name_str = unsafe { CStr::from_ptr(name_pa as *const _) }
            .to_string_lossy()
            .to_string();
        // Copy emu device cfg list from user ipa.
        let cfg_list_pa = vm_ipa2pa(&active_vm().unwrap(), cfg_list_ipa);
        if cfg_list_pa == 0 {
            error!("illegal emulated device cfg_list_ipa {:x}", cfg_list_ipa);
            return Err(());
        }
        let cfg_list = vec![0_usize; CFG_MAX_NUM];
        // SAFETY:
        // We have both read and write access to the src and dst memory regions.
        // The copied size will not exceed the memory region.
        unsafe {
            memcpy(
                &cfg_list[0] as *const _ as *const u8,
                cfg_list_pa as *mut u8,
                CFG_MAX_NUM * 8, // sizeof(usize) / sizeof(u8)
            );
        }

        info!(
            concat!(
                "\nVM[{}] vm_cfg_add_emu_dev: ori emu dev num {}\n",
                "    name {:?}\n",
                "     cfg_list {:?}\n",
                "     base ipa {:x} length {:x} irq_id {} emu_type {}"
            ),
            vmid,
            emu_cfg_list.len(),
            name_str,
            cfg_list,
            base_ipa,
            length,
            irq_id,
            emu_type
        );

        let emu_dev_type = EmuDeviceType::from_usize(emu_type);
        let emu_dev_cfg = VmEmulatedDeviceConfig {
            name: name_str,
            base_ipa,
            length,
            irq_id,
            cfg_list,
            emu_type: match emu_dev_type {
                EmuDeviceType::EmuDeviceTVirtioBlkMediated => EmuDeviceType::EmuDeviceTVirtioBlk,
                _ => emu_dev_type,
            },
            mediated: matches!(
                EmuDeviceType::from_usize(emu_type),
                EmuDeviceType::EmuDeviceTVirtioBlkMediated
            ),
        };
        vm_cfg.add_emulated_device_cfg(emu_dev_cfg);

        // Set GVM Mediated Blk Index Here.
        if emu_dev_type == EmuDeviceType::EmuDeviceTVirtioBlkMediated {
            let med_blk_index = match mediated_blk_request() {
                Ok(idx) => idx,
                Err(_) => {
                    error!("no more medaited blk for vm {}", vmid);
                    return Err(());
                }
            };
            vm_cfg.set_mediated_block_index(med_blk_index);
        }

        Ok(0)
    })
}

/// Add passthrough device config region for VM
pub fn vm_cfg_add_passthrough_device_region(
    vmid: usize,
    base_ipa: usize,
    base_pa: usize,
    length: usize,
) -> Result<usize, ()> {
    // Get VM config entry.
    vm_cfg_editor(vmid, |vm_cfg| {
        info!(
            concat!(
                "\nVM[{}] vm_cfg_add_pt_dev: \n",
                "     base_ipa {:x} base_pa {:x} length {:x}"
            ),
            vmid, base_ipa, base_pa, length
        );

        vm_cfg.add_passthrough_device_region(base_ipa, base_pa, length);
        Ok(0)
    })
}

/// Add passthrough device config irqs for VM.
pub fn vm_cfg_add_passthrough_device_irqs(vmid: usize, irqs_base_ipa: usize, irqs_length: usize) -> Result<usize, ()> {
    info!(
        "\nVM[{}] vm_cfg_add_pt_dev irqs:\n     base_ipa {:x} length {:x}",
        vmid, irqs_base_ipa, irqs_length
    );

    // Copy passthrough device irqs from user ipa.
    let irqs_base_pa = vm_ipa2pa(&active_vm().unwrap(), irqs_base_ipa);
    if irqs_base_pa == 0 {
        error!("illegal irqs_base_ipa {:x}", irqs_base_ipa);
        return Err(());
    }
    let mut irqs = vec![0_usize; irqs_length];
    if irqs_length > 0 {
        // SAFETY:
        // We have both read and write access to the src and dst memory regions.
        // The copied size will not exceed the memory region.
        unsafe {
            memcpy(
                &irqs[0] as *const _ as *const u8,
                irqs_base_pa as *mut u8,
                irqs_length * 8, // sizeof(usize) / sizeof(u8)
            );
        }
    }
    debug!("      irqs {:?}", irqs);

    vm_cfg_editor(vmid, |vm_cfg| {
        vm_cfg.add_passthrough_device_irqs(&mut irqs);
        Ok(0)
    })
}

/// Add passthrough device config streams ids for VM
pub fn vm_cfg_add_passthrough_device_streams_ids(
    vmid: usize,
    streams_ids_base_ipa: usize,
    streams_ids_length: usize,
) -> Result<usize, ()> {
    info!(
        "\nVM[{}] vm_cfg_add_pt_dev streams ids:\n     streams_ids_base_ipa {:x} streams_ids_length {:x}",
        vmid, streams_ids_base_ipa, streams_ids_length
    );

    // Copy passthrough device streams ids from user ipa.
    let streams_ids_base_pa = vm_ipa2pa(&active_vm().unwrap(), streams_ids_base_ipa);
    if streams_ids_base_pa == 0 {
        error!("illegal streams_ids_base_ipa {:x}", streams_ids_base_ipa);
        return Err(());
    }
    let mut streams_ids = vec![0_usize, streams_ids_length];
    if streams_ids_length > 0 {
        // SAFETY:
        // We have both read and write access to the src and dst memory regions.
        // The copied size will not exceed the memory region.
        unsafe {
            memcpy(
                &streams_ids[0] as *const _ as *const u8,
                streams_ids_base_pa as *mut u8,
                streams_ids_length * 8, // sizeof(usize) / sizeof(u8)
            );
        }
    }
    debug!("      get streams_ids {:?}", streams_ids);

    vm_cfg_editor(vmid, |vm_cfg| {
        vm_cfg.add_passthrough_device_streams_ids(&mut streams_ids);
        Ok(0)
    })
}

/// Add device tree device config for VM
pub fn vm_cfg_add_dtb_dev(
    vmid: usize,
    name_ipa: usize,
    dev_type: usize,
    irq_list_ipa: usize,
    irq_list_length: usize,
    addr_region_ipa: usize,
    addr_region_length: usize,
) -> Result<usize, ()> {
    info!(
        "\nVM[{}] vm_cfg_add_dtb_dev:\n     dev_type {} irq_list_length {} addr_region_ipa {:x} addr_region_length {:x}",
        vmid, dev_type, irq_list_length, addr_region_ipa, addr_region_length
    );

    // Copy DTB device name from user ipa.
    let name_pa = vm_ipa2pa(&active_vm().unwrap(), name_ipa);
    if name_pa == 0 {
        error!("illegal dtb_dev name ipa {:x}", name_ipa);
        return Err(());
    }
    // SAFETY: We have checked the name_pa is in the memory region of the VM by vm_ipa2pa.
    let dtb_dev_name_str = unsafe { CStr::from_ptr(name_pa as *const _) }
        .to_string_lossy()
        .to_string();
    debug!("      get dtb dev name {:?}", dtb_dev_name_str);

    // Copy DTB device irq list from user ipa.
    let irq_list_pa = vm_ipa2pa(&active_vm().unwrap(), irq_list_ipa);
    if irq_list_pa == 0 {
        error!("illegal dtb_dev irq list ipa {:x}", irq_list_ipa);
        return Err(());
    }
    let mut dtb_irq_list: Vec<usize> = Vec::new();

    if irq_list_length > 0 {
        let tmp_dtb_irq_list = [0_usize, irq_list_length];
        // SAFETY:
        // We have both read and write access to the src and dst memory regions.
        // The copied size will not exceed the memory region.
        unsafe {
            memcpy(
                &tmp_dtb_irq_list[0] as *const _ as *const u8,
                irq_list_pa as *mut u8,
                irq_list_length * 8, // sizeof(usize) / sizeof(u8)
            );
        }
        for i in 0..irq_list_length {
            dtb_irq_list.push(tmp_dtb_irq_list[i]);
        }
    }
    debug!("      get dtb dev dtb_irq_list {:?}", dtb_irq_list);

    let vm_dtb_dev = VmDtbDevConfig {
        name: dtb_dev_name_str,
        dev_type: DtbDevType::from(dev_type),
        irqs: dtb_irq_list,
        addr_region: VmRegion {
            ipa_start: addr_region_ipa,
            length: addr_region_length,
        },
    };

    vm_cfg_editor(vmid, |vm_cfg| {
        // Get DTB device config list.
        vm_cfg.add_dtb_device(vm_dtb_dev);

        Ok(0)
    })
}

/// Final step for GVM configuration.
/// Set up GVM configuration and VM kernel image load region.
fn vm_cfg_finish_configuration(vmid: usize, _img_size: usize) -> alloc::sync::Arc<Vm> {
    // Set up GVM configuration.
    vmm_init_gvm(vmid);

    // Get VM structure.
    match vm(vmid) {
        None => {
            panic!("vm_cfg_upload_kernel_image:failed to init VM[{}]", vmid);
        }
        Some(vm) => vm,
    }
}

/// Uploads the kernel image file from MVM user space.
///
/// This function is the last step in GVM configuration. It sets up the GVM and loads the kernel
/// image into the specified VM.
pub fn vm_cfg_upload_kernel_image(
    vmid: usize,
    img_size: usize,
    cache_ipa: usize,
    load_offset: usize,
    load_size: usize,
) -> Result<usize, ()> {
    // Before upload kernel image, set GVM.
    let vm = match vm(vmid) {
        None => {
            info!(
                "\nSuccessfully add configuration file for VM [{}]\nStart to init...",
                vmid
            );
            // This code should only run once.
            vm_cfg_finish_configuration(vmid, img_size)
        }
        Some(vm) => vm,
    };
    let config = vm.config();

    info!(
        "VM[{}] Upload kernel image. cache_ipa:{:x} load_offset:{:x} load_size:{:x}",
        vmid, cache_ipa, load_offset, load_size
    );
    // Get cache pa.
    // TODO: In the Hypervisor, we shouldn't use the cache_pa directly.
    // Instead, we should translate the IPA to the HVA and use the HVA to access the PA.
    // But currently, we don't have the HVA in the Hypervisor.
    let cache_pa = vm_ipa2pa(&active_vm().unwrap(), cache_ipa);
    let dest_pa = vm.ipa2pa(config.kernel_load_ipa() + load_offset);

    if cache_pa == 0 {
        error!("illegal cache ipa {:x}", cache_ipa);
        return Err(());
    }
    // SAFETY: We have checked the cache_pa is in the memory region of the VM by vm_ipa2pa.
    let src = unsafe { core::slice::from_raw_parts_mut(cache_pa as *mut u8, load_size) };

    let dst = unsafe { core::slice::from_raw_parts_mut(dest_pa as *mut u8, load_size) };
    dst.copy_from_slice(src);
    Ok(0)
}

/// Uploads the device tree from MVM user space.
pub fn vm_cfg_upload_device_tree(
    vmid: usize,
    _img_size: usize,
    cache_ipa: usize,
    load_offset: usize,
    load_size: usize,
) -> Result<usize, ()> {
    info!(
        "vm_cfg_upload_device_tree: VM[{}] upload device tree. cache_ipa: {:x} load_offset: {:x} load_size: {}",
        vmid, cache_ipa, load_offset, load_size,
    );

    let cache_pa = vm_ipa2pa(&active_vm().unwrap(), cache_ipa);
    if cache_pa == 0 {
        error!("illegal cache ipa {:x}", cache_ipa);
        return Err(());
    }

    // SAFETY: We have checked the cache_pa is in the memory region of the VM by vm_ipa2pa.
    let src = unsafe { core::slice::from_raw_parts(cache_pa as *mut u8, load_size) };

    vm_cfg_editor(vmid, |cfg| {
        cfg.fdt_overlay.extend_from_slice(src);

        Ok(0)
    })
}
