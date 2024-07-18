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

pub const PAGE_SIZE: usize = 4096;
pub const PAGE_SHIFT: usize = 12;

/// The number of page table entries in a page.
pub const ENTRY_PER_PAGE: usize = PAGE_SIZE / 8;

pub type ContextFrame = super::context_frame::Aarch64ContextFrame;

pub const WORD_SIZE: usize = 8;
pub const PTE_PER_PAGE: usize = PAGE_SIZE / WORD_SIZE;

pub type Arch = Aarch64Arch;

pub struct Aarch64Arch;

impl ArchTrait for Aarch64Arch {
    fn wait_for_interrupt() {
        // SAFETY: Wait for interrupt
        crate::arch::wfi();
    }

    // Restore the MMU context of a VM Stage2 (typically set vmid)
    fn install_vm_page_table(base: usize, vmid: usize) {
        // restore vm's Stage2 MMU context
        let vttbr = (vmid << 48) | base;
        // SAFETY: 'vttbr' is saved in the vcpu struct when last scheduled
        unsafe {
            core::arch::asm!("msr VTTBR_EL2, {0}", in(reg) vttbr);
            crate::arch::isb();
        }
    }
}
