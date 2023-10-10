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

use crate::board::{Platform, PlatOperation};
use crate::config::vm_cfg_add_vm_entry;
use crate::device::EmuDeviceType;
use crate::kernel::{HVC_IRQ, INTERRUPT_IRQ_GUEST_TIMER, VmType};

use super::{
    PassthroughRegion, vm_cfg_set_config_name, VmConfigEntry, VmCpuConfig, VMDtbDevConfigList, VmEmulatedDeviceConfig,
    VmEmulatedDeviceConfigList, VmImageConfig, VmMemoryConfig, VmPassthroughDeviceConfig, VmRegion,
};

#[rustfmt::skip]
pub fn mvm_config_init() {
    info!("mvm_config_init() init config for VM0, which is manager VM");

    vm_cfg_set_config_name("rk3588-default");

    // vm0 emu
    let emu_dev_config = vec![
        VmEmulatedDeviceConfig {
            name: Some(String::from("interrupt-controller@fe600000")),
            base_ipa: Platform::GICD_BASE,
            length: 0x10000,
            irq_id: 25,
            cfg_list: Vec::new(),
            emu_type: EmuDeviceType::EmuDeviceTGicd,
            mediated: false,
        },
        VmEmulatedDeviceConfig {
            name: Some(String::from("GICR@0xfe680000")),
            base_ipa: Platform::GICR_BASE,
            length: 0x100000,
            irq_id: 25,
            cfg_list: Vec::new(),
            emu_type: EmuDeviceType::EmuDeviceTGICR,
            mediated: false,
        },
        VmEmulatedDeviceConfig {
            name: Some(String::from("ICC_SRE_ADDR")),
            base_ipa: Platform::ICC_SRE_ADDR,
            length: 0,
            irq_id: 25,
            cfg_list: Vec::new(),
            emu_type: EmuDeviceType::EmuDeviceTICCSRE,
            mediated: false,
        },
        VmEmulatedDeviceConfig {
            name: Some(String::from("ICC_SGIR_ADDR")),
            base_ipa: Platform::ICC_SGIR_ADDR,
            length: 0 ,
            irq_id: 25,
            cfg_list: Vec::new(),
            emu_type: EmuDeviceType::EmuDeviceTSGIR,
            mediated: false,
        },
        // VmEmulatedDeviceConfig {
        //     name: Some(String::from("virtio-blk@feb60000")),
        //     base_ipa: 0xfeb60000,
        //     length: 0x1000,
        //     irq_id: 48,
        //     cfg_list: vec![0, 8192000],
        //     emu_type: EmuDeviceType::EmuDeviceTVirtioBlk,
        //     mediated: false,
        // },
        VmEmulatedDeviceConfig {
            name: Some(String::from("virtio_net@f0000000")),
            base_ipa: 0xf000_0000,
            length: 0x1000,
            irq_id: 45,
            cfg_list: vec![0x74, 0x56, 0xaa, 0x0f, 0x47, 0xd0],
            emu_type: EmuDeviceType::EmuDeviceTVirtioNet,
            mediated: false,
        },
        VmEmulatedDeviceConfig {
            name: Some(String::from("virtio_console@f0001000")),
            base_ipa: 0xf000_1000,
            length: 0x1000,
            irq_id: 46,
            cfg_list: vec![1, 0xf0140000],
            emu_type: EmuDeviceType::EmuDeviceTVirtioConsole,
            mediated: false,
        },
        VmEmulatedDeviceConfig {
            name: Some(String::from("virtio_console@f0002000")),
            base_ipa: 0xf000_2000,
            length: 0x1000,
            irq_id: 47,
            cfg_list: vec![2, 0xf001_1000],
            emu_type: EmuDeviceType::EmuDeviceTVirtioConsole,
            mediated: false,
        },
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
        // PassthroughRegion { ipa: 0xf0000000, pa: 0xf0000000, length: 0x10000000, dev_property: true },
        //all, exclude gic
        PassthroughRegion { ipa: 0xfb000000, pa: 0xfb000000, length: 0x100_0000, dev_property: true },
        PassthroughRegion { ipa: 0xfc000000, pa: 0xfc000000, length: 0xfe600000 - 0xfc000000, dev_property: true },
        PassthroughRegion { ipa: 0xfe680000 + 0x100000, pa: 0xfe680000 + 0x100000, length: 0xfeb40000 - (0xfe680000 + 0x100000), dev_property: true },
        PassthroughRegion { ipa: 0xfebc0000, pa: 0xfebc0000, length: 0x10000_0000 - 0xfebc0000, dev_property: true },
        // serial@feb5000——ttyFIQ
        PassthroughRegion { ipa: 0xfeb50000, pa: 0xfeb50000, length: 0x100, dev_property: true },
        // serial@feba000——ttyS7
        PassthroughRegion { ipa: 0xfeba0000, pa: 0xfeba0000, length: 0x100, dev_property: true },
        // dev_property: false means non-cachable here.
        // See 'vmm_init_passthrough_device'.
        PassthroughRegion { ipa: 0x0, pa: 0x0, length: 0x9400000, dev_property: false },
    ];
    pt_dev_config.irqs = vec![
        20,  //fsc_interrupt_int_n
        23,  //ARM-PMU
        26,  //arch-timer
        INTERRUPT_IRQ_GUEST_TIMER,  //timer
        30,  //ptimer
        105, //dmc 32 + 0x49
        118, //fea10000.dma-controller
        119, //fea10000.dma-controller
        120, //fea30000.dma-controller
        121, //fea30000.dma-controller
        122, //fed10000.dma-controller
        123, //fed10000.dma-controller
        124, //fb000000.gpu
        125, //fb000000.gpu
        126, //fb000000.gpu
        127, //fdc38100.rkvdec-core
        128, //fdc38700.iommu
        129, //fdc48100.rkvdec-core
        130, //fdc48700.iommu
        131, //fdbdf000.iommu
        132, //fdbdf000.iommu
        133, //fdbd0000.rkvenc-core
        134, //fdbef000.iommu
        135, //fdbef000.iommu
        136, //fdbe0000.rkvenc-core
        140, //av1d-master
        141, //fdca0000.iommu
        142, //fdab9000.iommu, fdab0000.npu
        143, //fdab9000.iommu, fdab0000.npu
        144, //fdab9000.iommu, fdab0000.npu
        146, //fdb60f00.iommu, rga3_core0
        145, //fdce0800.iommu
        147, //fdb60f00.iommu, rga3_core1
        148, //fdb60f00.iommu, rga2
        149, //fdbb0800.iommu, fdbb0000.iep
        150, //fdb50800.iommu
        151, //fdb50400.vdpu
        153, //fdba0800.iommu
        154, //fdba0000.jpege-core
        155, //fdba4800.iommu
        156, //fdba4000.jpege-core
        157, //fdba8800.iommu
        158, //fdba8000.jpege-core
        159, //fdbac800.iommu
        160, //fdbac000.jpege-core
        161, //fdb90000.jpegd
        162, //fdb90480.iommu
        179, //rockchip-mipi-csi2
        180, //rockchip-mipi-csi2
        187, //rkcifhw
        188, //fdd97e00.iommu, fdd90000.vop
        192, //dw-hdmi-qp-hpd
        193, //fde50000.dp
        199, //fde20000.dsi
        201, //fde80000.hdmi
        203, //fde80000.hdmi
        204, //fde80000.hdmi
        212, //i2s
        217, //i2s
        235, //dw-mci
        237, //mmc0
        247, //ehci_hcd:usb1
        248, //ehci_hcd:usb3
        250, //ehci_hcd:usb2
        251, //ehci_hcd:usb4
        252, //dwc3
        254, //xhci-hcd:usb5
        265, //eth0
        266, //eth0
        321, //rk_timer
        347, //feaf0000.watchdog
        349, //fd880000.i2c
        351, //feaa0000.i2c
        352, //feab0000.i2c
        353, //feac0000.i2c
        355, //fec80000.i2c
        356, //fec90000.i2c
        359, //feb10000.spi
        360, //feb20000.spi
        365, //debug
        370, //ttyS7
        384,
        423, //rockchip_usb2phy
        424, //rockchip_usb2phy
        425, //rockchip_usb2phy
        429, //rockchip_thermal
        430, //fec10000.saradc
        455, //debug-signal
    ];
    pt_dev_config.streams_ids = vec![];

    // vm0 vm_region
    let vm_region = vec![
        VmRegion {
            ipa_start:  0x09400000,
            length:     0x76c00000,
        },
    ];

    // vm0 config
    let mvm_config_entry = VmConfigEntry {
        id: 0,
        name: Some(String::from("RK3588")),
        os_type: VmType::VmTOs,
        cmdline:
        //String::from("storagemedia=emmc androidboot.storagemedia=emmc androidboot.mode=normal  dsi-0=2 storagenode=/mmc@fe2e0000 androidboot.verifiedbootstate=orange ro rootwait earlycon=uart8250,mmio32,0xfeb50000 console=ttyFIQ0 irqchip.gicv3_pseudo_nmi=0 root=PARTLABEL=rootfs rootfstype=ext4 overlayroot=device:dev=PARTLABEL=userdata,fstype=ext4,mkfs=1 coherent_pool=1m systemd.gpt_auto=0 cgroup_enable=memory swapaccount=1 net.ifnames=0"),
        String::from("earlycon=uart8250,mmio32,0xfeb50000 console=ttyFIQ,9600n8 irqchip.gicv3_pseudo_nmi=0 root=/dev/mmcblk0p6 rootfstype=ext4 rootwait rw default_hugepagesz=32M hugepagesz=32M hugepages=4"),
        // String::from("earlycon=uart8250,mmio32,0xfeb50000 root=/dev/sda1 rootfstype=ext4 rw rootwait console=ttyFIQ0"),
        // String::from("emmc androidboot.storagemedia=emmc androidboot.mode=normal  dsi-0=2 storagenode=/mmc@fe2e0000 earlycon=uart8250,mmio32,0xfeb50000 console=ttyFIQ0 root=/dev/nfs nfsroot=192.168.106.153:/tftp/rootfs,proto=tcp rw ip=192.168.106.143:192.168.106.153:192.168.106.1:255.255.255.0::eth0:off default_hugepagesz=32M hugepagesz=32M hugepages=4"),
        // String::from("earlycon=uart8250,mmio32,0xfeb50000 console=ttyFIQ0 irqchip.gicv3_pseudo_nmi=0 root=PARTLABEL=rootfs rootfstype=ext4 rw rootwait overlayroot=device:dev=PARTLABEL=userdata,fstype=ext4,mkfs=1 coherent_pool=1m systemd.gpt_auto=0 cgroup_enable=memory swapaccount=1 net.ifnames=0\0"),
        // String::from("storagemedia=emmc androidboot.storagemedia=emmc androidboot.mode=normal  dsi-0=2 storagenode=/mmc@fe2e0000 androidboot.verifiedbootstate=orange rw rootwait earlycon=uart8250,mmio32,0xfeb50000 console=ttyFIQ0 irqchip.gicv3_pseudo_nmi=0 root=PARTLABEL=rootfs rootfstype=ext4 overlayroot=device:dev=PARTLABEL=userdata,fstype=ext4,mkfs=1 coherent_pool=1m systemd.gpt_auto=0 cgroup_enable=memory swapaccount=1 net.ifnames=0\0"),
        image: Arc::new(Mutex::new(VmImageConfig {
            kernel_img_name: Some("Linux-5.10"),
            kernel_load_ipa: 0x10080000,
            kernel_load_pa: 0,
            kernel_entry_point: 0x10080000,
            device_tree_load_ipa: 0x10000000,
            ramdisk_load_ipa: 0,
            mediated_block_index: None,
        })),
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
