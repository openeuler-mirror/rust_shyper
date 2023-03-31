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

pub const PLATFORM_CPU_NUM_MAX: usize = 8;
pub const PLATFORM_VCPU_NUM_MAX: usize = 8;

#[repr(C)]
pub enum SchedRule {
    RoundRobin,
    None,
}

#[repr(C)]
pub struct PlatMemRegion {
    pub base: usize,
    pub size: usize,
}

#[repr(C)]
pub struct PlatMemoryConfig {
    pub base: usize,
    pub regions: &'static [PlatMemRegion],
}

pub struct PlatCpuCoreConfig {
    pub name: u8,
    pub mpidr: usize,
    pub sched: SchedRule,
}

#[repr(C)]
pub struct PlatCpuConfig {
    pub num: usize,
    pub core_list: &'static [PlatCpuCoreConfig],
}

#[repr(C)]
pub struct ArchDesc {
    pub gic_desc: GicDesc,
    pub smmu_desc: SmmuDesc,
}

#[repr(C)]
pub struct PlatformConfig {
    pub cpu_desc: PlatCpuConfig,
    pub mem_desc: PlatMemoryConfig,
    pub uart_base: usize,
    pub arch_desc: ArchDesc,
}
