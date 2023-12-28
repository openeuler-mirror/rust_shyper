// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

/// Safe wrapper for memset
/// # Safety:
/// s must be a valid pointer to a writable memory region of at least n bytes and aligned to 1.
/// c must be a valid value of type u8.
pub unsafe fn memset(s: *mut u8, c: i32, n: usize) {
    if (s as usize) < 0x1000 {
        panic!("illegal addr for memset s {:x}", s as usize);
    }
    core::ptr::write_bytes(s, c as u8, n);
}

/// Safe wrapper for memcpy
/// # Safety:
/// * `src` must be valid for reads of `count * size_of::<T>()` bytes.
/// * `dst` must be valid for writes of `count * size_of::<T>()` bytes.
/// * Both `src` and `dst` must be properly aligned.
pub unsafe fn memcpy(dst: *const u8, src: *const u8, n: usize) {
    if (src as usize) < 0x1000 || (dst as usize) < 0x1000 {
        panic!("illegal addr for memcpy s1 {:x} s2 {:x}", dst as usize, src as usize);
    }
    core::ptr::copy_nonoverlapping(src, dst as *mut _, n);
}
