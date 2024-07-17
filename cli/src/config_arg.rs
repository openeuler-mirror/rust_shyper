// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

#[repr(C)]
pub struct VmAddConfigArg {
    pub vm_name_addr: u64,
    pub vm_name_length: u64,
    pub vm_type: u64,
    pub cmd_line_addr: u64,
    pub cmd_line_length: u64,
    pub kernel_load_ipa: u64,
    pub device_tree_load_ipa: u64,
    pub ramdisk_load_ipa: u64,
    pub vm_id_addr: u64,
}

#[repr(C)]
pub struct VmSetCpuConfigArg {
    pub vmid: u64,
    pub num: u32,
    pub allocate_bitmap: u32,
    pub master: i32,
}

#[repr(C)]
pub struct VmMemoryColorBudgetConfigArg {
    pub vmid: u64,
    pub color_num: u64,
    pub color_array_addr: u64,
    pub budget: u32,
}

#[repr(C)]
pub struct VmAddEmulatedDeviceConfigArg {
    pub vmid: u64,
    pub dev_name_addr: u64,
    pub dev_name_length: u64,
    pub base_ipa: u64,
    pub length: u64,
    pub irq_id: u64,
    pub cfg_list_addr: u64,
    pub cfg_list_length: u64,
    pub emu_type: u64,
}

#[repr(C)]
pub struct VmAddMemoryRegionConfigArg {
    pub vmid: u64,
    pub ipa_start: u64,
    pub length: u64,
}

#[repr(C)]
pub struct VmAddPassthroughDeviceRegionConfigArg {
    pub vmid: u64,
    pub base_ipa: u64,
    pub base_pa: u64,
    pub length: u64,
}

#[repr(C)]
pub struct VmAddPassthroughDeviceIrqsConfigArg {
    pub vmid: u64,
    pub irqs_addr: u64,
    pub irqs_length: u64,
}

#[repr(C)]
pub struct VmAddPassthroughDeviceStreamsIdsConfigArg {
    pub vmid: u64,
    pub streams_ids_addr: u64,
    pub streams_ids_length: u64,
}

#[repr(C)]
pub struct VmAddDtbDeviceConfigArg {
    pub vmid: u64,
    pub dev_name_addr: u64,
    pub dev_name_length: u64,
    pub dev_type: u64,
    pub irq_list_addr: u64,
    pub irq_list_length: u64,
    pub addr_region_ipa: u64,
}

#[repr(C)]
pub struct VmLoadKernelImgFileArg {
    pub vmid: u64,
    pub img_size: u64,
    pub cache_ipa: u64,
    pub load_offset: u64,
    pub load_size: u64,
}

#[repr(C)]
pub struct VmKernelImageInfo {
    pub vm_id: u64,
    pub image_name: [u8; 32],
}
