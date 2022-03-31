use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use spin::Mutex;

use crate::board::*;
// use crate::board::*;
use crate::device::EmuDeviceType;
use crate::kernel::INTERRUPT_IRQ_GUEST_TIMER;
use crate::kernel::{active_vm, vm_ipa2pa, VmType, VM_NUM_MAX};
use crate::lib::memcpy_safe;

const NAME_MAX_LEN: usize = 32;
const PASSTHROUGH_DEV_MAX_NUM: usize = 128;
const EMULATED_DEV_MAX_NUM: usize = 16;

#[derive(Clone, PartialEq)]
pub enum DtbDevType {
    DevSerial = 0,
    DevGicd = 1,
    DevGicc = 2,
}

impl DtbDevType {
    pub fn from_usize(value: usize) -> DtbDevType {
        match value {
            0 => DtbDevType::DevSerial,
            1 => DtbDevType::DevGicd,
            2 => DtbDevType::DevGicc,
            _ => panic!("Unknown DtbDevType value: {}", value),
        }
    }
}

#[derive(Clone)]
pub struct VmEmulatedDeviceConfig {
    pub name: Option<String>,
    pub base_ipa: usize,
    pub length: usize,
    pub irq_id: usize,
    pub cfg_list: Vec<usize>,
    pub emu_type: EmuDeviceType,
    pub mediated: bool,
}

pub struct VmEmulatedDeviceConfigList {
    pub emu_dev_list: Vec<VmEmulatedDeviceConfig>,
}

impl VmEmulatedDeviceConfigList {
    pub const fn default() -> VmEmulatedDeviceConfigList {
        VmEmulatedDeviceConfigList {
            emu_dev_list: Vec::new(),
        }
    }
}

#[derive(Default, Clone)]
pub struct PassthroughRegion {
    pub ipa: usize,
    pub pa: usize,
    pub length: usize,
}

#[derive(Default, Clone)]
pub struct VmPassthroughDeviceConfig {
    pub regions: Vec<PassthroughRegion>,
    pub irqs: Vec<usize>,
    pub streams_ids: Vec<usize>,
}

impl VmPassthroughDeviceConfig {
    pub const fn default() -> VmPassthroughDeviceConfig {
        VmPassthroughDeviceConfig {
            regions: Vec::new(),
            irqs: Vec::new(),
            streams_ids: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct VmRegion {
    pub ipa_start: usize,
    pub length: usize,
}

impl VmRegion {
    pub const fn default() -> VmRegion {
        VmRegion {
            ipa_start: 0,
            length: 0,
        }
    }
}

#[derive(Clone)]
pub struct VmMemoryConfig {
    pub region: Vec<VmRegion>,
}

impl VmMemoryConfig {
    pub const fn default() -> VmMemoryConfig {
        VmMemoryConfig { region: vec![] }
    }
}

#[derive(Clone)]
pub struct VmImageConfig {
    pub kernel_img_name: Option<&'static str>,
    pub kernel_load_ipa: usize,
    pub kernel_entry_point: usize,
    // pub device_tree_filename: Option<&'static str>,
    pub device_tree_load_ipa: usize,
    // pub ramdisk_filename: Option<&'static str>,
    pub ramdisk_load_ipa: usize,
}

impl VmImageConfig {
    pub const fn default() -> VmImageConfig {
        VmImageConfig {
            kernel_img_name: None,
            kernel_load_ipa: 0,
            kernel_entry_point: 0,
            // device_tree_filename: None,
            device_tree_load_ipa: 0,
            // ramdisk_filename: None,
            ramdisk_load_ipa: 0,
        }
    }
}

#[derive(Clone)]
pub struct VmCpuConfig {
    pub num: usize,
    pub allocate_bitmap: u32,
    pub master: i32,
}

impl VmCpuConfig {
    pub const fn default() -> VmCpuConfig {
        VmCpuConfig {
            num: 0,
            allocate_bitmap: 0,
            master: 0,
        }
    }
}

#[derive(Clone)]
pub struct AddrRegions {
    pub ipa: usize,
    pub length: usize,
}

#[derive(Clone)]
pub struct VmDtbDevConfig {
    pub name: String,
    pub dev_type: DtbDevType,
    pub irqs: Vec<usize>,
    pub addr_region: AddrRegions,
}

#[derive(Clone)]
pub struct VMDtbDevConfigList {
    pub dtb_device_list: Vec<VmDtbDevConfig>,
}

impl VMDtbDevConfigList {
    pub const fn default() -> VMDtbDevConfigList {
        VMDtbDevConfigList {
            dtb_device_list: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct VmConfigEntry {
    // VM id, generate inside hypervisor.
    pub id: usize,
    // Following configs are not intended to be modified during configuration.
    pub name: Option<String>,
    pub name_vec: Option<Vec<char>>,
    pub os_type: VmType,
    pub cmdline: String,
    pub cmdline_vec: Option<Vec<char>>,
    pub med_blk_idx: Option<usize>,
    pub image: VmImageConfig,
    // Following config can be modified during configuration.
    pub memory: Arc<Mutex<VmMemoryConfig>>,
    pub cpu: Arc<Mutex<VmCpuConfig>>,
    pub vm_emu_dev_confg: Arc<Mutex<VmEmulatedDeviceConfigList>>,
    pub vm_pt_dev_confg: Arc<Mutex<VmPassthroughDeviceConfig>>,
    pub vm_dtb_devs: Arc<Mutex<VMDtbDevConfigList>>,
}

impl VmConfigEntry {
    pub fn default() -> VmConfigEntry {
        VmConfigEntry {
            id: 0,
            name: Some(String::from("unknown")),
            name_vec: None,
            os_type: VmType::VmTBma,
            image: VmImageConfig::default(),
            cmdline: String::from("root=/dev/vda rw audit=0"),
            cmdline_vec: None,
            med_blk_idx: None,

            memory: Arc::new(Mutex::new(VmMemoryConfig::default())),
            cpu: Arc::new(Mutex::new(VmCpuConfig::default())),
            vm_emu_dev_confg: Arc::new(Mutex::new(VmEmulatedDeviceConfigList::default())),
            vm_pt_dev_confg: Arc::new(Mutex::new(VmPassthroughDeviceConfig::default())),
            vm_dtb_devs: Arc::new(Mutex::new(VMDtbDevConfigList::default())),
        }
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn set_id_cfg(&mut self, id: usize) {
        self.id = id;
    }

    pub fn memory_region(&self) -> Vec<VmRegion> {
        let mem_cfg = self.memory.lock();
        mem_cfg.region.clone()
    }

    pub fn add_memory_cfg(&self, ipa_start: usize, length: usize) {
        let mut mem_cfg = self.memory.lock();
        mem_cfg.region.push(VmRegion { ipa_start, length });
    }

    pub fn cpu_num(&self) -> usize {
        let cpu_cfg = self.cpu.lock();
        cpu_cfg.num
    }

    pub fn cpu_allocated_bitmap(&self) -> u32 {
        let cpu_cfg = self.cpu.lock();
        cpu_cfg.allocate_bitmap
    }

    pub fn cpu_master(&self) -> usize {
        let cpu_cfg = self.cpu.lock();
        cpu_cfg.master as usize
    }

    pub fn set_cpu_cfg(&self, num: usize, allocate_bitmap: usize, master: usize) {
        let mut cpu_cfg = self.cpu.lock();
        cpu_cfg.num = num;
        cpu_cfg.allocate_bitmap = allocate_bitmap as u32;
        cpu_cfg.master = master as i32;
    }

    pub fn emulated_device_list(&self) -> Vec<VmEmulatedDeviceConfig> {
        let emu_dev_cfg = self.vm_emu_dev_confg.lock();
        emu_dev_cfg.emu_dev_list.clone()
    }

    pub fn add_emulated_device_cfg(&self, _emu_dev_cfg: VmEmulatedDeviceConfig) {
        let mut emu_dev_cfgs = self.vm_emu_dev_confg.lock();
        emu_dev_cfgs.emu_dev_list.push(_emu_dev_cfg);
    }

    pub fn passthrough_device_regions(&self) -> Vec<PassthroughRegion> {
        let pt_dev_cfg = self.vm_pt_dev_confg.lock();
        pt_dev_cfg.regions.clone()
    }

    pub fn passthrough_device_irqs(&self) -> Vec<usize> {
        let pt_dev_cfg = self.vm_pt_dev_confg.lock();
        pt_dev_cfg.irqs.clone()
    }

    pub fn passthrough_device_stread_ids(&self) -> Vec<usize> {
        let pt_dev_cfg = self.vm_pt_dev_confg.lock();
        pt_dev_cfg.streams_ids.clone()
    }

    pub fn add_passthrough_device_region(&self, base_ipa: usize, base_pa: usize, length: usize) {
        let mut pt_dev_cfg = self.vm_pt_dev_confg.lock();
        let pt_region_cfg = PassthroughRegion {
            ipa: base_ipa,
            pa: base_pa,
            length,
        };
        pt_dev_cfg.regions.push(pt_region_cfg)
    }

    pub fn add_passthrough_device_irqs(&self, _irqs: &mut Vec<usize>) {
        let mut pt_dev_cfg = self.vm_pt_dev_confg.lock();
        pt_dev_cfg.irqs.append(_irqs);
    }

    pub fn add_passthrough_device_streams_ids(&self, _streams_ids: &mut Vec<usize>) {
        let mut pt_dev_cfg = self.vm_pt_dev_confg.lock();
        pt_dev_cfg.streams_ids.append(_streams_ids);
    }

    pub fn dtb_device_list(&self) -> Vec<VmDtbDevConfig> {
        let dtb_dev_cfg = self.vm_dtb_devs.lock();
        dtb_dev_cfg.dtb_device_list.clone()
    }

    pub fn add_dtb_device(&self, _dtb_dev_cfg: VmDtbDevConfig) {
        let mut dtb_dev_cfg = self.vm_dtb_devs.lock();
        dtb_dev_cfg.dtb_device_list.push(_dtb_dev_cfg);
    }

    pub fn gicc_addr(&self) -> usize {
        let dtb_devs = self.vm_dtb_devs.lock();
        for dev in &dtb_devs.dtb_device_list {
            match dev.dev_type {
                DtbDevType::DevGicc => {
                    return dev.addr_region.ipa;
                }
                _ => {}
            }
        }
        0
    }

    pub fn gicd_addr(&self) -> usize {
        let dtb_devs = self.vm_dtb_devs.lock();
        for dev in &dtb_devs.dtb_device_list {
            match dev.dev_type {
                DtbDevType::DevGicd => {
                    return dev.addr_region.ipa;
                }
                _ => {}
            }
        }
        0
    }
}

#[derive(Clone)]
pub struct VmConfigTable {
    pub name: Option<&'static str>,
    pub vm_num: usize,
    pub entries: Vec<VmConfigEntry>,
}

impl VmConfigTable {
    pub const fn default() -> VmConfigTable {
        VmConfigTable {
            name: None,
            vm_num: 0,
            entries: Vec::new(),
        }
    }
}

lazy_static! {
    pub static ref DEF_VM_CONFIG_TABLE: Mutex<VmConfigTable> = Mutex::new(VmConfigTable::default());
}

pub fn vm_cfg_set_config_name(name: &'static str) {
    let mut vm_config = DEF_VM_CONFIG_TABLE.lock();
    vm_config.name = Some(name);
}

pub fn vm_num() -> usize {
    let vm_config = DEF_VM_CONFIG_TABLE.lock();
    vm_config.entries.len()
}

pub fn vm_type(id: usize) -> VmType {
    let vm_config = DEF_VM_CONFIG_TABLE.lock();
    vm_config.entries[id].os_type
}

pub fn vm_cfg_entry(vmid: usize) -> Option<VmConfigEntry> {
    let vm_config = DEF_VM_CONFIG_TABLE.lock();
    for _vm_cfg_entry in vm_config.entries.iter() {
        if _vm_cfg_entry.id == vmid {
            return Some(_vm_cfg_entry.clone());
        }
    }
    println!("failed to find VM[{}] in vm cfg entry list", vmid);
    return None;
}

pub fn vm_cfg_add_mvm_entry(mvm_cfg_entry: VmConfigEntry) {
    let mut vm_config = DEF_VM_CONFIG_TABLE.lock();
    if vm_config.entries.len() > 0 || vm_config.vm_num > 0 {
        panic!("error in mvm config init, the def vm config table is not empty");
    }
    println!(
        "\nSuccessfully add Manager VM {:?}, id {}, currently vm_num {}\n",
        mvm_cfg_entry.clone().name.unwrap(),
        mvm_cfg_entry.id(),
        vm_config.vm_num
    );

    vm_config.vm_num += 1;
    vm_config.entries.push(mvm_cfg_entry);
}

// Todo: gererate vm id in a more comprehensive way.
pub fn vm_cfg_generate_vm_id(vm_num: usize) -> usize {
    vm_num
}

/* Add VM config entry to DEF_VM_CONFIG_TABLE */
pub fn vm_cfg_add_vm_entry(mut vm_cfg_entry: VmConfigEntry) -> Result<usize, ()> {
    println!("vm_cfg_add_vm_entry: add guest VM [{}]", vm_cfg_entry.id());

    let mut vm_config = DEF_VM_CONFIG_TABLE.lock();
    if vm_config.vm_num + 1 > VM_NUM_MAX {
        println!("vm_cfg_add_vm_entry, vm num reached max value");
        return Err(());
    }

    vm_cfg_entry.set_id_cfg(vm_cfg_generate_vm_id(vm_config.vm_num));

    vm_config.vm_num += 1;
    vm_config.entries.push(vm_cfg_entry.clone());

    println!(
        "\nSuccessfully add GVM[{}] name {:?}, currently vm_num {}\n",
        vm_cfg_entry.id(),
        vm_cfg_entry.clone().name.unwrap(),
        vm_config.vm_num
    );

    Ok(vm_cfg_entry.id())
}

/* Generate a new VM Config Entry, set basic value */
pub fn vm_cfg_add_vm(
    vm_name_ipa: usize,
    vm_name_length: usize,
    vm_type: usize,
    cmdline_ipa: usize,
    cmdline_length: usize,
    kernel_load_ipa: usize,
    device_tree_load_ipa: usize,
) -> Result<usize, ()> {
    println!("\n\nStart to prepare configuration for new VM");
    println!("  vm_cfg_add_vm():");
    println!(
        "   vm_type {} kernel_load_ipa 0x{:x} device_tree_load_ipa 0x{:x}",
        vm_type, kernel_load_ipa, device_tree_load_ipa
    );

    // Copy VM name from user ipa.
    let vm_name_pa = vm_ipa2pa(active_vm().unwrap(), vm_name_ipa);
    if vm_name_pa == 0 {
        println!("illegal vm_name_ipa {:x}", vm_name_ipa);
        return Err(());
    }
    let vm_name_u8 = vec![0 as u8; vm_name_length];
    memcpy_safe(
        &vm_name_u8[0] as *const _ as *const u8,
        vm_name_pa as *mut u8,
        vm_name_length,
    );

    let vm_name_str = match String::from_utf8(vm_name_u8.clone()) {
        Ok(_str) => _str,
        Err(error) => {
            println!("error: {:?} in parsing the vm_name {:?}", error, vm_name_u8);
            String::from("unknown")
        }
    };

    // Copy VM cmdline from user ipa.
    let cmdline_pa = vm_ipa2pa(active_vm().unwrap(), cmdline_ipa);
    if cmdline_pa == 0 {
        println!("illegal cmdline_ipa {:x}", cmdline_ipa);
        return Err(());
    }
    let cmdline_u8 = vec![0 as u8; cmdline_length];
    memcpy_safe(
        &cmdline_u8[0] as *const _ as *const u8,
        cmdline_ipa as *mut u8,
        cmdline_length,
    );
    let cmdline_str = match String::from_utf8(cmdline_u8.clone()) {
        Ok(_str) => _str,
        Err(error) => {
            println!("error: {:?} in parsing the cmdline {:?}", error, cmdline_u8);
            String::from("unknown")
        }
    };

    // Generate a new VM config entry.
    let mut new_vm_cfg = VmConfigEntry::default();
    new_vm_cfg.name = Some(vm_name_str);
    new_vm_cfg.cmdline = cmdline_str;

    println!("      VM name is [{:?}]", new_vm_cfg.name.clone().unwrap());
    println!("      cmdline is \"{:?}\"", new_vm_cfg.cmdline.clone());
    vm_cfg_add_vm_entry(new_vm_cfg)
}

/* Delete a VM config entry */
pub fn vm_cfg_del_vm(vmid: usize) -> Result<usize, ()> {
    println!("VM[{}] vm_cfg_del_vm: wait for implementation", vmid);
    Ok(0)
}

/* Add VM memory region according to VM id */
pub fn vm_cfg_add_mem_region(vmid: usize, ipa_start: usize, length: usize) -> Result<usize, ()> {
    let vm_cfg = match vm_cfg_entry(vmid) {
        Some(vm_cfg) => vm_cfg,
        None => return Err(()),
    };
    vm_cfg.add_memory_cfg(ipa_start, length);
    println!(
        "VM[{}] vm_cfg_add_mem_region: add region start_ipa {:x} length {:x}",
        vmid, ipa_start, length
    );
    Ok(0)
}

/* Set VM cpu config according to VM id */
pub fn vm_cfg_set_cpu(
    vmid: usize,
    num: usize,
    allocate_bitmap: usize,
    master: usize,
) -> Result<usize, ()> {
    let vm_cfg = match vm_cfg_entry(vmid) {
        Some(_vm_cfg) => _vm_cfg,
        None => return Err(()),
    };

    vm_cfg.set_cpu_cfg(num, allocate_bitmap, master);

    println!(
        " VM[{}] vm_cfg_set_cpu: num {} allocate_bitmap {} master {}",
        vmid,
        vm_cfg.cpu_num(),
        vm_cfg.cpu_allocated_bitmap(),
        vm_cfg.cpu_master()
    );

    Ok(0)
}

/* Add emulated device config for VM */
pub fn vm_cfg_add_emu_dev(
    vmid: usize,
    name_ipa: usize,
    base_ipa: usize,
    length: usize,
    irq_id: usize,
    cfg_list_ipa: usize,
    emu_type: usize,
) -> Result<usize, ()> {
    let vm_cfg = match vm_cfg_entry(vmid) {
        Some(vm_cfg) => vm_cfg,
        None => return Err(()),
    };
    let emu_cfg_list = vm_cfg.emulated_device_list();

    // Copy emu device name from user ipa.
    let name_pa = vm_ipa2pa(active_vm().unwrap(), name_ipa);
    if name_pa == 0 {
        println!("illegal emulated device name_ipa {:x}", name_ipa);
        return Err(());
    }
    let name_u8 = vec![0 as u8; NAME_MAX_LEN];
    memcpy_safe(
        &name_u8[0] as *const _ as *const u8,
        name_pa as *mut u8,
        NAME_MAX_LEN,
    );
    let name_str = match String::from_utf8(name_u8.clone()) {
        Ok(_str) => _str,
        Err(error) => {
            println!(
                "error: {:?} in parsing the emulated device name {:?}",
                error, name_u8
            );
            String::from("unknown")
        }
    };

    println!(
        "VM[{}] vm_cfg_add_emu_dev: ori emu dev num {}\n    name {:?}\n     base ipa {:x} length {:x} irq_id {} emu_type{}",
        vmid,
        emu_cfg_list.len(),
        name_str.clone(),
        base_ipa,length,irq_id,emu_type
    );

    let _emu_dev_type = EmuDeviceType::from_usize(emu_type);
    let emu_dev_cfg = VmEmulatedDeviceConfig {
        name: Some(name_str),
        base_ipa,
        length,
        irq_id,
        cfg_list: Vec::new(),
        emu_type: match _emu_dev_type {
            EmuDeviceType::EmuDeviceTVirtioBlkMediated => EmuDeviceType::EmuDeviceTVirtioBlk,
            _ => _emu_dev_type,
        },
        mediated: match EmuDeviceType::from_usize(emu_type) {
            EmuDeviceType::EmuDeviceTVirtioBlkMediated => true,
            _ => false,
        },
    };
    vm_cfg.add_emulated_device_cfg(emu_dev_cfg);

    Ok(0)
}

/* Add passthrough device config region for VM */
pub fn vm_cfg_add_passthrough_device_region(
    vmid: usize,
    base_ipa: usize,
    base_pa: usize,
    length: usize,
) -> Result<usize, ()> {
    // Get VM config entry.
    let vm_cfg = match vm_cfg_entry(vmid) {
        Some(vm_cfg) => vm_cfg,
        None => return Err(()),
    };
    // Get passthrough device config list.
    let pt_dev_regions = vm_cfg.passthrough_device_regions();

    println!(
        " VM[{}] vm_cfg_add_pt_dev: ori pt dev regions num {}\n     base_ipa {:x} base_pa {:x} length {:x}",
        vmid,
        pt_dev_regions.len(),
        base_ipa, base_pa, length
    );

    vm_cfg.add_passthrough_device_region(base_ipa, base_pa, length);
    Ok(0)
}

/* Add passthrough device config irqs for VM */
pub fn vm_cfg_add_passthrough_device_irqs(
    vmid: usize,
    irqs_base_ipa: usize,
    irqs_length: usize,
) -> Result<usize, ()> {
    println!(
        " VM[{}] vm_cfg_add_pt_dev irqs:\n     base_ipa {:x} length {:x}",
        vmid, irqs_base_ipa, irqs_length
    );

    // Copy passthrough device irqs from user ipa.
    let irqs_base_pa = vm_ipa2pa(active_vm().unwrap(), irqs_base_ipa);
    if irqs_base_pa == 0 {
        println!("illegal irqs_base_ipa {:x}", irqs_base_ipa);
        return Err(());
    }
    let mut irqs = vec![0 as usize, irqs_length];
    memcpy_safe(
        &irqs[0] as *const _ as *const u8,
        irqs_base_pa as *mut u8,
        irqs_length * 8, // sizeof(usize) / sizeof(u8)
    );
    println!("      irqs {:?}", irqs);

    let vm_cfg = match vm_cfg_entry(vmid) {
        Some(vm_cfg) => vm_cfg,
        None => return Err(()),
    };
    vm_cfg.add_passthrough_device_irqs(&mut irqs);
    Ok(0)
}

/* Add passthrough device config streams ids for VM */
pub fn vm_cfg_add_passthrough_device_streams_ids(
    vmid: usize,
    streams_ids_base_ipa: usize,
    streams_ids_length: usize,
) -> Result<usize, ()> {
    println!(
        " VM[{}] vm_cfg_add_pt_dev streams ids:\n     streams_ids_base_ipa {:x} streams_ids_length {:x}",
        vmid,
        streams_ids_base_ipa,
        streams_ids_length
    );

    // Copy passthrough device streams ids from user ipa.
    let streams_ids_base_pa = vm_ipa2pa(active_vm().unwrap(), streams_ids_base_ipa);
    if streams_ids_base_pa == 0 {
        println!("illegal streams_ids_base_ipa {:x}", streams_ids_base_ipa);
        return Err(());
    }
    let mut streams_ids = vec![0 as usize, streams_ids_length];
    memcpy_safe(
        &streams_ids[0] as *const _ as *const u8,
        streams_ids_base_pa as *mut u8,
        streams_ids_length * 8, // sizeof(usize) / sizeof(u8)
    );
    println!("      get streams_ids {:?}", streams_ids);

    let vm_cfg = match vm_cfg_entry(vmid) {
        Some(vm_cfg) => vm_cfg,
        None => return Err(()),
    };
    vm_cfg.add_passthrough_device_streams_ids(&mut streams_ids);
    Ok(0)
}

/* Add device tree device config for VM */
pub fn vm_cfg_add_dtb_dev(
    vmid: usize,
    name_ipa: usize,
    dev_type: usize,
    irq_list_ipa: usize,
    irq_list_length: usize,
    addr_region_ipa: usize,
    addr_region_length: usize,
) -> Result<usize, ()> {
    println!(
        " VM[{}] vm_cfg_add_dtb_dev:\n     name_ipa {:x} dev_type {} irq_list_ipa {:x} irq_list_length {:x} addr_region_ipa {:x} addr_region_length {:x}",
        vmid,name_ipa,dev_type,irq_list_ipa,irq_list_length,addr_region_ipa,addr_region_length
    );

    // Copy DTB device name from user ipa.
    let name_pa = vm_ipa2pa(active_vm().unwrap(), name_ipa);
    if name_pa == 0 {
        println!("illegal dtb_dev name ipa {:x}", name_ipa);
        return Err(());
    }
    let mut dtb_dev_name_u8 = vec![0 as u8; NAME_MAX_LEN];
    memcpy_safe(
        &dtb_dev_name_u8[0] as *const _ as *const u8,
        name_pa as *mut u8,
        NAME_MAX_LEN,
    );
    let dtb_dev_name_str = match String::from_utf8(dtb_dev_name_u8.clone()) {
        Ok(_str) => _str,
        Err(error) => {
            println!(
                "error: {:?} in parsing the DTB device name {:?}",
                error, dtb_dev_name_u8
            );
            String::from("unknown")
        }
    };
    println!("      get dtb dev name {:?}", dtb_dev_name_str);

    // Copy DTB device irq list from user ipa.
    let irq_list_pa = vm_ipa2pa(active_vm().unwrap(), irq_list_ipa);
    if irq_list_pa == 0 {
        println!("illegal dtb_dev irq list ipa {:x}", irq_list_ipa);
        return Err(());
    }
    let mut dtb_irq_list = vec![0 as usize, irq_list_length];
    memcpy_safe(
        &dtb_irq_list[0] as *const _ as *const u8,
        irq_list_pa as *mut u8,
        irq_list_length * 8, // sizeof(usize) / sizeof(u8)
    );

    println!("      get dtb dev dtb_irq_list {:?}", dtb_irq_list);

    // Get VM config entry.
    let vm_cfg = match vm_cfg_entry(vmid) {
        Some(vm_cfg) => vm_cfg,
        None => return Err(()),
    };
    // Get passthrough device config list.
    let dtb_devs = vm_cfg.dtb_device_list();

    println!(
        "       vm_cfg_add_dtb_dev: ori dtb dev num {}",
        dtb_devs.len()
    );

    let vm_dtb_dev = VmDtbDevConfig {
        name: dtb_dev_name_str,
        dev_type: DtbDevType::from_usize(dev_type),
        irqs: dtb_irq_list,
        addr_region: AddrRegions {
            ipa: addr_region_ipa,
            length: addr_region_length,
        },
    };

    vm_cfg.add_dtb_device(vm_dtb_dev);
    Ok(0)
}

pub fn init_tmp_config_for_vm1() {
    println!("init_tmp_config_for_vm1");

    // #################### vm1 emu ######################
    let mut emu_dev_config: Vec<VmEmulatedDeviceConfig> = Vec::new();
    emu_dev_config.push(VmEmulatedDeviceConfig {
        name: Some(String::from("intc@8000000")),
        base_ipa: 0x8000000,
        length: 0x1000,
        irq_id: 0,
        cfg_list: Vec::new(),
        emu_type: EmuDeviceType::EmuDeviceTGicd,
        mediated: false,
    });
    emu_dev_config.push(VmEmulatedDeviceConfig {
        name: Some(String::from("virtio_blk@a000000")),
        base_ipa: 0xa000000,
        length: 0x1000,
        irq_id: 32 + 0x10,
        // cfg_list: vec![DISK_PARTITION_2_START, DISK_PARTITION_2_SIZE],
        cfg_list: vec![0, 8388608],
        // cfg_list: vec![0, 67108864], // 32G
        // cfg_list: vec![0, 209715200], // 100G
        emu_type: EmuDeviceType::EmuDeviceTVirtioBlk,
        mediated: true,
    });
    emu_dev_config.push(VmEmulatedDeviceConfig {
        name: Some(String::from("virtio_console@a002000")),
        base_ipa: 0xa002000,
        length: 0x1000,
        irq_id: 32 + 0x12,
        cfg_list: vec![0, 0xa002000],
        emu_type: EmuDeviceType::EmuDeviceTVirtioConsole,
        mediated: false,
    });
    emu_dev_config.push(VmEmulatedDeviceConfig {
        name: Some(String::from("virtio_net@a001000")),
        base_ipa: 0xa001000,
        length: 0x1000,
        irq_id: 32 + 0x11,
        cfg_list: vec![0x74, 0x56, 0xaa, 0x0f, 0x47, 0xd1],
        emu_type: EmuDeviceType::EmuDeviceTVirtioNet,
        mediated: false,
    });
    // emu_dev_config.push(VmEmulatedDeviceConfig {
    //     name: Some(String::from("vm_service")),
    //     base_ipa: 0,
    //     length: 0,
    //     irq_id: HVC_IRQ,
    //     cfg_list: Vec::new(),
    //     emu_type: EmuDeviceType::EmuDeviceTShyper,
    //     mediated: false,
    // });

    // vm1 passthrough
    let mut pt_dev_config: VmPassthroughDeviceConfig = VmPassthroughDeviceConfig::default();
    pt_dev_config.regions = vec![
        PassthroughRegion {
            ipa: UART_1_ADDR,
            pa: UART_1_ADDR,
            length: 0x1000,
        },
        PassthroughRegion {
            ipa: 0x8010000,
            pa: PLATFORM_GICV_BASE,
            length: 0x2000,
        },
    ];
    pt_dev_config.irqs = vec![UART_1_INT, INTERRUPT_IRQ_GUEST_TIMER];
    // pt_dev_config.irqs = vec![INTERRUPT_IRQ_GUEST_TIMER];

    // vm1 vm_region
    let mut vm_region: Vec<VmRegion> = Vec::new();
    vm_region.push(VmRegion {
        ipa_start: 0x80000000,
        length: 0x40000000,
    });

    let mut vm_dtb_devs: Vec<VmDtbDevConfig> = vec![];
    vm_dtb_devs.push(VmDtbDevConfig {
        name: String::from("gicd"),
        dev_type: DtbDevType::DevGicd,
        irqs: vec![],
        addr_region: AddrRegions {
            ipa: 0x8000000,
            length: 0x1000,
        },
    });
    vm_dtb_devs.push(VmDtbDevConfig {
        name: String::from("gicc"),
        dev_type: DtbDevType::DevGicc,
        irqs: vec![],
        addr_region: AddrRegions {
            ipa: 0x8010000,
            length: 0x2000,
        },
    });
    vm_dtb_devs.push(VmDtbDevConfig {
        name: String::from("serial"),
        dev_type: DtbDevType::DevSerial,
        irqs: vec![UART_1_INT],
        addr_region: AddrRegions {
            ipa: UART_1_ADDR,
            length: 0x1000,
        },
    });

    // vm1 config
    let vm1_config = VmConfigEntry {
        id: 1,
        // name: Some("guest-os-0"),
        name: Some(String::from("guest-os-0")),
        name_vec: None,
        os_type: VmType::VmTOs,
        image: VmImageConfig {
            kernel_img_name: None,
            kernel_load_ipa: 0x80080000,
            kernel_entry_point: 0x80080000,
            device_tree_load_ipa: 0x80000000,
            ramdisk_load_ipa: 0, //0x83000000,
        },
        // cmdline: "root=/dev/vda rw audit=0",
        cmdline: String::from("earlycon console=hvc0,115200n8 root=/dev/vda rw audit=0"),
        cmdline_vec: None,
        med_blk_idx: Some(0),

        memory: Arc::new(Mutex::new(VmMemoryConfig { region: vm_region })),
        cpu: Arc::new(Mutex::new(VmCpuConfig {
            num: 1,
            allocate_bitmap: 0b0010,
            master: 1,
        })),
        vm_emu_dev_confg: Arc::new(Mutex::new(VmEmulatedDeviceConfigList {
            emu_dev_list: emu_dev_config,
        })),
        vm_pt_dev_confg: Arc::new(Mutex::new(pt_dev_config)),
        vm_dtb_devs: Arc::new(Mutex::new(VMDtbDevConfigList {
            dtb_device_list: vm_dtb_devs,
        })),
    };
    println!("generate tmp_config for vm1");
    vm_cfg_add_vm_entry(vm1_config);
}

pub fn init_tmp_config_for_vm2() {
    println!("init_tmp_config_for_vm2");
    let mut vm_config = DEF_VM_CONFIG_TABLE.lock();

    // #################### vm2 emu ######################
    let mut emu_dev_config: Vec<VmEmulatedDeviceConfig> = Vec::new();
    emu_dev_config.push(VmEmulatedDeviceConfig {
        name: Some(String::from("intc@8000000")),
        base_ipa: 0x8000000,
        length: 0x1000,
        irq_id: 0,
        cfg_list: Vec::new(),
        emu_type: EmuDeviceType::EmuDeviceTGicd,
        mediated: false,
    });
    emu_dev_config.push(VmEmulatedDeviceConfig {
        name: Some(String::from("virtio_blk@a000000")),
        base_ipa: 0xa000000,
        length: 0x1000,
        irq_id: 32 + 0x10,
        cfg_list: vec![0, 209715200], // 100G
        emu_type: EmuDeviceType::EmuDeviceTVirtioBlk,
        mediated: true,
    });
    emu_dev_config.push(VmEmulatedDeviceConfig {
        name: Some(String::from("virtio_net@a001000")),
        base_ipa: 0xa001000,
        length: 0x1000,
        irq_id: 32 + 0x11,
        cfg_list: vec![0x74, 0x56, 0xaa, 0x0f, 0x47, 0xd2],
        emu_type: EmuDeviceType::EmuDeviceTVirtioNet,
        mediated: false,
    });

    // vm2 passthrough
    let mut pt_dev_config: VmPassthroughDeviceConfig = VmPassthroughDeviceConfig::default();
    pt_dev_config.regions = vec![
        PassthroughRegion {
            ipa: UART_1_ADDR,
            pa: UART_1_ADDR,
            length: 0x1000,
        },
        PassthroughRegion {
            ipa: 0x8010000,
            pa: PLATFORM_GICV_BASE,
            length: 0x2000,
        },
    ];
    pt_dev_config.irqs = vec![UART_1_INT, INTERRUPT_IRQ_GUEST_TIMER];
    // pt_dev_config.irqs = vec![INTERRUPT_IRQ_GUEST_TIMER];

    // vm2 vm_region
    let mut vm_region: Vec<VmRegion> = Vec::new();
    vm_region.push(VmRegion {
        ipa_start: 0x80000000,
        length: 0x40000000,
    });

    let mut vm_dtb_devs: Vec<VmDtbDevConfig> = vec![];
    vm_dtb_devs.push(VmDtbDevConfig {
        name: String::from("gicd"),
        dev_type: DtbDevType::DevGicd,
        irqs: vec![],
        addr_region: AddrRegions {
            ipa: 0x8000000,
            length: 0x1000,
        },
    });
    vm_dtb_devs.push(VmDtbDevConfig {
        name: String::from("gicc"),
        dev_type: DtbDevType::DevGicc,
        irqs: vec![],
        addr_region: AddrRegions {
            ipa: 0x8010000,
            length: 0x2000,
        },
    });
    vm_dtb_devs.push(VmDtbDevConfig {
        name: String::from("serial"),
        dev_type: DtbDevType::DevSerial,
        irqs: vec![UART_1_INT],
        addr_region: AddrRegions {
            ipa: UART_1_ADDR,
            length: 0x1000,
        },
    });

    // vm2 config
    let vm2_config = VmConfigEntry {
        id: 2,
        name: Some(String::from("guest-os-1")),
        // name: Some("guest-os-1"),
        name_vec: None,
        os_type: VmType::VmTOs,
        image: VmImageConfig {
            kernel_img_name: None,
            kernel_load_ipa: 0x80080000,
            kernel_entry_point: 0x80080000,
            device_tree_load_ipa: 0x80000000,
            ramdisk_load_ipa: 0, //0x83000000,
        },
        // cmdline: "root=/dev/vda rw audit=0",
        cmdline: String::from("earlycon console=ttyS0,115200n8 root=/dev/vda rw audit=0"),
        cmdline_vec: None,
        med_blk_idx: Some(1),

        memory: Arc::new(Mutex::new(VmMemoryConfig { region: vm_region })),
        cpu: Arc::new(Mutex::new(VmCpuConfig {
            num: 1,
            allocate_bitmap: 0b0100,
            master: 2,
        })),
        vm_emu_dev_confg: Arc::new(Mutex::new(VmEmulatedDeviceConfigList {
            emu_dev_list: emu_dev_config,
        })),
        vm_pt_dev_confg: Arc::new(Mutex::new(pt_dev_config)),
        vm_dtb_devs: Arc::new(Mutex::new(VMDtbDevConfigList {
            dtb_device_list: vm_dtb_devs,
        })),
    };
    vm_cfg_add_vm_entry(vm2_config);
}
