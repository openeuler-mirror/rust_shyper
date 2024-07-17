// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

//! Panic handler

use core::panic::PanicInfo;

use crate::kernel::current_cpu;

#[cfg_attr(target_os = "none", panic_handler)]
fn panic(info: &PanicInfo) -> ! {
    println!("\u{1B}[1;31m[Panic] core {}", current_cpu().id); // 1;31 BrightRed
    println!("{}\u{1B}[0m", info);
    loop {
        core::hint::spin_loop();
    }
}
