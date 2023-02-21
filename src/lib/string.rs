// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use core::arch::global_asm;

global_asm!(include_str!("../arch/aarch64/memset.S"));
global_asm!(include_str!("../arch/aarch64/memcpy.S"));
extern "C" {
    pub fn memset(s: *mut u8, c: i32, n: usize) -> *mut u8;
    pub fn memcpy(s1: *const u8, s2: *const u8, n: usize) -> *mut u8;
}

pub fn memset_safe(s: *mut u8, c: i32, n: usize) -> *mut u8 {
    if (s as usize) < 0x1000 {
        panic!("illegal addr for memset s {:x}", s as usize);
    }
    unsafe { memset(s, c, n) }
}

pub fn memcpy_safe(s1: *const u8, s2: *const u8, n: usize) -> *mut u8 {
    if (s1 as usize) < 0x1000 || (s2 as usize) < 0x1000 {
        panic!("illegal addr for memcpy s1 {:x} s2 {:x}", s1 as usize, s2 as usize);
    }
    unsafe { memcpy(s1, s2, n) }
}
