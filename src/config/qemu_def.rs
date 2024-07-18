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

use crate::board::*;
use crate::device::EmuDeviceType;
use crate::kernel::{HVC_IRQ, VmType};

use super::{
    VmConfigEntry, VmCpuConfig, VmEmulatedDeviceConfig, VmImageConfig, VmMemoryConfig, VmPassthroughDeviceConfig,
    VmRegion, vm_cfg_set_config_name, PassthroughRegion, vm_cfg_add_vm_entry, VmEmulatedDeviceConfigList,
};

/// Initializes the configuration for the manager VM (VM0).
#[rustfmt::skip]
pub fn mvm_config_init() {
    // Set the configuration name for VM0
    vm_cfg_set_config_name("qemu-default");

    // vm0 emu
    let emu_dev_config = vec![
        VmEmulatedDeviceConfig {
            name: String::from("vgicd"),
            base_ipa: Platform::GICD_BASE,
            #[cfg(not(feature="gicv3"))]
            length: 0x1000,
            #[cfg(feature="gicv3")]
            length: 0x10000,
            irq_id: 25,
            cfg_list: Vec::new(),
            emu_type: EmuDeviceType::EmuDeviceTGicd,
            mediated: false,
        },
        #[cfg(feature="gicv3")]
        VmEmulatedDeviceConfig {
            name: String::from("vgicr"),
            base_ipa: Platform::GICR_BASE,
            length: 0xf60000,
            irq_id: 25,
            cfg_list: Vec::new(),
            emu_type: EmuDeviceType::EmuDeviceTGICR,
            mediated: false,
        },
        VmEmulatedDeviceConfig {
            name: String::from("virtio-nic0"),
            base_ipa: 0xa001000,
            length: 0x1000,
            irq_id: 32 + 0x11,
            cfg_list: vec![0x74, 0x56, 0xaa, 0x0f, 0x47, 0xd0],
            emu_type: EmuDeviceType::EmuDeviceTVirtioNet,
            mediated: false,
        },
        VmEmulatedDeviceConfig {
            name: String::from("virtio_console@a002000"),
            base_ipa: 0xa002000,
            length: 0x1000,
            irq_id: 32 + 0x12,
            cfg_list: vec![1, 0xa002000],
            emu_type: EmuDeviceType::EmuDeviceTVirtioConsole,
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
    let pt_dev_config: VmPassthroughDeviceConfig = VmPassthroughDeviceConfig{
    regions: vec![
        PassthroughRegion { ipa: Platform::UART_0_ADDR, pa: Platform::UART_0_ADDR, length: 0x1000, dev_property: true },
        #[cfg(not(feature = "gicv3"))]
        PassthroughRegion { ipa: Platform::GICC_BASE, pa: Platform::GICV_BASE, length: 0x2000, dev_property: true },
        #[cfg(feature = "gicv3")]
        PassthroughRegion { ipa: 0x8080000, pa: 0x8080000, length: 0x20000, dev_property: true }, //pass-through gicv3-its
        // pass-througn virtio blk/net
        PassthroughRegion { ipa: 0x0a003000, pa: 0x0a003000, length: 0x1000, dev_property: true },
    ],
    irqs: vec![33,27, 72, 73,74,75,76,77,78,79],
    streams_ids: vec![]
    };

    // vm0 vm_region
    let vm_region = vec![
        VmRegion {
            ipa_start: 0x50000000,
            length: 0x80000000,
        }
    ];

    // vm0 config
    let mvm_config_entry =VmConfigEntry {
        id: 0,
        name: String::from("supervisor"),
        os_type: VmType::VmTOs,
        cmdline: String::from("earlycon console=ttyAMA0 root=/dev/vda rw audit=0 default_hugepagesz=32M hugepagesz=32M hugepages=4\0"),
        image: VmImageConfig {
            kernel_img_name: Some("Image"),
            kernel_load_ipa: 0x80080000,
            kernel_entry_point: 0x80080000,
            device_tree_load_ipa: 0x80000000,
            ramdisk_load_ipa: 0,
            mediated_block_index: None,
        },
        cpu: VmCpuConfig {
            num: 1,
            allocate_bitmap: 0b0001,
            master: None,
        },
        memory: VmMemoryConfig {
            region: vm_region,
        },
        vm_emu_dev_confg: VmEmulatedDeviceConfigList { emu_dev_list: emu_dev_config },
        vm_pt_dev_confg: pt_dev_config,
        ..Default::default()
    };
    let _ = vm_cfg_add_vm_entry(mvm_config_entry);
}
