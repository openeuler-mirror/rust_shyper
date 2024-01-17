// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use alloc::sync::Arc;
use alloc::vec::Vec;

use spin::{Mutex, Once};

use crate::arch::PAGE_SIZE;
use crate::arch::PageTable;
use crate::arch::Vgic;
use crate::config::VmConfigEntry;
use crate::device::EmuDevs;
use crate::utils::*;
use crate::mm::PageFrame;
use super::vcpu::Vcpu;

pub const VM_NUM_MAX: usize = 8;
pub static VM_IF_LIST: [Mutex<VmInterface>; VM_NUM_MAX] = [const { Mutex::new(VmInterface::default()) }; VM_NUM_MAX];

pub fn vm_if_reset(vm_id: usize) {
    let mut vm_if = VM_IF_LIST[vm_id].lock();
    vm_if.reset();
}

pub fn vm_if_set_state(vm_id: usize, vm_state: VmState) {
    let mut vm_if = VM_IF_LIST[vm_id].lock();
    vm_if.state = vm_state;
}

pub fn vm_if_get_state(vm_id: usize) -> VmState {
    let vm_if = VM_IF_LIST[vm_id].lock();
    vm_if.state
}

pub fn vm_if_set_type(vm_id: usize, vm_type: VmType) {
    let mut vm_if = VM_IF_LIST[vm_id].lock();
    vm_if.vm_type = vm_type;
}

pub fn vm_if_get_type(vm_id: usize) -> VmType {
    let vm_if = VM_IF_LIST[vm_id].lock();
    vm_if.vm_type
}

fn vm_if_set_cpu_id(vm_id: usize, master_cpu_id: usize) {
    let vm_if = VM_IF_LIST[vm_id].lock();
    vm_if.master_cpu_id.call_once(|| master_cpu_id);
    debug!(
        "vm_if_list_set_cpu_id vm [{}] set master_cpu_id {}",
        vm_id, master_cpu_id
    );
}

// todo: rewrite return val to Option<usize>
pub fn vm_if_get_cpu_id(vm_id: usize) -> Option<usize> {
    let vm_if = VM_IF_LIST[vm_id].lock();
    vm_if.master_cpu_id.get().cloned()
}

/// compare vm_if mac addr with frame mac addr
pub fn vm_if_cmp_mac(vm_id: usize, frame: &[u8]) -> bool {
    let vm_if = VM_IF_LIST[vm_id].lock();
    for (i, &b) in vm_if.mac.iter().enumerate() {
        if b != frame[i] {
            return false;
        }
    }
    true
}

pub fn vm_if_set_ivc_arg(vm_id: usize, ivc_arg: usize) {
    let mut vm_if = VM_IF_LIST[vm_id].lock();
    vm_if.ivc_arg = ivc_arg;
}

pub fn vm_if_ivc_arg(vm_id: usize) -> usize {
    let vm_if = VM_IF_LIST[vm_id].lock();
    vm_if.ivc_arg
}

pub fn vm_if_set_ivc_arg_ptr(vm_id: usize, ivc_arg_ptr: usize) {
    let mut vm_if = VM_IF_LIST[vm_id].lock();
    vm_if.ivc_arg_ptr = ivc_arg_ptr;
}

pub fn vm_if_ivc_arg_ptr(vm_id: usize) -> usize {
    let vm_if = VM_IF_LIST[vm_id].lock();
    vm_if.ivc_arg_ptr
}
// End vm interface func implementation

#[derive(Clone, Copy, Default)]
pub enum VmState {
    #[default]
    VmInv = 0,
    VmPending = 1,
    VmActive = 2,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum VmType {
    #[default]
    VmTOs = 0,
    VmTBma = 1,
}

impl From<usize> for VmType {
    fn from(value: usize) -> Self {
        match value {
            0 => Self::VmTOs,
            1 => Self::VmTBma,
            _ => panic!("Unknown VmType value: {}", value),
        }
    }
}

pub struct VmInterface {
    pub master_cpu_id: Once<usize>,
    pub state: VmState,
    pub vm_type: VmType,
    pub mac: [u8; 6],
    pub ivc_arg: usize,
    pub ivc_arg_ptr: usize,
}

impl VmInterface {
    const fn default() -> VmInterface {
        VmInterface {
            master_cpu_id: Once::new(),
            state: VmState::VmPending,
            vm_type: VmType::VmTBma,
            mac: [0; 6],
            ivc_arg: 0,
            ivc_arg_ptr: 0,
        }
    }

    fn reset(&mut self) {
        self.master_cpu_id = Once::new();
        self.state = VmState::VmPending;
        self.vm_type = VmType::VmTBma;
        self.mac = [0; 6];
        self.ivc_arg = 0;
        self.ivc_arg_ptr = 0;
    }
}

#[derive(Clone, Copy)]
pub struct VmPa {
    pub pa_start: usize,
    pub pa_length: usize,
    pub offset: isize,
}

impl VmPa {
    pub fn default() -> VmPa {
        VmPa {
            pa_start: 0,
            pa_length: 0,
            offset: 0,
        }
    }
}

/// Vm interrupt controller type
#[derive(Clone, Copy, Default, Debug)]
pub enum IntCtrlType {
    #[default]
    Emulated,
    Passthrough,
}

#[derive(Clone)]
pub struct Vm {
    pub inner: Arc<Mutex<VmInner>>,
    // innner_const: VmInnerConst,
    // inner_mut: Mutex<VmInnerMut>,
}

// struct VmInnerConst {
//     id: usize,
//     config: Option<VmConfigEntry>,
//     dtb: Option<usize>,
//     // Memory config
//     mem_region_num: usize,
//     pa_region: Vec<VmPa>,
//     entry_point: usize,
//     // Vcpu config
//     has_master: bool,
//     vcpu_list: Vec<Vcpu>,
//     cpu_num: usize,
//     ncpu: usize,
//     // Interrupt config
//     intc_dev_id: usize,
//     sint_bitmap: Option<BitMap<BitAlloc256>>,
//     // Emul devs config
//     emu_devs: Vec<EmuDevs>,
// }

// struct VmInnerMut {
//     pt: Option<PageTable>,
//     ready: bool,
//     iommu_ctx_id: Option<usize>,
// }

// impl VmInnerConst {
//     fn new(id: usize, config: Option<VmConfigEntry>, vm: Weak<Vm>) -> VmInnerConst {
//         VmInnerConst {
//             id,
//             config,
//             dtb: None,
//             mem_region_num: 0,
//             pa_region: Vec::new(),
//             entry_point: 0,
//             has_master: false,
//             vcpu_list: Vec::new(),
//             cpu_num: 0,
//             ncpu: 0,
//             intc_dev_id: 0,
//             int_bitmap: Some(BitAlloc4K::default()),
//             emu_devs: Vec::new(),
//         }
//     }
// }

// fn cal_phys_id_list(config: &VmConfigEntry) -> Vec<usize> {
//     let mut phys_id_list = Vec::new();
//     let cpu_num = config.cpu_num();
//     let cpu_master = config.cpu_master();
//     let cpu_allocated_bitmap = config.cpu_allocated_bitmap();
//     for i in 0..cpu_num {
//         if (cpu_allocated_bitmap & (1 << i)) != 0 {
//             if i == cpu_master {
//                 phys_id_list.insert(0, i);
//             } else {
//                 phys_id_list.push(i);
//             }
//         }
//     }
//     phys_id_list
// }

impl Vm {
    pub fn inner(&self) -> Arc<Mutex<VmInner>> {
        self.inner.clone()
    }

    pub fn default() -> Vm {
        Vm {
            inner: Arc::new(Mutex::new(VmInner::default())),
        }
    }

    pub fn new(id: usize) -> Vm {
        Vm {
            inner: Arc::new(Mutex::new(VmInner::new(id))),
        }
    }

    pub fn set_iommu_ctx_id(&self, id: usize) {
        let mut vm_inner = self.inner.lock();
        vm_inner.iommu_ctx_id = Some(id);
    }

    pub fn iommu_ctx_id(&self) -> usize {
        let vm_inner = self.inner.lock();
        match vm_inner.iommu_ctx_id {
            None => {
                panic!("vm {} do not have iommu context bank", vm_inner.id);
            }
            Some(id) => id,
        }
    }

    pub fn med_blk_id(&self) -> usize {
        let vm_inner = self.inner.lock();
        match vm_inner.config.as_ref().unwrap().mediated_block_index() {
            None => {
                panic!("vm {} do not have mediated blk", vm_inner.id);
            }
            Some(idx) => idx,
        }
    }

    pub fn dtb(&self) -> Option<*mut fdt::myctypes::c_void> {
        let vm_inner = self.inner.lock();
        vm_inner.dtb.map(|x| x as *mut fdt::myctypes::c_void)
    }

    pub fn set_dtb(&self, val: *mut fdt::myctypes::c_void) {
        let mut vm_inner = self.inner.lock();
        vm_inner.dtb = Some(val as usize);
    }

    pub fn vcpu(&self, index: usize) -> Option<Vcpu> {
        let vm_inner = self.inner.lock();
        match vm_inner.vcpu_list.get(index).cloned() {
            Some(vcpu) => {
                assert_eq!(index, vcpu.id());
                Some(vcpu)
            }
            None => {
                error!(
                    "vcpu idx {} is to large than vcpu_list len {}",
                    index,
                    vm_inner.vcpu_list.len()
                );
                None
            }
        }
    }

    pub fn push_vcpu(&self, vcpu: Vcpu) {
        let mut vm_inner = self.inner.lock();
        if vcpu.id() >= vm_inner.vcpu_list.len() {
            vm_inner.vcpu_list.push(vcpu);
        } else {
            trace!("VM[{}] insert VCPU {}", vm_inner.id, vcpu.id());
            vm_inner.vcpu_list.insert(vcpu.id(), vcpu);
        }
    }

    // avoid circular references
    pub fn clear_list(&self) {
        let mut vm_inner = self.inner.lock();
        vm_inner.emu_devs.clear();
        vm_inner.vcpu_list.clear();
    }

    pub fn select_vcpu2assign(&self, cpu_id: usize) -> Option<Vcpu> {
        let cfg_master = self.config().cpu_master();
        let cfg_cpu_num = self.config().cpu_num();
        let cfg_cpu_allocate_bitmap = self.config().cpu_allocated_bitmap();
        // make sure that vcpu assign is executed sequentially, otherwise
        // the PCPUs may found that vm.cpu_num() == 0 at the same time and
        // if cfg_master is not setted, they will not set master vcpu for VM
        let mut vm_inner = self.inner.lock();
        if (cfg_cpu_allocate_bitmap & (1 << cpu_id)) != 0 && vm_inner.cpu_num < cfg_cpu_num {
            // vm.vcpu(0) must be the VM's master vcpu
            let trgt_id = if let Some(master) = cfg_master {
                if cpu_id == master || (!vm_inner.has_master && vm_inner.cpu_num == cfg_cpu_num - 1) {
                    0
                } else if vm_inner.has_master {
                    cfg_cpu_num - vm_inner.cpu_num
                } else {
                    // if master vcpu is not assigned, retain id 0 for it
                    cfg_cpu_num - vm_inner.cpu_num - 1
                }
            } else {
                0
            };
            match vm_inner.vcpu_list.get(trgt_id).cloned() {
                None => None,
                Some(vcpu) => {
                    if vcpu.id() == 0 {
                        vm_if_set_cpu_id(vm_inner.id, cpu_id);
                        vm_inner.has_master = true;
                    }
                    vm_inner.cpu_num += 1;
                    vm_inner.ncpu |= 1 << cpu_id;
                    Some(vcpu)
                }
            }
        } else {
            None
        }
    }

    pub fn set_entry_point(&self, entry_point: usize) {
        let mut vm_inner = self.inner.lock();
        vm_inner.entry_point = entry_point;
    }

    pub fn set_emu_devs(&self, idx: usize, emu: EmuDevs) {
        let mut vm_inner = self.inner.lock();
        if idx < vm_inner.emu_devs.len() {
            if let EmuDevs::None = vm_inner.emu_devs[idx] {
                // println!("set_emu_devs: cover a None emu dev");
                vm_inner.emu_devs[idx] = emu;
                return;
            } else {
                panic!("set_emu_devs: set an exsit emu dev");
            }
        }
        vm_inner.emu_devs.resize(idx, EmuDevs::None);
        vm_inner.emu_devs.push(emu);
    }

    pub fn set_intc_dev_id(&self, intc_dev_id: usize) {
        let mut vm_inner = self.inner.lock();
        vm_inner.intc_dev_id = intc_dev_id;
    }

    pub fn set_int_bit_map(&self, int_id: usize) {
        let mut vm_inner = self.inner.lock();
        vm_inner.int_bitmap.as_mut().unwrap().set(int_id);
    }

    pub fn set_config_entry(&self, config: Option<VmConfigEntry>) {
        let mut vm_inner = self.inner.lock();
        vm_inner.config = config;
    }

    pub fn intc_dev_id(&self) -> usize {
        let vm_inner = self.inner.lock();
        vm_inner.intc_dev_id
    }

    pub fn pt_map_range(&self, ipa: usize, len: usize, pa: usize, pte: usize, map_block: bool) {
        let vm_inner = self.inner.lock();
        match &vm_inner.pt {
            Some(pt) => pt.pt_map_range(ipa, len, pa, pte, map_block),
            None => {
                panic!("Vm::pt_map_range: vm{} pt is empty", vm_inner.id);
            }
        }
    }

    pub fn pt_unmap_range(&self, ipa: usize, len: usize, map_block: bool) {
        let vm_inner = self.inner.lock();
        match &vm_inner.pt {
            Some(pt) => pt.pt_unmap_range(ipa, len, map_block),
            None => {
                panic!("Vm::pt_umnmap_range: vm{} pt is empty", vm_inner.id);
            }
        }
    }

    // ap: access permission
    pub fn pt_set_access_permission(&self, ipa: usize, ap: usize) -> (usize, usize) {
        let vm_inner = self.inner.lock();
        match &vm_inner.pt {
            Some(pt) => pt.access_permission(ipa, PAGE_SIZE, ap),
            None => {
                panic!("pt_set_access_permission: vm{} pt is empty", vm_inner.id);
            }
        }
    }

    pub fn set_pt(&self, pt_dir_frame: PageFrame) {
        let mut vm_inner = self.inner.lock();
        vm_inner.pt = Some(PageTable::new(pt_dir_frame))
    }

    pub fn pt_dir(&self) -> usize {
        let vm_inner = self.inner.lock();
        match &vm_inner.pt {
            Some(pt) => pt.base_pa(),
            None => {
                panic!("Vm::pt_dir: vm{} pt is empty", vm_inner.id);
            }
        }
    }

    pub fn cpu_num(&self) -> usize {
        let vm_inner = self.inner.lock();
        vm_inner.cpu_num
    }

    pub fn id(&self) -> usize {
        let vm_inner = self.inner.lock();
        vm_inner.id
    }

    pub fn config(&self) -> VmConfigEntry {
        let vm_inner = self.inner.lock();
        match &vm_inner.config {
            None => {
                panic!("VM[{}] do not have vm config entry", vm_inner.id);
            }
            Some(config) => config.clone(),
        }
    }

    pub fn add_region(&self, region: VmPa) {
        let mut vm_inner = self.inner.lock();
        vm_inner.pa_region.push(region);
    }

    pub fn region_num(&self) -> usize {
        let vm_inner = self.inner.lock();
        vm_inner.pa_region.len()
    }

    pub fn pa_start(&self, idx: usize) -> usize {
        let vm_inner = self.inner.lock();
        vm_inner.pa_region[idx].pa_start
    }

    pub fn pa_length(&self, idx: usize) -> usize {
        let vm_inner = self.inner.lock();
        vm_inner.pa_region[idx].pa_length
    }

    pub fn pa_offset(&self, idx: usize) -> usize {
        let vm_inner = self.inner.lock();
        vm_inner.pa_region[idx].offset as usize
    }

    pub fn set_mem_region_num(&self, mem_region_num: usize) {
        let mut vm_inner = self.inner.lock();
        vm_inner.mem_region_num = mem_region_num;
    }

    pub fn mem_region_num(&self) -> usize {
        let vm_inner = self.inner.lock();
        vm_inner.mem_region_num
    }

    pub fn vgic(&self) -> Arc<Vgic> {
        let vm_inner = self.inner.lock();
        match &vm_inner.emu_devs[vm_inner.intc_dev_id] {
            EmuDevs::Vgic(vgic) => vgic.clone(),
            _ => {
                panic!("vm{} cannot find vgic", vm_inner.id);
            }
        }
    }

    pub fn has_vgic(&self) -> bool {
        let vm_inner = self.inner.lock();
        if vm_inner.intc_dev_id >= vm_inner.emu_devs.len() {
            return false;
        }
        matches!(&vm_inner.emu_devs[vm_inner.intc_dev_id], EmuDevs::Vgic(_))
    }

    pub fn emu_dev(&self, dev_id: usize) -> EmuDevs {
        let vm_inner = self.inner.lock();
        vm_inner.emu_devs[dev_id].clone()
    }

    pub fn emu_net_dev(&self, id: usize) -> EmuDevs {
        let vm_inner = self.inner.lock();
        let mut dev_num = 0;

        for i in 0..vm_inner.emu_devs.len() {
            if let EmuDevs::VirtioNet(_) = vm_inner.emu_devs[i] {
                if dev_num == id {
                    return vm_inner.emu_devs[i].clone();
                }
                dev_num += 1;
            }
        }
        EmuDevs::None
    }

    pub fn emu_blk_dev(&self) -> EmuDevs {
        for emu in &self.inner.lock().emu_devs {
            if let EmuDevs::VirtioBlk(_) = emu {
                return emu.clone();
            }
        }
        EmuDevs::None
    }

    // Get console dev by ipa.
    pub fn emu_console_dev(&self, ipa: usize) -> EmuDevs {
        for (idx, emu_dev_cfg) in self.config().emulated_device_list().iter().enumerate() {
            if emu_dev_cfg.base_ipa == ipa {
                return self.inner.lock().emu_devs[idx].clone();
            }
        }
        EmuDevs::None
    }

    pub fn ncpu(&self) -> usize {
        let vm_inner = self.inner.lock();
        vm_inner.ncpu
    }

    pub fn has_interrupt(&self, int_id: usize) -> bool {
        let mut vm_inner = self.inner.lock();
        vm_inner.int_bitmap.as_mut().unwrap().get(int_id) != 0
    }

    pub fn emu_has_interrupt(&self, int_id: usize) -> bool {
        for emu_dev in self.config().emulated_device_list() {
            if int_id == emu_dev.irq_id {
                return true;
            }
        }
        false
    }

    pub fn vcpuid_to_vcpu(&self, vcpuid: usize) -> Option<Vcpu> {
        let inner = self.inner.lock();
        inner.vcpu_list.iter().find(|vcpu| vcpu.id() == vcpuid).cloned()
    }

    pub fn vcpuid_to_pcpuid(&self, vcpuid: usize) -> Result<usize, ()> {
        let vm_inner = self.inner.lock();
        if vcpuid < vm_inner.cpu_num {
            let vcpu = vm_inner.vcpu_list[vcpuid].clone();
            drop(vm_inner);
            Ok(vcpu.phys_id())
        } else {
            Err(())
        }
    }

    pub fn pcpuid_to_vcpuid(&self, pcpuid: usize) -> Result<usize, ()> {
        let vm_inner = self.inner.lock();
        for vcpuid in 0..vm_inner.cpu_num {
            if vm_inner.vcpu_list[vcpuid].phys_id() == pcpuid {
                return Ok(vcpuid);
            }
        }
        Err(())
    }

    pub fn vcpu_to_pcpu_mask(&self, mask: usize, len: usize) -> usize {
        let mut pmask = 0;
        for i in 0..len {
            let shift = self.vcpuid_to_pcpuid(i);
            if mask & (1 << i) != 0 && shift.is_ok() {
                pmask |= 1 << shift.unwrap();
            }
        }
        pmask
    }

    pub fn pcpu_to_vcpu_mask(&self, mask: usize, len: usize) -> usize {
        let mut pmask = 0;
        for i in 0..len {
            let shift = self.pcpuid_to_vcpuid(i);
            if mask & (1 << i) != 0 && shift.is_ok() {
                pmask |= 1 << shift.unwrap();
            }
        }
        pmask
    }

    pub fn show_pagetable(&self, ipa: usize) {
        let vm_inner = self.inner.lock();
        vm_inner.pt.as_ref().unwrap().show_pt(ipa);
    }

    pub fn ready(&self) -> bool {
        let vm_inner = self.inner.lock();
        vm_inner.ready
    }

    pub fn set_ready(&self, ready: bool) {
        let mut vm_inner = self.inner.lock();
        vm_inner.ready = ready;
    }

    pub fn get_vcpu_by_mpidr(&self, mpdir: usize) -> Option<Vcpu> {
        let inner = self.inner.lock();
        let cpuid = if (mpdir >> 8) & 0xff != 0 {
            if cfg!(feature = "rk3588") {
                mpdir >> 8
            } else {
                4 + (mpdir & 0xff)
            }
        } else {
            mpdir & 0xff
        };
        inner.vcpu_list.iter().find(|vcpu| vcpu.id() == cpuid).cloned()
    }
}

#[repr(align(4096))]
pub struct VmInner {
    pub id: usize,
    pub ready: bool,
    pub config: Option<VmConfigEntry>,
    pub dtb: Option<usize>,
    // memory config
    pub pt: Option<PageTable>,
    pub mem_region_num: usize,
    pub pa_region: Vec<VmPa>, // Option<[VmPa; VM_MEM_REGION_MAX]>,

    // image config
    pub entry_point: usize,

    // vcpu config
    pub has_master: bool,
    pub vcpu_list: Vec<Vcpu>,
    pub cpu_num: usize,
    pub ncpu: usize,

    // interrupt
    pub intc_dev_id: usize,
    pub int_bitmap: Option<BitMap<BitAlloc256>>,

    // iommu
    pub iommu_ctx_id: Option<usize>,

    // emul devs
    pub emu_devs: Vec<EmuDevs>,
}

impl VmInner {
    pub const fn default() -> VmInner {
        VmInner {
            id: 0,
            ready: false,
            config: None,
            dtb: None,
            pt: None,
            mem_region_num: 0,
            pa_region: Vec::new(),
            entry_point: 0,

            has_master: false,
            vcpu_list: Vec::new(),
            cpu_num: 0,
            ncpu: 0,

            intc_dev_id: 0,
            int_bitmap: Some(BitAlloc4K::default()),

            iommu_ctx_id: None,
            emu_devs: Vec::new(),
        }
    }

    pub fn new(id: usize) -> VmInner {
        VmInner {
            id,
            ready: false,
            config: None,
            dtb: None,
            pt: None,
            mem_region_num: 0,
            pa_region: Vec::new(),
            entry_point: 0,

            has_master: false,
            vcpu_list: Vec::new(),
            cpu_num: 0,
            ncpu: 0,

            intc_dev_id: 0,
            int_bitmap: Some(BitAlloc4K::default()),
            iommu_ctx_id: None,
            emu_devs: Vec::new(),
        }
    }
}

pub static VM_LIST: Mutex<Vec<Vm>> = Mutex::new(Vec::new());

pub fn push_vm(id: usize) -> Result<(), ()> {
    let mut vm_list = VM_LIST.lock();
    if vm_list.iter().any(|x| x.id() == id) {
        error!("push_vm: vm {} already exists", id);
        Err(())
    } else {
        vm_list.push(Vm::new(id));
        Ok(())
    }
}

pub fn remove_vm(id: usize) -> Vm {
    let mut vm_list = VM_LIST.lock();
    match vm_list.iter().position(|x| x.id() == id) {
        None => {
            panic!("VM[{}] not exist in VM LIST", id);
        }
        Some(idx) => vm_list.remove(idx),
    }
}

pub fn vm(id: usize) -> Option<Vm> {
    let vm_list = VM_LIST.lock();
    vm_list.iter().find(|&x| x.id() == id).cloned()
}

pub fn vm_list_size() -> usize {
    let vm_list = VM_LIST.lock();
    vm_list.len()
}

pub fn vm_ipa2pa(vm: Vm, ipa: usize) -> usize {
    if ipa == 0 {
        error!("vm_ipa2pa: VM {} access invalid ipa {:x}", vm.id(), ipa);
        return 0;
    }

    for i in 0..vm.mem_region_num() {
        if in_range(
            (ipa as isize - vm.pa_offset(i) as isize) as usize,
            vm.pa_start(i),
            vm.pa_length(i),
        ) {
            return (ipa as isize - vm.pa_offset(i) as isize) as usize;
        }
    }

    error!("vm_ipa2pa: VM {} access invalid ipa {:x}", vm.id(), ipa);
    0
}

pub fn vm_pa2ipa(vm: Vm, pa: usize) -> usize {
    if pa == 0 {
        error!("vm_pa2ipa: VM {} access invalid pa {:x}", vm.id(), pa);
        return 0;
    }

    for i in 0..vm.mem_region_num() {
        if in_range(pa, vm.pa_start(i), vm.pa_length(i)) {
            return (pa as isize + vm.pa_offset(i) as isize) as usize;
        }
    }

    error!("vm_pa2ipa: VM {} access invalid pa {:x}", vm.id(), pa);
    0
}

pub fn pa2ipa(pa_region: &[VmPa], pa: usize) -> usize {
    if pa == 0 {
        error!("pa2ipa: access invalid pa {:x}", pa);
        return 0;
    }

    for region in pa_region.iter() {
        if in_range(pa, region.pa_start, region.pa_length) {
            return (pa as isize + region.offset) as usize;
        }
    }

    error!("pa2ipa: access invalid pa {:x}", pa);
    0
}

pub fn ipa2pa(pa_region: &[VmPa], ipa: usize) -> usize {
    if ipa == 0 {
        // println!("ipa2pa: access invalid ipa {:x}", ipa);
        return 0;
    }

    for region in pa_region.iter() {
        if in_range(
            (ipa as isize - region.offset) as usize,
            region.pa_start,
            region.pa_length,
        ) {
            return (ipa as isize - region.offset) as usize;
        }
    }

    0
}

pub fn cpuid2mpidr(cpuid: usize) -> usize {
    if cfg!(feature = "rk3588") {
        0x81000000 | (cpuid << 8)
    } else {
        // qemu
        if cpuid < 4 {
            cpuid | (1 << 31)
        } else {
            0x100 | (cpuid - 4) | (1 << 31)
        }
    }
}
