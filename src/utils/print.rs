// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use core::fmt::{Arguments, Write};
use spin::Mutex;

pub struct Writer;

static WRITER: Mutex<Writer> = Mutex::new(Writer);

impl Write for Writer {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for b in s.bytes() {
            #[cfg(target_arch = "aarch64")]
            crate::driver::putc(b);
            #[cfg(target_arch = "riscv64")]
            sbi::legacy::console_putchar(b);
        }
        Ok(())
    }
}

/// Print a format string to the console.
pub fn _print(args: Arguments) {
    // use core::fmt::Write;
    let mut lock = WRITER.lock();
    lock.write_fmt(args).unwrap();
}
