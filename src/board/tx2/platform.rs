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
use crate::device::ARM_CORTEX_A57;
#[allow(unused_imports)]
use crate::device::ARM_NVIDIA_DENVER;

/// Represents the platform configuration for NVIDIA TX2
pub struct Tx2Platform;

/// Implementation of platform operations for NVIDIA TX2
impl PlatOperation for Tx2Platform {
    /// UART base addresses
    const UART_0_ADDR: usize = 0x3100000;
    const UART_1_ADDR: usize = 0xc280000;

    /// UART interrupt numbers
    const UART_0_INT: usize = 32 + 0x70;
    const UART_1_INT: usize = 32 + 0x72;

    /// Hypervisor UART base address
    const HYPERVISOR_UART_BASE: usize = Self::UART_1_ADDR;

    /// GIC (Generic Interrupt Controller) base addresses
    const GICD_BASE: usize = 0x3881000;
    const GICC_BASE: usize = 0x3882000;
    const GICH_BASE: usize = 0x3884000;
    const GICV_BASE: usize = 0x3886000;

    // start sector number (LBA)
    const DISK_PARTITION_0_START: usize = 43643256;
    const DISK_PARTITION_1_START: usize = 4104;
    const DISK_PARTITION_2_START: usize = 45740408;

    // size in sector (512-byte)
    // pub const DISK_PARTITION_TOTAL_SIZE: usize = 31457280;
    const DISK_PARTITION_0_SIZE: usize = 2097152;
    const DISK_PARTITION_1_SIZE: usize = 41943040;
    const DISK_PARTITION_2_SIZE: usize = 8388608;

    fn cpuid_to_cpuif(cpuid: usize) -> usize {
        cpuid + PLAT_DESC.cpu_desc.num
    }

    /// Maps CPU interface number to CPU ID for RK3588
    fn cpuif_to_cpuid(cpuif: usize) -> usize {
        cpuif - PLAT_DESC.cpu_desc.num
    }
}

/// Platform configuration for NVIDIA TX2
pub const PLAT_DESC: PlatformConfig = PlatformConfig {
    /// CPU configuration details for NVIDIA TX2
    cpu_desc: PlatCpuConfig {
        num: 4,
        core_list: &[
            /// Configuration for the first ARM Cortex-A57 core
            PlatCpuCoreConfig {
                name: ARM_CORTEX_A57,
                mpidr: 0x80000100,
                sched: RoundRobin,
            },
            PlatCpuCoreConfig {
                name: ARM_CORTEX_A57,
                mpidr: 0x80000101,
                sched: RoundRobin,
            },
            PlatCpuCoreConfig {
                name: ARM_CORTEX_A57,
                mpidr: 0x80000102,
                sched: RoundRobin,
            },
            PlatCpuCoreConfig {
                name: ARM_CORTEX_A57,
                mpidr: 0x80000103,
                sched: RoundRobin,
            },
        ],
        /// Cluster description for the CPU cores
        cluster_desc: ClusterDesc { num: 1, core_num: &[4] },
    },
    /// Memory configuration details for NVIDIA TX2
    mem_desc: PlatMemoryConfig {
        regions: &[
            /// Memory region configuration
            PlatMemRegion {
                base: 0x80000000,
                size: 0x10000000,
            },
            PlatMemRegion {
                base: 0x90000000,
                size: 0x60000000,
            },
            PlatMemRegion {
                base: 0xf0200000,
                size: 0x185600000,
            },
        ],
        /// Base address of the memory region for the hypervisor
        base: 0x80000000,
    },
    /// Architecture-specific configuration for NVIDIA TX2
    arch_desc: ArchDesc {
        // GIC (Generic Interrupt Controller) configuration
        gic_desc: crate::arch::GicDesc {
            gicd_addr: Platform::GICD_BASE,
            gicc_addr: Platform::GICC_BASE,
            gich_addr: Platform::GICH_BASE,
            gicv_addr: Platform::GICV_BASE,
            maintenance_int_id: 25,
        },
        /// SMMU (System Memory Management Unit) configuration
        smmu_desc: crate::arch::SmmuDesc {
            base: 0x12000000,
            interrupt_id: 187,
            global_mask: 0x7f80,
        },
    },
};
