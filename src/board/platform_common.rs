// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use crate::arch::ArchDesc;

#[cfg(target_arch = "riscv64")]
use crate::arch::arch_boot_other_cores;

/// Maximum number of CPUs supported by the platform
pub const PLATFORM_CPU_NUM_MAX: usize = 8;

/// Enum representing the scheduling rule for CPU cores
pub enum SchedRule {
    /// Round-robin scheduling
    RoundRobin,
    /// No specific scheduling rule
    None,
}

/// Structure representing a memory region in the platform
pub struct PlatMemRegion {
    pub base: usize,
    pub size: usize,
}

/// Configuration for the platform's memory
pub struct PlatMemoryConfig {
    pub base: usize,
    pub regions: &'static [PlatMemRegion],
}

/// Configuration for an individual CPU core
pub struct PlatCpuCoreConfig {
    pub name: u8,
    pub mpidr: usize,
    pub sched: SchedRule,
}

/// Configuration for the platform's CPU
pub struct PlatCpuConfig {
    pub num: usize,
    pub core_list: &'static [PlatCpuCoreConfig],
    pub cluster_desc: ClusterDesc,
}

/// Description of a CPU cluster
pub struct ClusterDesc {
    pub num: usize,
    pub core_num: &'static [usize],
}

/// Configuration for the entire platform, including CPU, memory, and architecture
pub struct PlatformConfig {
    pub cpu_desc: PlatCpuConfig,
    pub mem_desc: PlatMemoryConfig,
    pub arch_desc: ArchDesc,
}

/// Trait defining operations for platform management
pub trait PlatOperation {
    // must offer UART_0 and UART_1 address
    const UART_0_ADDR: usize;
    const UART_1_ADDR: usize;
    const UART_2_ADDR: usize = usize::MAX;

    // must offer hypervisor used uart
    const HYPERVISOR_UART_BASE: usize;

    const UART_0_INT: usize = usize::MAX;
    const UART_1_INT: usize = usize::MAX;
    const UART_2_INT: usize = usize::MAX;

    // must offer interrupt controller
    const GICD_BASE: usize;
    const GICC_BASE: usize;
    const GICH_BASE: usize;
    const GICV_BASE: usize;
    #[cfg(feature = "gicv3")]
    const GICR_BASE: usize;

    const DISK_PARTITION_0_START: usize = usize::MAX;
    const DISK_PARTITION_1_START: usize = usize::MAX;
    const DISK_PARTITION_2_START: usize = usize::MAX;
    const DISK_PARTITION_3_START: usize = usize::MAX;
    const DISK_PARTITION_4_START: usize = usize::MAX;

    const DISK_PARTITION_TOTAL_SIZE: usize = usize::MAX;
    const DISK_PARTITION_0_SIZE: usize = usize::MAX;
    const DISK_PARTITION_1_SIZE: usize = usize::MAX;
    const DISK_PARTITION_2_SIZE: usize = usize::MAX;
    const DISK_PARTITION_3_SIZE: usize = usize::MAX;
    const DISK_PARTITION_4_SIZE: usize = usize::MAX;

    /// # Safety:
    /// The caller must ensure that the arch_core_id is in the range of the cpu_desc.core_list
    /// The entry must be a valid address with executable permission
    /// The ctx must be a valid cpu_idx
    unsafe fn cpu_on(arch_core_id: usize, entry: usize, ctx: usize) {
        crate::arch::power_arch_cpu_on(arch_core_id, entry, ctx);
    }

    /// Shuts down the current CPU
    fn cpu_shutdown() {
        crate::arch::power_arch_cpu_shutdown();
    }

    /// Powers on secondary cores of the CPU
    fn power_on_secondary_cores() {
        extern "C" {
            fn _secondary_start();
        }
        #[cfg(target_arch = "aarch64")]
        {
            use super::PLAT_DESC;
            for i in 1..PLAT_DESC.cpu_desc.num {
                // SAFETY:
                // We iterate all the cores except the primary core so the arch_core_id must be valid.
                // Entry is a valid address with executable permission.
                // The 'i' is a valid cpu_idx for ctx.
                unsafe {
                    Self::cpu_on(PLAT_DESC.cpu_desc.core_list[i].mpidr, _secondary_start as usize, i);
                }
            }
        }
        #[cfg(target_arch = "riscv64")]
        {
            arch_boot_other_cores();
        }
    }

    /// Reboots the system
    /// # Safety:
    /// The caller must ensure that the system can be reboot
    unsafe fn sys_reboot() -> ! {
        info!("Hypervisor reset...");
        // SAFETY: We are ready to reset the system when rebooting.
        unsafe {
            crate::arch::power_arch_sys_reset();
        }
        loop {
            core::hint::spin_loop();
        }
    }

    /// Shuts down the system
    /// # Safety:
    /// The caller must ensure that the system can be shutdown
    unsafe fn sys_shutdown() -> ! {
        info!("Hypervisor shutdown...");
        crate::arch::power_arch_sys_shutdown();
        loop {
            core::hint::spin_loop();
        }
    }

    /// Maps a CPU ID to a CPU interface number
    fn cpuid_to_cpuif(cpuid: usize) -> usize;

    /// Maps a CPU interface number to a CPU ID
    fn cpuif_to_cpuid(cpuif: usize) -> usize;

    /// Maps an MPIDR value to a CPU ID
    ///
    /// This function does not print to the console and is used to translate
    /// MPIDR (Multiprocessor Affinity Register) values to CPU IDs.
    // should not add any println!()
    fn mpidr2cpuid(mpidr: usize) -> usize {
        use crate::board::PLAT_DESC;
        let mpidr = mpidr | 0x8000_0000;
        for i in 0..PLAT_DESC.cpu_desc.num {
            if mpidr == PLAT_DESC.cpu_desc.core_list[i].mpidr {
                return i;
            }
        }
        usize::MAX
    }

    /// Maps a CPU ID to an MPIDR value
    ///
    /// This function is used for translating a CPU ID to its corresponding
    /// MPIDR value.
    fn cpuid2mpidr(cpuid: usize) -> usize {
        use crate::board::PLAT_DESC;
        PLAT_DESC.cpu_desc.core_list[cpuid].mpidr
    }

    /// Maps a virtual MPIDR value to a virtual CPU ID
    ///
    /// This function is used in virtualized environments to translate
    /// virtual MPIDR values to virtual CPU IDs.
    fn vmpidr2vcpuid(vmpidr: usize) -> usize {
        vmpidr & 0xff
    }
}
