// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use crate::arch::{timer_arch_get_counter, timer_arch_get_frequency};

/// Get current time in microseconds.
pub fn time_current_us() -> usize {
    let count = timer_arch_get_counter();
    let freq = timer_arch_get_frequency();
    count * 1000000 / freq
}

/// Get current time in milliseconds.
pub fn time_current_ms() -> usize {
    let count = timer_arch_get_counter();
    let freq = timer_arch_get_frequency();
    count * 1000 / freq
}

/// Sleep for `us` microseconds.
pub fn sleep(us: usize) {
    let end = time_current_us() + us;
    while time_current_us() < end {
        core::hint::spin_loop();
    }
}
