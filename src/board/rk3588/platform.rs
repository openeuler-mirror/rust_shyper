// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use crate::arch::GicDesc;
use crate::arch::SmmuDesc;
use crate::arch::sysreg_enc_addr;
use crate::board::{
    PlatOperation, Platform, PlatCpuCoreConfig, ClusterDesc, ArchDesc, PlatCpuConfig, PlatformConfig, PlatMemoryConfig,
    PlatMemRegion,
};
use crate::board::SchedRule::RoundRobin;
use crate::device::ARM_CORTEX_A55;
use crate::device::ARM_CORTEX_A76;
#[allow(unused_imports)]
use crate::device::ARM_NVIDIA_DENVER;

pub struct Rk3588Platform;

impl PlatOperation for Rk3588Platform {
    const UART_0_ADDR: usize = 0xfeb50000;
    const UART_1_ADDR: usize = 0xfeba0000;

    const UART_0_INT: usize = 32 + 0x12d;
    const UART_1_INT: usize = 32 + 0x134;

    const HYPERVISOR_UART_BASE: usize = Self::UART_0_ADDR;

    const GICD_BASE: usize = 0xfe600000;
    const GICC_BASE: usize = 0xfe610000;
    const GICH_BASE: usize = 0xfe620000;
    const GICV_BASE: usize = 0xfe630000;
    const GICR_BASE: usize = 0xfe680000;

    // start sector number (LBA)
    const DISK_PARTITION_0_START: usize = 16384;
    const DISK_PARTITION_1_START: usize = 32768;
    const DISK_PARTITION_2_START: usize = 40960;

    // size in sector (512-byte)
    // pub const DISK_PARTITION_TOTAL_SIZE: usize = 31457280;
    const DISK_PARTITION_0_SIZE: usize = 16384;
    const DISK_PARTITION_1_SIZE: usize = 8192;
    const DISK_PARTITION_2_SIZE: usize = 524288;

    const SHARE_MEM_BASE: usize = 0xd_0000_0000;

    //sysreg
    const ICC_SRE_ADDR: usize = sysreg_enc_addr(3, 0, 12, 12, 5);
    const ICC_SGIR_ADDR: usize = sysreg_enc_addr(3, 0, 12, 11, 5);

    fn cpuid_to_cpuif(cpuid: usize) -> usize {
        PLAT_DESC.cpu_desc.core_list[cpuid].mpidr
    }

    fn cpuif_to_cpuid(cpuif: usize) -> usize {
        cpuif
    }

    fn vmpidr2vcpuid(vmpidr: usize) -> usize {
        (vmpidr >> 8) & 0xff
    }

    fn blk_init() {
        todo!()
    }

    fn blk_read(_sector: usize, _count: usize, _buf: usize) {
        todo!()
    }

    fn blk_write(_sector: usize, _count: usize, _buf: usize) {
        todo!()
    }
}

pub static PLAT_DESC: PlatformConfig = PlatformConfig {
    cpu_desc: PlatCpuConfig {
        num: 8,
        core_list: &[
            PlatCpuCoreConfig {
                //cluster0
                name: ARM_CORTEX_A55,
                mpidr: 0x81000000,
                sched: RoundRobin,
            },
            PlatCpuCoreConfig {
                //cluster0
                name: ARM_CORTEX_A55,
                mpidr: 0x81000100,
                sched: RoundRobin,
            },
            PlatCpuCoreConfig {
                //cluster0
                name: ARM_CORTEX_A55,
                mpidr: 0x81000200,
                sched: RoundRobin,
            },
            PlatCpuCoreConfig {
                //cluster0
                name: ARM_CORTEX_A55,
                mpidr: 0x81000300,
                sched: RoundRobin,
            },
            PlatCpuCoreConfig {
                //cluster1
                name: ARM_CORTEX_A76,
                mpidr: 0x81000400,
                sched: RoundRobin,
            },
            PlatCpuCoreConfig {
                //cluster1
                name: ARM_CORTEX_A76,
                mpidr: 0x81000500,
                sched: RoundRobin,
            },
            PlatCpuCoreConfig {
                //cluster2
                name: ARM_CORTEX_A76,
                mpidr: 0x81000600,
                sched: RoundRobin,
            },
            PlatCpuCoreConfig {
                //cluster2
                name: ARM_CORTEX_A76,
                mpidr: 0x81000700,
                sched: RoundRobin,
            },
        ],
    },
    mem_desc: PlatMemoryConfig {
        regions: &[
            PlatMemRegion {
                base: 0x200000,
                size: 0x8200000,
            },
            PlatMemRegion {
                base: 0x9400000,
                size: 0x76c00000,
            },
            PlatMemRegion {
                base: 0x80000000,
                size: 0x80000000,
            },
            PlatMemRegion {
                base: 0x100000000,
                size: 0x80000000,
            },
            PlatMemRegion {
                base: 0x180000000,
                size: 0x80000000,
            },
        ],
        base: 0x200000,
    },
    arch_desc: ArchDesc {
        gic_desc: GicDesc {
            gicd_addr: Platform::GICD_BASE,
            gicc_addr: Platform::GICC_BASE,
            gich_addr: Platform::GICH_BASE,
            gicv_addr: Platform::GICV_BASE,
            gicr_addr: Platform::GICR_BASE,
            maintenance_int_id: 25,
        },
        smmu_desc: SmmuDesc {
            base: 0xfcb00000,
            interrupt_id: 0x17d,
            global_mask: 0, //0x200000
        },
        cluster_desc: ClusterDesc {
            num: 3,
            core_num: &[4, 2, 2],
        },
    },
};
