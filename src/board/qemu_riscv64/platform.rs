// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

/// external fdt crate
extern crate fdt_rs;

// TODO: move these core name to device
/// Crate imports for architecture and board configurations
use crate::arch::ArchDesc;
#[cfg(feature = "gicv3")]
use crate::arch::sysreg_enc_addr;
use crate::board::{
    PlatOperation, Platform, PlatCpuCoreConfig, PlatCpuConfig, PlatformConfig, PlatMemoryConfig, PlatMemRegion,
    ClusterDesc,
};
use crate::board::SchedRule::RoundRobin;

/// Represents the platform configuration for QEMU virtual machines
pub struct QemuPlatform;

/// Implementation of platform operations for QEMU virtual machines
impl PlatOperation for QemuPlatform {
    /// UART base addresses
    const UART_0_ADDR: usize = 0x9000000;
    const UART_1_ADDR: usize = 0x9100000;
    const UART_2_ADDR: usize = 0x9110000;

    /// UART interrupt numbers
    const UART_0_INT: usize = 32 + 0x70;
    const UART_1_INT: usize = 32 + 0x72;

    /// Hypervisor UART base address
    const HYPERVISOR_UART_BASE: usize = Self::UART_0_ADDR;

    // Invalid fields, just for PlatOperation Trait
    const GICD_BASE: usize = 0;
    const GICC_BASE: usize = 0;
    const GICH_BASE: usize = 0;
    const GICV_BASE: usize = 0;
    #[cfg(feature = "gicv3")]
    const GICR_BASE: usize = 0;
    #[cfg(feature = "gicv3")]
    const ICC_SRE_ADDR: usize = 0;
    #[cfg(feature = "gicv3")]
    const ICC_SGIR_ADDR: usize = 0;

    const DISK_PARTITION_0_START: usize = 0;
    const DISK_PARTITION_1_START: usize = 0;
    const DISK_PARTITION_2_START: usize = 0;

    const DISK_PARTITION_TOTAL_SIZE: usize = 0;
    const DISK_PARTITION_0_SIZE: usize = 0;
    const DISK_PARTITION_1_SIZE: usize = 0;
    const DISK_PARTITION_2_SIZE: usize = 0;

    /// Maps CPU ID to CPU interface number for QEMU
    fn cpuid_to_cpuif(cpuid: usize) -> usize {
        // PLAT_DESC.cpu_desc.core_list[cpuid].mpidr
        cpuid
    }
    /// Maps CPU interface number to CPU ID for QEMU
    fn cpuif_to_cpuid(cpuif: usize) -> usize {
        cpuif
    }

    // should not add any println!()
    /// Converts MPIDR to CPU ID for QEMU, ensuring it is in the valid range
    /// In riscv, mpidr is equal to id.
    fn mpidr2cpuid(mpidr: usize) -> usize {
        mpidr
    }
}

/// Static configuration for the QEMU platform
pub const PLAT_DESC: PlatformConfig = PlatformConfig {
    /// CPU configuration details for QEMU
    cpu_desc: PlatCpuConfig {
        num: 4,
        core_list: &[
            /// Configuration for each CPU core in QEMU
            PlatCpuCoreConfig {
                name: 0,
                mpidr: 0,
                sched: RoundRobin,
            },
            PlatCpuCoreConfig {
                name: 0,
                mpidr: 1,
                sched: RoundRobin,
            },
            PlatCpuCoreConfig {
                name: 0,
                mpidr: 2,
                sched: RoundRobin,
            },
            PlatCpuCoreConfig {
                name: 0,
                mpidr: 3,
                sched: RoundRobin,
            },
        ],
        /// Cluster description for the CPU cores
        cluster_desc: ClusterDesc { num: 1, core_num: &[4] },
    },
    // Memory configuration details for QEMU
    mem_desc: PlatMemoryConfig {
        regions: &[
            // reserve 0x80200000 ~ 0x88200000 for Shyper
            PlatMemRegion {
                base: 0x80200000,
                size: 0x08000000,
            },
            // General memory region for QEMU
            PlatMemRegion {
                base: 0x90000000,
                size: 0x1f0000000,
            },
        ],
        base: 0x80000000,
    },
    // Architecture-specific configuration for QEMU
    arch_desc: ArchDesc {
        // GIC (Generic Interrupt Controller) configuration for QEMU
        gic_desc: crate::arch::GicDesc {
            gicd_addr: Platform::GICD_BASE,
            gicc_addr: Platform::GICC_BASE,
            gich_addr: Platform::GICH_BASE,
            gicv_addr: Platform::GICV_BASE,
            #[cfg(feature = "gicv3")]
            gicr_addr: Platform::GICR_BASE,
            maintenance_int_id: 0,
        },
        // SMMU (System Memory Management Unit) configuration for QEMU
        smmu_desc: crate::arch::SmmuDesc {
            base: 0,
            interrupt_id: 0,
            global_mask: 0,
        },
    },
};
