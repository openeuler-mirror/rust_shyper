// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use cortex_a::registers::*;

use crate::kernel::Vcpu;
use crate::arch::traits::ContextFrameTrait;
use crate::config::VmConfigEntry;
use crate::kernel::VmType;

/// vcpu init function for specific architecture
impl Vcpu {
    /// init boot info, including argument and exception pc
    pub fn init_boot_info(&self, config: &VmConfigEntry) {
        let arg = match config.os_type {
            VmType::VmTOs => config.device_tree_load_ipa(),
            _ => {
                let arg = &config.memory_region()[0];
                arg.ipa_start + arg.length
            }
        };
        let mut inner = self.inner.inner_mut.lock();
        inner.vcpu_ctx.set_argument(arg);
        inner.vcpu_ctx.set_exception_pc(config.kernel_entry_point());
    }

    pub fn set_gich_ctlr(&self, ctlr: u32) {
        let mut inner = self.inner.inner_mut.lock();
        inner.vm_ctx.gic_state.ctlr = ctlr;
    }

    pub fn set_hcr(&self, hcr: u64) {
        let mut inner = self.inner.inner_mut.lock();
        inner.vm_ctx.hcr_el2 = hcr;
    }

    pub fn init_spsr(&self) {
        let mut vcpu_inner = self.inner.inner_mut.lock();
        vcpu_inner.vcpu_ctx.spsr =
            (SPSR_EL1::M::EL1h + SPSR_EL1::I::Masked + SPSR_EL1::F::Masked + SPSR_EL1::A::Masked + SPSR_EL1::D::Masked)
                .value;
    }
}
