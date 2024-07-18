// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use alloc::string::String;
use alloc::vec::Vec;
use crate::device::EmuDeviceType;
use crate::kernel::{HVC_IRQ, VmType};

use super::{
    VmConfigEntry, VmCpuConfig, VmEmulatedDeviceConfig, VmImageConfig, VmMemoryConfig, VmPassthroughDeviceConfig,
    VmRegion, vm_cfg_set_config_name, PassthroughRegion, vm_cfg_add_vm_entry, VmEmulatedDeviceConfigList,
    VMDtbDevConfigList,
};

/// Initializes the configuration for the manager VM (VM0).
#[rustfmt::skip]
pub fn mvm_config_init() {
    // Set the configuration name for VM0
    vm_cfg_set_config_name("qemu-default");

    // vm0 emu
    let emu_dev_config = vec![
        // Defines the start address and length of the PLIC device
        VmEmulatedDeviceConfig {
            name: String::from("plic"),
            base_ipa: 0xc000000,
            length: 0x600000,
            irq_id: 0,
            cfg_list: Vec::new(),
            emu_type: EmuDeviceType::EmuDeviceTPlic,
            mediated: false,
        },
        // hvc2
        VmEmulatedDeviceConfig {
            name: String::from("virtio_console@40001000"),
            base_ipa: 0x4000_1000,
            length: 0x1000,
            irq_id: 46,
            cfg_list: vec![1, 0x40001000],
            emu_type: EmuDeviceType::EmuDeviceTVirtioConsole,
            mediated: false,
        },
        // hvc1
        VmEmulatedDeviceConfig {
            name: String::from("virtio_console@40002000"),
            base_ipa: 0x4000_2000,
            length: 0x1000,
            irq_id: 47,
            cfg_list: vec![2, 0x4000_2000], // Address of the peer vm and the peer virtio-console
            emu_type: EmuDeviceType::EmuDeviceTVirtioConsole,
            mediated: false,
        },
        // virtual eth0
        VmEmulatedDeviceConfig {
            name: String::from("virtio_net@40003000"),
            base_ipa: 0x40003000,
            length: 0x1000,
            irq_id: 48,
            cfg_list: vec![0x74, 0x56, 0xaa, 0x0f, 0x47, 0xd0],
            emu_type: EmuDeviceType::EmuDeviceTVirtioNet,
            mediated: false,
        },
        VmEmulatedDeviceConfig {
            name: String::from("shyper"),
            base_ipa: 0,
            length: 0,
            irq_id: HVC_IRQ,
            cfg_list: Vec::new(),
            emu_type: EmuDeviceType::EmuDeviceTShyper,
            mediated: false,
        }
    ];

    // vm0 passthrough
    let pt_dev_config: VmPassthroughDeviceConfig = VmPassthroughDeviceConfig {
        regions: vec![
            // pass-through virtio blk
            PassthroughRegion { ipa: 0x10001000, pa: 0x10001000, length: 0x1000, dev_property: true },
            PassthroughRegion { ipa: 0x10002000, pa: 0x10002000, length: 0x1000, dev_property: true },
            PassthroughRegion { ipa: 0x10003000, pa: 0x10003000, length: 0x1000, dev_property: true },
            PassthroughRegion { ipa: 0x10004000, pa: 0x10004000, length: 0x1000, dev_property: true },
            PassthroughRegion { ipa: 0x10005000, pa: 0x10005000, length: 0x1000, dev_property: true },
            PassthroughRegion { ipa: 0x10006000, pa: 0x10006000, length: 0x1000, dev_property: true },
            PassthroughRegion { ipa: 0x10007000, pa: 0x10007000, length: 0x1000, dev_property: true },
            PassthroughRegion { ipa: 0x10008000, pa: 0x10008000, length: 0x1000, dev_property: true },
            // Serial Device
            PassthroughRegion { ipa: 0x10000000, pa: 0x10000000, length: 0x1000, dev_property: true },
            // RTC
            PassthroughRegion { ipa: 0x101000, pa: 0x101000, length: 0x1000, dev_property: true },
        ],
        irqs: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11,],
        streams_ids: vec![]
    };

    // vm0 vm_region
    let vm_region = vec![
        VmRegion {
            ipa_start: 0x90000000,
            length: 0x100000000,
        }
    ];

    // vm0 config
    let mvm_config_entry = VmConfigEntry {
        id: 0,
        name: String::from("supervisor"),
        os_type: VmType::VmTOs,
        cmdline: String::from("earlycon=sbi console=ttyS0 root=/dev/vda rw audit=0 default_hugepagesz=2M hugepagesz=2M hugepages=10\0"),
        image: VmImageConfig {
            kernel_img_name: Some("Image"),
            kernel_load_ipa: 0x90000000,
            kernel_entry_point: 0x90000000,
            // device_tree_filename: Some("qemu1.bin"),
            // Note: In Linux, Device Tree should be placed above linux kernel, otherwise it will be ignored
            // Linux Print: OF: fdt: Ignoring memory range 0x90000000 - 0x90200000
            device_tree_load_ipa: 0x180000000,
            // ramdisk_filename: Some("initrd.gz"),
            // ramdisk_load_ipa: 0x53000000,
            ramdisk_load_ipa: 0,
            mediated_block_index: None,
        },
        cpu: VmCpuConfig {
            num: 1,
            allocate_bitmap: 0b0001,
            master: Some(0),
        },
        memory: VmMemoryConfig {
            region: vm_region,
        },
        vm_emu_dev_confg: VmEmulatedDeviceConfigList { emu_dev_list: emu_dev_config },
        vm_pt_dev_confg: pt_dev_config,
        vm_dtb_devs: VMDtbDevConfigList::default(),
        ..Default::default()
    };
    let _ = vm_cfg_add_vm_entry(mvm_config_entry);
}
