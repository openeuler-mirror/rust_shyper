// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use crate::arch::ArchTrait;
use riscv::register::hgatp::Setting;

pub const PAGE_SIZE: usize = 4096;
pub const PAGE_SHIFT: usize = 12;
pub const ENTRY_PER_PAGE: usize = PAGE_SIZE / 8;

pub type ContextFrame = super::context_frame::Riscv64ContextFrame;
pub const NUM_CORE: usize = crate::board::PLAT_DESC.cpu_desc.num;

pub const WORD_SIZE: usize = 8;
pub const PTE_PER_PAGE: usize = PAGE_SIZE / WORD_SIZE;

pub type Arch = Riscv64Arch;

pub struct Riscv64Arch;

impl ArchTrait for Riscv64Arch {
    fn wait_for_interrupt() {
        // SAFETY: Wait for interrupt
        unsafe { riscv::asm::wfi() };
    }

    fn install_vm_page_table(base: usize, vmid: usize) {
        // TODO: Too many VMs may result in insufficient bits of vm id
        use riscv::register::hgatp::Mode;
        let setting = Setting::new(Mode::Sv39x4, vmid as u16, base >> 12);
        riscv::register::hgatp::set(&setting);

        // SAFETY: Flush gTLB
        unsafe { core::arch::riscv64::hfence_gvma_all() };

        // SAFETY: Flush I-Cache
        unsafe { riscv::asm::fence_i() };

        // Flush D-Cache
        // TODO: Is the D-Cache refresh here necessary?
        // If it is VIPT, you do not need to refresh the D-Cache
    }
}
