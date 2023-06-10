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
use alloc::sync::Arc;
use alloc::vec::Vec;

use spin::Mutex;

use crate::board::{Platform, PlatOperation, PLAT_DESC};
use crate::config::vm_cfg_add_vm_entry;
use crate::device::EmuDeviceType;
use crate::kernel::{HVC_IRQ, INTERRUPT_IRQ_GUEST_TIMER, VmType};

use super::{
    PassthroughRegion, vm_cfg_set_config_name, VmConfigEntry, VmCpuConfig, VMDtbDevConfigList, VmEmulatedDeviceConfig,
    VmEmulatedDeviceConfigList, VmImageConfig, VmMemoryConfig, VmPassthroughDeviceConfig, VmRegion,
};

#[rustfmt::skip]
pub fn mvm_config_init() {
    println!("mvm_config_init() init config for VM0, which is manager VM");

    vm_cfg_set_config_name("rk3588-default");

    // vm0 emu
    let emu_dev_config = vec![
        VmEmulatedDeviceConfig {
            name: Some(String::from("interrupt-controller@fe600000")),
            base_ipa: Platform::GICD_BASE,
            length: 0x10000,
            irq_id: 0,
            cfg_list: Vec::new(),
            emu_type: EmuDeviceType::EmuDeviceTGicd,
            mediated: false,
        },
        VmEmulatedDeviceConfig {
            name: Some(String::from("GICR@0xfe680000")),
            base_ipa: Platform::GICR_BASE,
            length: 0x20000 * PLAT_DESC.cpu_desc.num,
            irq_id: 0,
            cfg_list: Vec::new(),
            emu_type: EmuDeviceType::EmuDeviceTGICR,
            mediated: false,
        },
        // VmEmulatedDeviceConfig {
        //     name: Some(String::from("virtio_net@a001000")),
        //     base_ipa: 0xa001000,
        //     length: 0x1000,
        //     irq_id: 32 + 0x100,
        //     cfg_list: vec![0x74, 0x56, 0xaa, 0x0f, 0x47, 0xd0],
        //     emu_type: EmuDeviceType::EmuDeviceTVirtioNet,
        //     mediated: false,
        // },
        // VmEmulatedDeviceConfig {
        //     name: Some(String::from("virtio_console@a002000")),
        //     base_ipa: 0xa002000,
        //     length: 0x1000,
        //     irq_id: 32 + 0x101,
        //     cfg_list: vec![1, 0xa002000],
        //     emu_type: EmuDeviceType::EmuDeviceTVirtioConsole,
        //     mediated: false,
        // },
        // VmEmulatedDeviceConfig {
        //     name: Some(String::from("virtio_console@a003000")),
        //     base_ipa: 0xa003000,
        //     length: 0x1000,
        //     irq_id: 32 + 0x102,
        //     cfg_list: vec![2, 0xa002000],
        //     emu_type: EmuDeviceType::EmuDeviceTVirtioConsole,
        //     mediated: false,
        // },
        // VmEmulatedDeviceConfig {
        //     name: Some(String::from("iommu")),
        //     base_ipa: 0x12000000,
        //     length: 0x1000000,
        //     irq_id: 0,
        //     cfg_list: Vec::new(),
        //     emu_type: EmuDeviceType::EmuDeviceTIOMMU,
        //     mediated: false,
        // },
        VmEmulatedDeviceConfig {
            name: Some(String::from("vm_service")),
            base_ipa: 0,
            length: 0,
            irq_id: HVC_IRQ,
            cfg_list: Vec::new(),
            emu_type: EmuDeviceType::EmuDeviceTShyper,
            mediated: false,
        }
    ];

    // vm0 passthrough
    let mut pt_dev_config: VmPassthroughDeviceConfig = VmPassthroughDeviceConfig::default();
    pt_dev_config.regions = vec![
        //all
        PassthroughRegion { ipa: 0xF0000000, pa: 0xF0000000, length: 0x10000000, dev_property: true },
    ];
    // 146 is UART_INT
    pt_dev_config.irqs = vec![
        16,17,18,19,20,21,22,23,24,25,26,INTERRUPT_IRQ_GUEST_TIMER,28,29,30,31,32, 33, 34, 35, 
        36, 37, 38, 39, 40,41,42,43,44,45,46,47,48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 
        60, 61,62, 63, 64, 65, 67, 68,69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 
        83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98,102, 103, 104, 105, 107,
        108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125,
        126, 127, 128, 129, 130, 131, 132, 133, 134, 135, 136, 137, 138, 139, Platform::UART_0_INT, 151, 152,
        153, 154, 155, 156, 157, 158, 159, 165, 166, 167, 168, 173, 174, 175, 176, 177, 178, 179,
        185, 186, 187, 190, 191, 192, 193, 194, 195, 196, 197, 198, 199, 200, 201, 202, 203, 208,
        212, 218, 219, 220, 221, 222, 223, 224, 225, 226, 227, 229, 230, 233, 234, 235, 237, 238,
        242, 255, 256, 295, 297, 315, 322, 328, 329, 330, 331, 352, 353, 366, 0xf04
    ];
    pt_dev_config.streams_ids = vec![];

    // vm0 vm_region
    let vm_region = vec![
        VmRegion {
            ipa_start: 0x10000000,
            length: 0x60000000,
        }
    ];

    // vm0 config
    let mvm_config_entry = VmConfigEntry {
        id: 0,
        name: Some(String::from("RK3588")),
        os_type: VmType::VmTOs,
        cmdline:
        //String::from("storagemedia=emmc androidboot.storagemedia=emmc androidboot.mode=normal  dsi-0=2 storagenode=/mmc@fe2e0000 androidboot.verifiedbootstate=orange ro rootwait earlycon=uart8250,mmio32,0xfeb50000 console=ttyFIQ0 irqchip.gicv3_pseudo_nmi=0 root=PARTLABEL=rootfs rootfstype=ext4 overlayroot=device:dev=PARTLABEL=userdata,fstype=ext4,mkfs=1 coherent_pool=1m systemd.gpt_auto=0 cgroup_enable=memory swapaccount=1 net.ifnames=0"),
        String::from("storagemedia=emmc earlycon=uart8250,mmio32,0xfeb50000 console=ttyFIQ0 root=PARTLABEL=rootfs rw audit=0 rootfstype=ext4 overlayroot=device:dev=PARTLABEL=userdata,fstype=ext4,mkfs=1"),

        image: Arc::new(Mutex::new(VmImageConfig {
            kernel_img_name: Some("Linux-5.10"),
            kernel_load_ipa: 0x10200000,
            kernel_load_pa: 0,
            kernel_entry_point: 0x10200000,
            device_tree_load_ipa: 0x10000000,
            ramdisk_load_ipa: 0,
            mediated_block_index: None,
        })),
        memory: Arc::new(Mutex::new(VmMemoryConfig {
            region: vm_region,
        })),
        cpu: Arc::new(Mutex::new(VmCpuConfig {
            num: 1,
            allocate_bitmap: 0b0001,
            master: 0,
        })),
        vm_emu_dev_confg: Arc::new(Mutex::new(VmEmulatedDeviceConfigList { emu_dev_list: emu_dev_config })),
        vm_pt_dev_confg: Arc::new(Mutex::new(pt_dev_config)),
        vm_dtb_devs: Arc::new(Mutex::new(VMDtbDevConfigList::default())),
    };
    let _ = vm_cfg_add_vm_entry(mvm_config_entry);
}
