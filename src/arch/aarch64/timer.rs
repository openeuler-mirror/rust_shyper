// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use core::sync::atomic::{AtomicUsize, Ordering};

use tock_registers::interfaces::*;

const CTL_IMASK: usize = 1 << 1;

pub static TIMER_FREQ: AtomicUsize = AtomicUsize::new(0);
pub static TIMER_SLICE: AtomicUsize = AtomicUsize::new(0); // ms

pub fn timer_arch_set(num: usize) {
    let slice = TIMER_SLICE.load(Ordering::Relaxed);
    let val = slice * num;
    msr!(CNTHP_TVAL_EL2, val);
}

pub fn timer_arch_enable_irq() {
    let val = 1;
    msr!(CNTHP_CTL_EL2, val, "x");
}

pub fn timer_arch_disable_irq() {
    let val = 2;
    msr!(CNTHP_CTL_EL2, val, "x");
}

pub fn timer_arch_get_counter() -> usize {
    cortex_a::registers::CNTPCT_EL0.get() as usize
}

pub fn timer_arch_get_frequency() -> usize {
    cortex_a::registers::CNTFRQ_EL0.get() as usize
}

pub fn timer_arch_init() {
    let freq = timer_arch_get_frequency();
    let slice = freq / 1000;
    TIMER_FREQ.store(freq, Ordering::Relaxed);
    TIMER_SLICE.store(freq / 1000, Ordering::Relaxed);

    let ctl = 0x3 & (1 | !CTL_IMASK);
    let tval = slice * 10;
    msr!(CNTHP_CTL_EL2, ctl);
    msr!(CNTHP_TVAL_EL2, tval);
}
