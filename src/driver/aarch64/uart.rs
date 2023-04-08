// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use core::ptr;

use crate::board::{Platform, PlatOperation};

pub fn putc(byte: u8) {
    const UART_BASE: usize = Platform::HYPERVISOR_UART_BASE + 0x8_0000_0000;
    // ns16550
    #[cfg(feature = "tx2")]
    unsafe {
        if byte == '\n' as u8 {
            putc('\r' as u8);
        }
        while ptr::read_volatile((UART_BASE + 20) as *const u8) & 0x20 == 0 {}
        ptr::write_volatile(UART_BASE as *mut u8, byte);
    }
    // pl011
    #[cfg(any(feature = "pi4", feature = "qemu"))]
    unsafe {
        if byte == '\n' as u8 {
            putc('\r' as u8);
        }
        while (ptr::read_volatile((UART_BASE as usize + 24) as *const u32) & (1 << 5)) != 0 {}
        ptr::write_volatile(UART_BASE as *mut u32, byte as u32);
    }
}
