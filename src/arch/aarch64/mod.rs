// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

pub use self::context_frame::*;
pub use self::cpu::*;
pub use self::exception::*;
#[cfg(not(feature = "gicv3"))]
pub use self::gic::*;
#[cfg(feature = "gicv3")]
pub use self::gicv3::*;
pub use self::interface::*;
pub use self::interrupt::*;
pub use self::page_table::*;
pub use self::psci::*;
pub use self::smc::*;
pub use self::smmu::*;
pub use self::sync::*;
pub use self::timer::*;
#[cfg(not(feature = "gicv3"))]
pub use self::vgic::*;
#[cfg(feature = "gicv3")]
pub use self::vgicv3::*;
pub use self::start::*;
pub use self::regs::*;
pub use self::cache::*;
pub use self::tlb::*;

#[macro_use]
mod regs;
pub mod cache;
mod context_frame;
mod cpu;
mod exception;
#[cfg(not(feature = "gicv3"))]
mod gic;
#[cfg(feature = "gicv3")]
mod gicv3;
mod interface;
mod interrupt;
mod mmu;
mod page_table;
mod psci;
mod smc;
mod smmu;
mod start;
mod sync;
mod timer;
pub mod tlb;
mod vcpu;
#[cfg(not(feature = "gicv3"))]
mod vgic;
#[cfg(feature = "gicv3")]
mod vgicv3;
mod vm;

#[repr(C)]
pub struct ArchDesc {
    pub gic_desc: GicDesc,
    pub smmu_desc: SmmuDesc,
}
