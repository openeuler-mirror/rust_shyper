// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use cortex_a::registers::{DAIF, TPIDR_EL2};
use tock_registers::interfaces::*;

/// Mask (disable) interrupt from perspective of CPU
#[inline(always)]
pub fn cpu_interrupt_mask() {
    DAIF.write(DAIF::I::Masked)
}

/// Unmask (enable) interrupt from perspective of CPU
#[inline(always)]
pub fn cpu_interrupt_unmask() {
    DAIF.write(DAIF::I::Unmasked)
}

/// return the current interrupt mask state
pub fn cpu_daif() -> u64 {
    DAIF.read(DAIF::I)
}

#[inline(always)]
pub fn current_cpu_arch() -> u64 {
    TPIDR_EL2.get()
}

/// # Safety:
/// The 'cpu_addr' must be a valid address of 'Cpu' struct.
pub unsafe fn set_current_cpu(cpu_addr: u64) {
    TPIDR_EL2.set(cpu_addr);
}
