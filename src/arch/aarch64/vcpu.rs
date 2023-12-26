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

use crate::arch::traits::ContextFrameTrait;
use crate::kernel::{Vcpu, Vm};
use crate::kernel::VmType;

/// vcpu init function for specific architecture
pub fn vcpu_arch_init(vm: Vm, vcpu: Vcpu) {
    let config = vm.config();
    let mut vcpu_inner = vcpu.inner.inner_mut.lock();
    match config.os_type {
        VmType::VmTOs => {
            vcpu_inner.vcpu_ctx.set_argument(config.device_tree_load_ipa());
        }
        _ => {
            let arg = &config.memory_region()[0];
            vcpu_inner.vcpu_ctx.set_argument(arg.ipa_start + arg.length);
        }
    }

    vcpu_inner.vcpu_ctx.set_exception_pc(config.kernel_entry_point());
    vcpu_inner.vcpu_ctx.spsr =
        (SPSR_EL1::M::EL1h + SPSR_EL1::I::Masked + SPSR_EL1::F::Masked + SPSR_EL1::A::Masked + SPSR_EL1::D::Masked)
            .value;
}
