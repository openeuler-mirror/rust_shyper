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
        PassthroughRegion { ipa: 0xfc000000, pa: 0xfc000000, length: 0xfe600000 - 0xfc000000, dev_property: true },
        PassthroughRegion { ipa: 0xfe680000 + 0x100000, pa: 0xfe680000 + 0x100000, length: 0x10000_0000 - (0xfe680000 + 0x100000), dev_property: true },
        // PassthroughRegion { ipa: 0xfd000000, pa: 0xfd000000, length: 0x00100000, dev_property: true },
        // PassthroughRegion { ipa: 0xfd900000, pa: 0xfd900000, length: 0x00200000, dev_property: true },
        // PassthroughRegion { ipa: 0xfe300000, pa: 0xfe300000, length: 0x00100000, dev_property: true },
        // PassthroughRegion { ipa: 0xfe800000, pa: 0xfe800000, length: 0x00200000, dev_property: true },
        // PassthroughRegion { ipa: 0xfee20000, pa: 0xfee20000, length: 0x00020000, dev_property: true },
        // //serial@feb50000
        // PassthroughRegion { ipa: 0xfeb50000, pa: 0xfeb50000, length: 0x00020000, dev_property: true },
        // PassthroughRegion { ipa: 0xfd800000, pa: 0xfd800000, length: 0x00100000, dev_property: true },
        // PassthroughRegion { ipa: 0xfd700000, pa: 0xfd700000, length: 0x00100000, dev_property: true },
        // //timer@feae0000
        // PassthroughRegion { ipa: 0xfeae0000, pa: 0xfeae0000, length: 0x00100000, dev_property: true },
        // //debug@fd104000
        // PassthroughRegion { ipa: 0xfd104000, pa: 0xfd104000, length: 0x00100000, dev_property: true },
        // //dma-controller@fea10000
        // PassthroughRegion { ipa: 0xfea10000, pa: 0xfea10000, length: 0x4000, dev_property: true },
        // //dma-controller@fea30000
        // PassthroughRegion { ipa: 0xfea30000, pa: 0xfea30000, length: 0x4000, dev_property: true },
        // //dma-controller@fed10000
        // PassthroughRegion { ipa: 0xfed10000, pa: 0xfed10000, length: 0x4000, dev_property: true },
        // //gpio@fec20000
        // PassthroughRegion { ipa: 0xfec20000, pa: 0xfec20000, length: 0x100, dev_property: true },
        // //gpio@fec30000
        // PassthroughRegion { ipa: 0xfec30000, pa: 0xfec30000, length: 0x100, dev_property: true },
        // //gpio@fec40000
        // PassthroughRegion { ipa: 0xfec40000, pa: 0xfec40000, length: 0x100, dev_property: true },
        // //gpio@fec50000
        // PassthroughRegion { ipa: 0xfec50000, pa: 0xfec50000, length: 0x100, dev_property: true },
        // //syscon@fd5f0000
        // PassthroughRegion { ipa: 0xfd5f0000, pa: 0xfd5f0000, length: 0x10000, dev_property: true },
        // //syscon@fd5ec000
        // PassthroughRegion { ipa: 0xfd5ec000, pa: 0xfd5ec000, length: 0x4000, dev_property: true },
        // //syscon@fd5d8000
        // PassthroughRegion { ipa: 0xfd5d8000, pa: 0xfd5d8000, length: 0x4000, dev_property: true },
        // //syscon@fd5dc000
        // PassthroughRegion { ipa: 0xfd5dc000, pa: 0xfd5dc000, length: 0x4000, dev_property: true },
        // //syscon@fd5e0000
        // PassthroughRegion { ipa: 0xfd5e0000, pa: 0xfd5e0000, length: 0x100, dev_property: true },
        // //syscon@fd5e8000
        // PassthroughRegion { ipa: 0xfd5e8000, pa: 0xfd5e8000, length: 0x4000, dev_property: true },
        // //syscon@fd5b0000
        // PassthroughRegion { ipa: 0xfd5b0000, pa: 0xfd5b0000, length: 0x1000, dev_property: true },
        // //syscon@fd5b4000
        // PassthroughRegion { ipa: 0xfd5b4000, pa: 0xfd5b4000, length: 0x1000, dev_property: true },
        // //syscon@fd5bc000
        // PassthroughRegion { ipa: 0xfd5bc000, pa: 0xfd5bc000, length: 0x100, dev_property: true },
        // //syscon@fd5c4000
        // PassthroughRegion { ipa: 0xfd5c4000, pa: 0xfd5c4000, length: 0x100, dev_property: true },
        // //syscon@fd5c8000
        // PassthroughRegion { ipa: 0xfd5c8000, pa: 0xfd5c8000, length: 0x4000, dev_property: true },
        // //syscon@fd5d0000
        // PassthroughRegion { ipa: 0xfd5d0000, pa: 0xfd5d0000, length: 0x4000, dev_property: true },
        // //qos@fdf35000 - fe060000
        // PassthroughRegion { ipa: 0xfdf35000, pa: 0xfdf35000, length: 0xfe060000 - 0xfdf35000, dev_property: true },
        // //otp@fecc0000
        // PassthroughRegion { ipa: 0xfecc0000, pa: 0xfecc0000, length: 0x400, dev_property: true },
        // //pwm@febe0000 - febe0030
        // PassthroughRegion { ipa: 0xfebe0000, pa: 0xfebe0000, length: 0x40, dev_property: true },
        // //pwm@febf0000 - febf0030
        // PassthroughRegion { ipa: 0xfebf0000, pa: 0xfebf0000, length: 0x40, dev_property: true },
        // //iommu@fdb90480
        // PassthroughRegion { ipa: 0xfdb90480, pa: 0xfdb90480, length: 0x40, dev_property: true },
        // //iommu@fdc38700
        // PassthroughRegion { ipa: 0xfdc38700, pa: 0xfdc38700, length: 0x80, dev_property: true },        
        // //iommu@fdc38700
        // PassthroughRegion { ipa: 0xfdc48700, pa: 0xfdc48700, length: 0x80, dev_property: true },
        // //iommu@fdca0000
        // PassthroughRegion { ipa: 0xfdca0000, pa: 0xfdca0000, length: 0x600, dev_property: true },
        // //iommu@fdbdf000
        // PassthroughRegion { ipa: 0xfdbdf000, pa: 0xfdbdf000, length: 0x80, dev_property: true },
        // //iommu@fdbef000
        // PassthroughRegion { ipa: 0xfdbef000, pa: 0xfdbef000, length: 0x80, dev_property: true },
        // //rkvenc-core@fdbd0000
        // PassthroughRegion { ipa: 0xfdbd0000, pa: 0xfdbd0000, length: 0x6000, dev_property: true },    
        // //rkvenc-core@fdbe0000
        // PassthroughRegion { ipa: 0xfdbe0000, pa: 0xfdbe0000, length: 0x6000, dev_property: true },
        // //av1d@fdc70000
        // PassthroughRegion { ipa: 0xfdc70000, pa: 0xfdc70000, length: 0x3000, dev_property: true },
        // //rkvdec-core@fdc38000
        // PassthroughRegion { ipa: 0xfdc38000, pa: 0xfdc38000, length: 0x800, dev_property: true },
        // PassthroughRegion { ipa: 0xfdc80000, pa: 0xfdc80000, length: 0x400, dev_property: true },
        // PassthroughRegion { ipa: 0xfdc90000, pa: 0xfdc90000, length: 0x400, dev_property: true },

        PassthroughRegion { ipa: 0x00000000, pa: 0x00000000, length: 0x00200000, dev_property: true },
    ];
    // 146 is UART_INT
    pt_dev_config.irqs = (16..496).collect();
    // pt_dev_config.irqs = vec![
    //     16,17,18,19,20,21,22,23,24,25,26,INTERRUPT_IRQ_GUEST_TIMER,28,29,30,31,32, 33, 34, 35, 
    //     36, 37, 38, 39, 40,41,42,43,44,45,46,47,48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 
    //     60, 61,62, 63, 64, 65, 67, 68,69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 
    //     83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98,102, 103, 104, 105, 107,
    //     108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125,
    //     126, 127, 128, 129, 130, 131, 132, 133, 134, 135, 136, 137, 138, 139, Platform::UART_0_INT, 151, 152,
    //     153, 154, 155, 156, 157, 158, 159, 160, 161, 162, 163, 164, 165, 166, 167, 168, 169, 170, 171, 172,
    //     173, 174, 175, 176, 177, 178, 179, 180, 181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 191, 192, 
    //     193, 194, 195, 196, 197, 198, 199, 200, 201, 202, 203, 204, 205, 206, 207, 208, 209, 210, 211,
    //     212, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223, 224, 225, 226, 227, 228, 229, 230, 231,
    //     232, 233, 234, 235, 236, 237, 238, 239, 240, 241, 242, 255, 256, 257, 258, 259, 260, 261, 262, 263,
    //     264, 265, 266, 267, 268, 269, 270, 271, 272, 273, 274, 275, 276, 277, 278, 279, 280, 281, 282, 283,
    //     284, 285, 286, 287, 288, 289, 290, 291, 292, 293, 294, 295, 296, 297 ,298, 299, 300, 301, 302, 303,
    //     304, 305, 306, 307, 308, 309, 310, 311, 312, 313, 314, 315, 316, 317, 318, 319, 320, 322, 328, 329, 
    //     330, 331, 332, 333, 334, 335, 336, 337 ,338, 339 ,340, 352, 353, 366, 0xf04
    // ];
    pt_dev_config.streams_ids = vec![];

    // vm0 vm_region
    let vm_region = vec![
        VmRegion {
            ipa_start:  0x09400000,
            length:     0xe6c00000,
        },
    ];

    // vm0 config
    let mvm_config_entry = VmConfigEntry {
        id: 0,
        name: Some(String::from("RK3588")),
        os_type: VmType::VmTOs,
        cmdline:
        //String::from("storagemedia=emmc androidboot.storagemedia=emmc androidboot.mode=normal  dsi-0=2 storagenode=/mmc@fe2e0000 androidboot.verifiedbootstate=orange ro rootwait earlycon=uart8250,mmio32,0xfeb50000 console=ttyFIQ0 irqchip.gicv3_pseudo_nmi=0 root=PARTLABEL=rootfs rootfstype=ext4 overlayroot=device:dev=PARTLABEL=userdata,fstype=ext4,mkfs=1 coherent_pool=1m systemd.gpt_auto=0 cgroup_enable=memory swapaccount=1 net.ifnames=0"),
        String::from("storagenode=/mmc@fe2e0000 earlycon=uart8250,mmio32,0xfeb50000 console=ttyFIQ0 root=/dev/mmcblk0p6 rootfstype=ext4 rootwait rw default_hugepagesz=32M hugepagesz=32M hugepages=4"),
        // String::from("earlycon=uart8250,mmio32,0xfeb50000 root=/dev/sda1 rw audit=0 rootwait console=ttyFIQ0"),
        //String::from("emmc androidboot.storagemedia=emmc androidboot.mode=normal  dsi-0=2 storagenode=/mmc@fe2e0000 earlycon=uart8250,mmio32,0xfeb50000 console=ttyFIQ0 root=/dev/nfs nfsroot=192.168.106.153:/tftp/rootfs,proto=tcp rw ip=192.168.106.143:192.168.106.153:192.168.106.1:255.255.255.0::eth0:off default_hugepagesz=32M hugepagesz=32M hugepages=4"),
        // String::from("earlycon=uart8250,mmio32,0xfeb50000 console=ttyFIQ0 irqchip.gicv3_pseudo_nmi=0 root=PARTLABEL=rootfs rootfstype=ext4 rw rootwait overlayroot=device:dev=PARTLABEL=userdata,fstype=ext4,mkfs=1 coherent_pool=1m systemd.gpt_auto=0 cgroup_enable=memory swapaccount=1 net.ifnames=0\0"),
        // String::from("storagemedia=emmc androidboot.storagemedia=emmc androidboot.mode=normal  dsi-0=2 storagenode=/mmc@fe2e0000 androidboot.verifiedbootstate=orange ro rootwait earlycon=uart8250,mmio32,0xfeb50000 console=ttyFIQ0 irqchip.gicv3_pseudo_nmi=0 root=PARTLABEL=rootfs rootfstype=ext4 overlayroot=device:dev=PARTLABEL=userdata,fstype=ext4,mkfs=1 coherent_pool=1m systemd.gpt_auto=0 cgroup_enable=memory swapaccount=1 net.ifnames=0\0"),
        image: Arc::new(Mutex::new(VmImageConfig {
            kernel_img_name: Some("Linux-5.10"),
            kernel_load_ipa: 0x20080000,
            kernel_load_pa: 0,
            kernel_entry_point: 0x20080000,
            device_tree_load_ipa: 0x10000000,
            ramdisk_load_ipa: 0,
            mediated_block_index: None,
        })),
        // image: Arc::new(Mutex::new(VmImageConfig {
        //     kernel_img_name: Some("unishper"),
        //     kernel_load_ipa: 0x40080000,
        //     kernel_load_pa: 0,
        //     kernel_entry_point: 0x40080000,
        //     device_tree_load_ipa: 0,
        //     ramdisk_load_ipa: 0,
        //     mediated_block_index: None,
        // })),
        memory: Arc::new(Mutex::new(VmMemoryConfig {
            region: vm_region,
        })),
        cpu: Arc::new(Mutex::new(VmCpuConfig {
            num: 1,
            allocate_bitmap: 0b1,
            master: 0,
        })),
        vm_emu_dev_confg: Arc::new(Mutex::new(VmEmulatedDeviceConfigList { emu_dev_list: emu_dev_config })),
        vm_pt_dev_confg: Arc::new(Mutex::new(pt_dev_config)),
        vm_dtb_devs: Arc::new(Mutex::new(VMDtbDevConfigList::default())),
    };
    let _ = vm_cfg_add_vm_entry(mvm_config_entry);
}
