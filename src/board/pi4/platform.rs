// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

// Crate imports for architecture and board configurations
use crate::arch::ArchDesc;
use crate::board::{
    PlatOperation, Platform, PlatCpuCoreConfig, ClusterDesc, PlatCpuConfig, PlatformConfig, PlatMemoryConfig,
    PlatMemRegion,
};

/// Set scheduling for CPU cores
use crate::board::SchedRule::RoundRobin;

/// Import device-specific constants for ARM Cortex-A57
use crate::device::ARM_CORTEX_A57;

/// Allow unused imports for potential future use
#[allow(unused_imports)]

/// Import device-specific constants for ARM Cortex-A57
use crate::device::ARM_NVIDIA_DENVER;

/// Represents the platform configuration for Raspberry Pi 4
pub struct Pi4Platform;

/// Definition platform operations for the Raspberry Pi 4
impl PlatOperation for Pi4Platform {
    /// UART base addresses
    const UART_0_ADDR: usize = 0xFE201000;
    const UART_1_ADDR: usize = 0xFE201400;

    /// UART interrupt numbers
    const UART_0_INT: usize = 32 + 0x79;
    const UART_1_INT: usize = 32 + 0x79;

    /// Hypervisor UART base address
    const HYPERVISOR_UART_BASE: usize = Self::UART_0_ADDR;

    /// GIC (Generic Interrupt Controller) base addresses
    const GICD_BASE: usize = 0xFF841000;
    const GICC_BASE: usize = 0xFF842000;
    const GICH_BASE: usize = 0xFF844000;
    const GICV_BASE: usize = 0xFF846000;

    // start sector number (LBA)
    const DISK_PARTITION_0_START: usize = 2048;
    const DISK_PARTITION_1_START: usize = 526336;
    const DISK_PARTITION_2_START: usize = 17303552;
    const DISK_PARTITION_3_START: usize = 34082816;
    const DISK_PARTITION_4_START: usize = 50862080;

    /// size in sector (512-byte)
    const DISK_PARTITION_0_SIZE: usize = 524288;
    const DISK_PARTITION_1_SIZE: usize = 16777216;
    const DISK_PARTITION_2_SIZE: usize = 16777216;
    const DISK_PARTITION_3_SIZE: usize = 16777216;
    const DISK_PARTITION_4_SIZE: usize = 11471872;

    /// Maps CPU ID to CPU interface number
    fn cpuid_to_cpuif(cpuid: usize) -> usize {
        cpuid
    }

    /// Maps CPU interface number to CPU ID
    fn cpuif_to_cpuid(cpuif: usize) -> usize {
        cpuif
    }
}

/// Static configuration for the Raspberry Pi 4 platform
pub const PLAT_DESC: PlatformConfig = PlatformConfig {
    /// CPU configuration details
    cpu_desc: PlatCpuConfig {
        num: 4,
        core_list: &[
            /// Configuration for each CPU core
            PlatCpuCoreConfig {
                name: ARM_CORTEX_A57,
                mpidr: 0x80000000,
                sched: RoundRobin,
            },
            PlatCpuCoreConfig {
                name: ARM_CORTEX_A57,
                mpidr: 0x80000001,
                sched: RoundRobin,
            },
            PlatCpuCoreConfig {
                name: ARM_CORTEX_A57,
                mpidr: 0x80000002,
                sched: RoundRobin,
            },
            PlatCpuCoreConfig {
                name: ARM_CORTEX_A57,
                mpidr: 0x80000003,
                sched: RoundRobin,
            },
        ],
        /// Cluster description for the CPU cores
        cluster_desc: ClusterDesc { num: 1, core_num: &[4] },
    },
    /// Memory configuration details
    mem_desc: PlatMemoryConfig {
        regions: &[
            /// Memory region configurations
            PlatMemRegion {
                base: 0xf0000000,
                size: 0xc000000,
            },
            PlatMemRegion {
                base: 0x200000,
                size: 0x3e000000 - 0x200000,
            },
            PlatMemRegion {
                base: 0x40000000,
                size: 0xf0000000 - 0x40000000,
            },
            PlatMemRegion {
                base: 0x100000000,
                size: 0x100000000,
            },
        ],
        /// Memory base configuration
        base: 0xf0000000,
    },
    /// Architecture-specific configuration details
    arch_desc: ArchDesc {
        /// GIC (Generic Interrupt Controller) configuration
        gic_desc: crate::arch::GicDesc {
            gicd_addr: Platform::GICD_BASE,
            gicc_addr: Platform::GICC_BASE,
            gich_addr: Platform::GICH_BASE,
            gicv_addr: Platform::GICV_BASE,
            maintenance_int_id: 25,
        },
        /// SMMU (System Memory Management Unit) configuration
        smmu_desc: crate::arch::SmmuDesc {
            base: 0,
            interrupt_id: 0,
            global_mask: 0,
        },
    },
};
