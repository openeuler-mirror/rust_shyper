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

pub fn putc(byte: u8) {
    #[cfg(feature = "qemu")]
    unsafe {
        use crate::board::UART_0_ADDR;
        ptr::write_volatile(UART_0_ADDR as *mut u8, byte);
    }
    // ns16550
    #[cfg(feature = "tx2")]
    unsafe {
        use crate::board::UART_1_ADDR;
        if byte == '\n' as u8 {
            putc('\r' as u8);
        }
        while ptr::read_volatile((UART_1_ADDR + 0x8_0000_0000 + 20) as *const u8) & 0x20 == 0 {}
        ptr::write_volatile((UART_1_ADDR + 0x8_0000_0000) as *mut u8, byte);
        // while ptr::read_volatile((UART_1_ADDR + 20) as *const u8) & 0x20 == 0 {}
        // ptr::write_volatile(UART_1_ADDR as *mut u8, byte);
    }
    // pl011
    #[cfg(feature = "pi4")]
    unsafe {
        use crate::board::UART_0_ADDR;
        if byte == '\n' as u8 {
            putc('\r' as u8);
        }
        while (ptr::read_volatile((UART_0_ADDR as usize + 24) as *const u32) & (1 << 5)) != 0 {}
        ptr::write_volatile(UART_0_ADDR as *mut u32, byte as u32);
    }
}
