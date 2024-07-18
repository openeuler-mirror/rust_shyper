// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

/// Crate imports for architecture and board configurations
use crate::arch::ArchDesc;
use crate::board::{
    PlatOperation, Platform, PlatCpuCoreConfig, ClusterDesc, PlatCpuConfig, PlatformConfig, PlatMemoryConfig,
    PlatMemRegion,
};
use crate::board::SchedRule::RoundRobin;
use crate::device::ARM_CORTEX_A55;
use crate::device::ARM_CORTEX_A76;
#[allow(unused_imports)]
use crate::device::ARM_NVIDIA_DENVER;

/// Represents the platform configuration for Rockchip RK3588
pub struct Rk3588Platform;

/// Implementation of platform operations for Rockchip RK3588
impl PlatOperation for Rk3588Platform {
    /// UART base addresses
    const UART_0_ADDR: usize = 0xfeb50000;
    const UART_1_ADDR: usize = 0xfeba0000;

    /// UART interrupt numbers
    const UART_0_INT: usize = 32 + 0x12d;
    const UART_1_INT: usize = 32 + 0x134;

    /// Hypervisor UART base address
    const HYPERVISOR_UART_BASE: usize = Self::UART_0_ADDR;

    /// GIC (Generic Interrupt Controller) base addresses
    const GICD_BASE: usize = 0xfe600000;
    const GICC_BASE: usize = 0xfe610000;
    const GICH_BASE: usize = 0xfe620000;
    const GICV_BASE: usize = 0xfe630000;
    const GICR_BASE: usize = 0xfe680000;

    /// start sector number (LBA)
    const DISK_PARTITION_0_START: usize = 16384;
    const DISK_PARTITION_1_START: usize = 32768;
    const DISK_PARTITION_2_START: usize = 40960;

    /// size in sector (512-byte)
    /// pub const DISK_PARTITION_TOTAL_SIZE: usize = 31457280;
    const DISK_PARTITION_0_SIZE: usize = 16384;
    const DISK_PARTITION_1_SIZE: usize = 8192;
    const DISK_PARTITION_2_SIZE: usize = 524288;

    /// Converts MPIDR to CPU ID for RK3588
    fn mpidr2cpuid(mpidr: usize) -> usize {
        (mpidr >> 8) & 0xff
    }

    /// Maps CPU ID to CPU interface number for RK3588
    fn cpuid_to_cpuif(cpuid: usize) -> usize {
        PLAT_DESC.cpu_desc.core_list[cpuid].mpidr
    }

    /// Maps CPU interface number to CPU ID for RK3588
    fn cpuif_to_cpuid(cpuif: usize) -> usize {
        cpuif
    }

    /// Converts virtual MPIDR to virtual CPU ID for RK3588
    fn vmpidr2vcpuid(vmpidr: usize) -> usize {
        (vmpidr >> 8) & 0xff
    }
}

/// Static configuration for the Rockchip RK3588 platform
pub const PLAT_DESC: PlatformConfig = PlatformConfig {
    /// CPU configuration details for RK3588
    cpu_desc: PlatCpuConfig {
        num: 8,
        core_list: &[
            /// Configuration for each CPU core in cluster 0
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
        /// Cluster description for the CPU cores
        cluster_desc: ClusterDesc {
            num: 3,
            core_num: &[4, 2, 2],
        },
    },
    /// Memory configuration details for RK3588
    mem_desc: PlatMemoryConfig {
        regions: &[
            /// Memory region configurations
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
        /// Memory base configuration
        base: 0x200000,
    },
    /// Architecture-specific configuration for RK3588
    arch_desc: ArchDesc {
        /// GIC (Generic Interrupt Controller) configuration for RK3588
        gic_desc: crate::arch::GicDesc {
            gicd_addr: Platform::GICD_BASE,
            gicc_addr: Platform::GICC_BASE,
            gich_addr: Platform::GICH_BASE,
            gicv_addr: Platform::GICV_BASE,
            gicr_addr: Platform::GICR_BASE,
            maintenance_int_id: 25,
        },
        /// SMMU (System Memory Management Unit) configuration for RK3588
        smmu_desc: crate::arch::SmmuDesc {
            base: 0xfcb00000,
            interrupt_id: 0x17d,
            global_mask: 0, //0x200000
        },
    },
};
