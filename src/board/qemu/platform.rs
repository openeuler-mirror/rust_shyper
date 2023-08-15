// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

// TODO: move these core name to device
use crate::arch::GicDesc;
use crate::arch::SmmuDesc;
use crate::board::{
    PlatOperation, Platform, PlatCpuCoreConfig, ClusterDesc, ArchDesc, PlatCpuConfig, PlatformConfig, PlatMemoryConfig,
    PlatMemRegion,
};
use crate::board::SchedRule::RoundRobin;
use crate::device::ARM_CORTEX_A57;
use crate::driver::{read, write};

pub struct QemuPlatform;

impl PlatOperation for QemuPlatform {
    const UART_0_ADDR: usize = 0x9000000;
    const UART_1_ADDR: usize = 0x9100000;
    const UART_2_ADDR: usize = 0x9110000;

    const UART_0_INT: usize = 32 + 0x70;
    const UART_1_INT: usize = 32 + 0x72;

    const HYPERVISOR_UART_BASE: usize = Self::UART_0_ADDR;

    const GICD_BASE: usize = 0x08000000;
    const GICC_BASE: usize = 0x08010000;
    const GICH_BASE: usize = 0x08030000;
    const GICV_BASE: usize = 0x08040000;
    #[cfg(feature = "gicv3")]
    const GICR_BASE: usize = 0x080a0000;

    const SHARE_MEM_BASE: usize = 0x7_0000_0000;

    const DISK_PARTITION_0_START: usize = 0;
    const DISK_PARTITION_1_START: usize = 2097152;
    const DISK_PARTITION_2_START: usize = 10289152;

    const DISK_PARTITION_TOTAL_SIZE: usize = 18481152;
    const DISK_PARTITION_0_SIZE: usize = 524288;
    const DISK_PARTITION_1_SIZE: usize = 8192000;
    const DISK_PARTITION_2_SIZE: usize = 8192000;

    fn cpuid_to_cpuif(cpuid: usize) -> usize {
        // PLAT_DESC.cpu_desc.core_list[cpuid].mpidr
        cpuid
    }

    fn cpuif_to_cpuid(cpuif: usize) -> usize {
        cpuif
    }

    fn blk_init() {
        println!("Platform block driver init ok");
        crate::driver::virtio_blk_init();
    }

    fn blk_read(sector: usize, count: usize, buf: usize) {
        read(sector, count, buf);
    }

    fn blk_write(sector: usize, count: usize, buf: usize) {
        write(sector, count, buf);
    }
}

pub static PLAT_DESC: PlatformConfig = PlatformConfig {
    cpu_desc: PlatCpuConfig {
        num: 1,
        core_list: &[
            PlatCpuCoreConfig {
                name: ARM_CORTEX_A57,
                mpidr: 0,
                sched: RoundRobin,
            },
            // PlatCpuCoreConfig {
            //     name: ARM_CORTEX_A57,
            //     mpidr: 1,
            //     sched: RoundRobin,
            // },
            // PlatCpuCoreConfig {
            //     name: ARM_CORTEX_A57,
            //     mpidr: 2,
            //     sched: RoundRobin,
            // },
            // PlatCpuCoreConfig {
            //     name: ARM_CORTEX_A57,
            //     mpidr: 3,
            //     sched: RoundRobin,
            // },
        ],
    },
    mem_desc: PlatMemoryConfig {
        regions: &[
            // reserve 0x48000000 ~ 0x48100000 for QEMU dtb
            PlatMemRegion {
                base: 0x40000000,
                size: 0x08000000,
            },
            PlatMemRegion {
                base: 0x50000000,
                size: 0x1f0000000,
            },
        ],
        base: 0x40000000,
    },
    arch_desc: ArchDesc {
        gic_desc: GicDesc {
            gicd_addr: Platform::GICD_BASE,
            gicc_addr: Platform::GICC_BASE,
            gich_addr: Platform::GICH_BASE,
            gicv_addr: Platform::GICV_BASE,
            #[cfg(feature = "gicv3")]
            gicr_addr: Platform::GICR_BASE,
            maintenance_int_id: 25,
        },
        smmu_desc: SmmuDesc {
            base: 0,
            interrupt_id: 0,
            global_mask: 0,
        },
        cluster_desc: ClusterDesc { num: 1, core_num: &[4] },
    },
};
