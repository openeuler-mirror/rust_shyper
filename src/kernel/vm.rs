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
use spin::{Mutex, Once};

#[cfg(target_arch = "aarch64")]
use crate::arch::Vgic;
#[cfg(target_arch = "riscv64")]
use crate::arch::VPlic;
use crate::arch::{PAGE_SIZE, emu_intc_init, PageTable};
use crate::config::VmConfigEntry;
use crate::device::{EmuDev, emu_virtio_mmio_init};
use crate::kernel::{shyper_init, emu_iommu_init};
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

pub fn vm_if_get_cpu_id(vm_id: usize) -> Option<usize> {
    let vm_if = VM_IF_LIST[vm_id].lock();
    vm_if.master_cpu_id.get().cloned()
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
    VmTBma = 1, // Bare Metal Application
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
            vm_type: VmType::VmTOs,
            mac: [0; 6],
            ivc_arg: 0,
            ivc_arg_ptr: 0,
        }
    }

    fn reset(&mut self) {
        self.master_cpu_id = Once::new();
        self.state = VmState::VmPending;
        self.vm_type = VmType::VmTOs;
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

pub struct Vm {
    inner_const: VmInnerConst,
    inner_mut: Mutex<VmInnerMut>,
}

struct VmInnerConst {
    id: usize,
    config: VmConfigEntry,
    // Vcpu config
    vcpu_list: Vec<Vcpu>,
    // Interrupt config
    intc_type: IntCtrlType,
    int_bitmap: BitAlloc4K,
    #[cfg(target_arch = "aarch64")]
    arch_intc_dev: Option<Arc<Vgic>>,
    #[cfg(target_arch = "riscv64")]
    arch_intc_dev: Option<Arc<VPlic>>,
    // Emul devs config
    emu_devs: Vec<Arc<dyn EmuDev>>,
}

struct VmInnerMut {
    pt: Option<PageTable>,
    // Memory config
    mem_region_num: usize,
    pa_region: Vec<VmPa>,
    iommu_ctx_id: Option<usize>,
}

impl VmInnerConst {
    fn new(id: usize, config: VmConfigEntry, vm: Weak<Vm>) -> VmInnerConst {
        let phys_id_list = cal_phys_id_list(&config);
        info!("VM[{}] phys_id_list {:?}", id, phys_id_list);

        // cpu total number count must equal to the number of cpu in config
        assert_eq!(phys_id_list.len(), config.cpu_num());
        // set master cpu id
        vm_if_set_cpu_id(id, *phys_id_list.first().unwrap());

        let mut vcpu_list = Vec::with_capacity(config.cpu_num());

        for (vcpu_id, phys_id) in phys_id_list.into_iter().enumerate() {
            vcpu_list.push(Vcpu::new(vm.clone(), vcpu_id, phys_id));
        }
        let mut this = Self {
            id,
            config,
            vcpu_list,
            intc_type: IntCtrlType::Emulated,
            arch_intc_dev: None,
            int_bitmap: BitAlloc4K::default(),
            emu_devs: Vec::new(),
        };
        this.init_devices(vm);
        this
    }

    fn init_devices(&mut self, vm: Weak<Vm>) -> bool {
        use crate::device::EmuDeviceType::*;
        for (idx, emu_cfg) in self.config.emulated_device_list().iter().enumerate() {
            let dev = match emu_cfg.emu_type {
                #[cfg(target_arch = "aarch64")]
                EmuDeviceTGicd => {
                    self.intc_type = IntCtrlType::Emulated;
                    emu_intc_init(emu_cfg, &self.vcpu_list).map(|vgic| {
                        self.arch_intc_dev = vgic.clone().into_any_arc().downcast::<Vgic>().ok();
                        vgic
                    })
                }
                #[cfg(target_arch = "riscv64")]
                EmuDeviceTPlic => {
                    self.intc_type = IntCtrlType::Emulated;
                    emu_intc_init(emu_cfg, &self.vcpu_list).map(|vplic| {
                        self.arch_intc_dev = vplic.clone().into_any_arc().downcast::<VPlic>().ok();
                        vplic
                    })
                }
                #[cfg(feature = "gicv3")]
                EmuDeviceTGICR => {
                    if let Some(vgic) = &self.arch_intc_dev {
                        crate::arch::emu_vgicr_init(emu_cfg, vgic.clone())
                    } else {
                        panic!("init_device: vgic not init");
                    }
                }
                #[cfg(target_arch = "aarch64")]
                EmuDeviceTGPPT => {
                    self.intc_type = IntCtrlType::Passthrough;
                    crate::arch::partial_passthrough_intc_init(emu_cfg)
                }
                EmuDeviceTVirtioConsole | EmuDeviceTVirtioNet | EmuDeviceTVirtioBlk => {
                    emu_virtio_mmio_init(vm.clone(), emu_cfg)
                }
                EmuDeviceTIOMMU => emu_iommu_init(emu_cfg),
                EmuDeviceTShyper => shyper_init(vm.clone(), emu_cfg.base_ipa, emu_cfg.length),
                _ => {
                    panic!("init_device: unknown emu dev type {:?}", emu_cfg.emu_type);
                }
            };
            // Then add the dev to the emu_devs list
            if let Ok(emu_dev) = dev {
                if self.emu_devs.iter().any(|dev| {
                    emu_dev.address_range().contains(&dev.address_range().start)
                        || dev.address_range().contains(&emu_dev.address_range().start)
                }) {
                    panic!(
                        "duplicated emul address region: prev address {:x?}",
                        emu_dev.address_range(),
                    );
                } else {
                    self.emu_devs.push(emu_dev);
                }
            } else {
                panic!("init_device: failed to init emu dev");
            }
            // Then init int_bitmap
            if emu_cfg.irq_id != 0 {
                self.int_bitmap.set(emu_cfg.irq_id);
            }
            info!(
                "VM {} registers emulated device: id=<{}>, name=\"{:?}\", ipa=<{:#x}>",
                self.id, idx, emu_cfg.emu_type, emu_cfg.base_ipa
            );
        }
        // Passthrough irqs
        for irq in self.config.passthrough_device_irqs() {
            self.int_bitmap.set(*irq);
        }
        true
    }
}

fn cal_phys_id_list(config: &VmConfigEntry) -> Vec<usize> {
    // generate the vcpu physical id list
    let mut phys_id_list = vec![];
    let mut cfg_cpu_allocate_bitmap = config.cpu_allocated_bitmap();
    if let Some(cpu_master) = config.cpu_master() {
        if cfg_cpu_allocate_bitmap & (1 << cpu_master) != 0 {
            phys_id_list.push(cpu_master);
        }
        let mut phys_id = 0;
        while cfg_cpu_allocate_bitmap != 0 {
            if cfg_cpu_allocate_bitmap & 1 != 0 && phys_id != cpu_master {
                phys_id_list.push(phys_id);
            }
            phys_id += 1;
            cfg_cpu_allocate_bitmap >>= 1;
        }
    } else {
        let mut phys_id = 0;
        while cfg_cpu_allocate_bitmap != 0 {
            if cfg_cpu_allocate_bitmap & 1 != 0 {
                phys_id_list.push(phys_id);
            }
            phys_id += 1;
            cfg_cpu_allocate_bitmap >>= 1;
        }
    }
    phys_id_list
}

impl VmInnerMut {
    fn new() -> Self {
        VmInnerMut {
            pt: None,
            mem_region_num: 0,
            pa_region: Vec::new(),
            iommu_ctx_id: None,
        }
    }
}

impl Vm {
    pub fn new(id: usize, config: VmConfigEntry) -> Arc<Self> {
        let this = Arc::new_cyclic(|weak| Vm {
            inner_const: VmInnerConst::new(id, config, weak.clone()),
            inner_mut: Mutex::new(VmInnerMut::new()),
        });
        for vcpu in this.vcpu_list() {
            vcpu.init(this.config());
        }
        this.init_intc_mode(this.inner_const.intc_type);
        this
    }

    pub fn set_iommu_ctx_id(&self, id: usize) {
        let mut vm_inner = self.inner_mut.lock();
        vm_inner.iommu_ctx_id = Some(id);
    }

    pub fn iommu_ctx_id(&self) -> usize {
        let vm_inner = self.inner_mut.lock();
        match vm_inner.iommu_ctx_id {
            None => {
                panic!("vm {} do not have iommu context bank", self.id());
            }
            Some(id) => id,
        }
    }

    pub fn med_blk_id(&self) -> usize {
        match self.config().mediated_block_index() {
            None => {
                panic!("vm {} do not have mediated blk", self.id());
            }
            Some(idx) => idx,
        }
    }

    #[inline]
    pub fn vcpu_list(&self) -> &[Vcpu] {
        &self.inner_const.vcpu_list
    }

    #[inline]
    pub fn id(&self) -> usize {
        self.inner_const.id
    }

    pub fn vcpu(&self, index: usize) -> Option<&Vcpu> {
        match self.vcpu_list().get(index) {
            Some(vcpu) => {
                assert_eq!(index, vcpu.id());
                Some(vcpu)
            }
            None => {
                error!(
                    "vcpu idx {} is to large than vcpu_list len {}",
                    index,
                    self.vcpu_list().len()
                );
                None
            }
        }
    }

    pub fn find_emu_dev(&self, ipa: usize) -> Option<Arc<dyn EmuDev>> {
        self.inner_const
            .emu_devs
            .iter()
            .find(|&dev| dev.address_range().contains(&ipa))
            .cloned()
    }

    pub fn pt_map_range(&self, ipa: usize, len: usize, pa: usize, pte: usize, map_block: bool) {
        let vm_inner = self.inner_mut.lock();
        match &vm_inner.pt {
            Some(pt) => pt.pt_map_range(ipa, len, pa, pte, map_block),
            None => {
                panic!("Vm::pt_map_range: vm{} pt is empty", self.id());
            }
        }
    }

    pub fn pt_unmap_range(&self, ipa: usize, len: usize, map_block: bool) {
        let vm_inner = self.inner_mut.lock();
        match &vm_inner.pt {
            Some(pt) => pt.pt_unmap_range(ipa, len, map_block),
            None => {
                panic!("Vm::pt_umnmap_range: vm{} pt is empty", self.id());
            }
        }
    }

    // ap: access permission
    pub fn pt_set_access_permission(&self, ipa: usize, ap: usize) -> (usize, usize) {
        let vm_inner = self.inner_mut.lock();
        match &vm_inner.pt {
            Some(pt) => pt.access_permission(ipa, PAGE_SIZE, ap),
            None => {
                panic!("pt_set_access_permission: vm{} pt is empty", self.id());
            }
        }
    }

    pub fn set_pt(&self, pt_dir_frame: PageFrame) {
        let mut vm_inner = self.inner_mut.lock();
        vm_inner.pt = Some(PageTable::new(pt_dir_frame))
    }

    pub fn pt_dir(&self) -> usize {
        let vm_inner = self.inner_mut.lock();
        match &vm_inner.pt {
            Some(pt) => pt.base_pa(),
            None => {
                panic!("Vm::pt_dir: vm{} pt is empty", self.id());
            }
        }
    }

    pub fn cpu_num(&self) -> usize {
        self.inner_const.config.cpu_num()
    }

    #[inline]
    pub fn config(&self) -> &VmConfigEntry {
        &self.inner_const.config
    }

    pub fn add_region(&self, region: VmPa) {
        let mut vm_inner = self.inner_mut.lock();
        vm_inner.pa_region.push(region);
    }

    pub fn region_num(&self) -> usize {
        let vm_inner = self.inner_mut.lock();
        vm_inner.pa_region.len()
    }

    pub fn pa_start(&self, idx: usize) -> usize {
        let vm_inner = self.inner_mut.lock();
        vm_inner.pa_region[idx].pa_start
    }

    pub fn pa_length(&self, idx: usize) -> usize {
        let vm_inner = self.inner_mut.lock();
        vm_inner.pa_region[idx].pa_length
    }

    pub fn pa_offset(&self, idx: usize) -> usize {
        let vm_inner = self.inner_mut.lock();
        vm_inner.pa_region[idx].offset as usize
    }

    pub fn set_mem_region_num(&self, mem_region_num: usize) {
        let mut vm_inner = self.inner_mut.lock();
        vm_inner.mem_region_num = mem_region_num;
    }

    pub fn mem_region_num(&self) -> usize {
        let vm_inner = self.inner_mut.lock();
        vm_inner.mem_region_num
    }

    #[cfg(target_arch = "aarch64")]
    pub fn vgic(&self) -> &Vgic {
        if let Some(vgic) = self.inner_const.arch_intc_dev.as_ref() {
            return vgic;
        }
        panic!("vm{} cannot find vgic", self.id());
    }

    #[cfg(target_arch = "aarch64")]
    pub fn has_vgic(&self) -> bool {
        self.inner_const.arch_intc_dev.is_some()
    }

    #[cfg(target_arch = "riscv64")]
    pub fn vplic(&self) -> &VPlic {
        if let Some(vplic) = self.inner_const.arch_intc_dev.as_ref() {
            return vplic;
        }
        panic!("vm{} cannot find vgic", self.id());
    }

    #[cfg(target_arch = "riscv64")]
    pub fn has_vplic(&self) -> bool {
        self.inner_const.arch_intc_dev.is_some()
    }

    pub fn ncpu(&self) -> usize {
        self.inner_const.config.cpu_allocated_bitmap() as usize
    }

    // Whether there is a pass-through interrupt int_id
    pub fn has_interrupt(&self, int_id: usize) -> bool {
        self.inner_const.int_bitmap.get(int_id) != 0
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
        self.vcpu_list().iter().find(|vcpu| vcpu.id() == vcpuid).cloned()
    }

    pub fn vcpuid_to_pcpuid(&self, vcpuid: usize) -> Result<usize, ()> {
        self.vcpu_list().get(vcpuid).map(|vcpu| vcpu.phys_id()).ok_or(())
    }

    pub fn pcpuid_to_vcpuid(&self, pcpuid: usize) -> Result<usize, ()> {
        for vcpu in self.vcpu_list() {
            if vcpu.phys_id() == pcpuid {
                return Ok(vcpu.id());
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
        let vm_inner = self.inner_mut.lock();
        vm_inner.pt.as_ref().unwrap().show_pt(ipa);
    }

    pub fn get_vcpu_by_mpidr(&self, mpdir: usize) -> Option<Vcpu> {
        let cpuid = if (mpdir >> 8) & 0xff != 0 {
            if cfg!(feature = "rk3588") {
                mpdir >> 8
            } else {
                4 + (mpdir & 0xff)
            }
        } else {
            mpdir & 0xff
        };
        self.vcpu_list().iter().find(|vcpu| vcpu.id() == cpuid).cloned()
    }

    pub fn ipa2pa(&self, ipa: usize) -> usize {
        if ipa == 0 {
            error!("vm_ipa2pa: VM {} access invalid ipa {:x}", self.id(), ipa);
            return 0;
        }

        for i in 0..self.mem_region_num() {
            if in_range(
                (ipa as isize - self.pa_offset(i) as isize) as usize,
                self.pa_start(i),
                self.pa_length(i),
            ) {
                return (ipa as isize - self.pa_offset(i) as isize) as usize;
            }
        }

        error!("vm_ipa2pa: VM {} access invalid ipa {:x}", self.id(), ipa);
        0
    }
}

static VM_LIST: Mutex<Vec<Arc<Vm>>> = Mutex::new(Vec::new());

#[inline]
pub fn vm_list_walker<F>(mut f: F)
where
    F: FnMut(&Arc<Vm>),
{
    let vm_list = VM_LIST.lock();
    for vm in vm_list.iter() {
        f(vm);
    }
}

pub fn push_vm(id: usize, config: VmConfigEntry) -> Result<Arc<Vm>, ()> {
    let mut vm_list = VM_LIST.lock();
    if vm_list.iter().any(|x| x.id() == id) {
        error!("push_vm: vm {} already exists", id);
        Err(())
    } else {
        let vm = Vm::new(id, config);
        vm_list.push(vm.clone());
        Ok(vm)
    }
}

pub fn remove_vm(id: usize) -> Arc<Vm> {
    let mut vm_list = VM_LIST.lock();
    match vm_list.iter().position(|x| x.id() == id) {
        None => {
            panic!("VM[{}] not exist in VM LIST", id);
        }
        Some(idx) => vm_list.remove(idx),
    }
}

pub fn vm(id: usize) -> Option<Arc<Vm>> {
    let vm_list = VM_LIST.lock();
    vm_list.iter().find(|&x| x.id() == id).cloned()
}

pub fn vm_list_size() -> usize {
    let vm_list = VM_LIST.lock();
    vm_list.len()
}

pub fn vm_ipa2pa(vm: &Vm, ipa: usize) -> usize {
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
