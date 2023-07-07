// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use std::process::Command;
use std::fs;
use std::env::var;

fn main() {
    let files = fs::read_dir("libfdt-binding").unwrap().into_iter().filter_map(|f| {
        let f = f.as_ref().unwrap();
        if f.file_type().unwrap().is_file() && matches!(f.path().extension(), Some(ext) if ext == "c") {
            Some(f.path())
        } else {
            None
        }
    });
    cc::Build::new()
        .compiler("aarch64-none-elf-gcc")
        .include("libfdt-binding")
        .files(files)
        .flag("-w")
        .compile("fdt-binding");

    let arch = var("CARGO_CFG_TARGET_ARCH").unwrap();
    let text_start = if cfg!(feature = "tx2") {
        if cfg!(feature = "update") {
            0x8a000000_u64
        } else {
            0x83000000_u64
        }
    } else if cfg!(feature = "pi4") {
        0xf0080000_u64
    } else if cfg!(feature = "qemu") {
        0x40080000_u64
    } else {
        panic!("Unsupported platform!");
    };
    println!("cargo:rustc-link-arg=-Tlinkers/{arch}.ld");
    println!("cargo:rustc-link-arg=--defsym=TEXT_START={text_start}");

    // note: add error checking yourself.
    let output = Command::new("date").arg("+\"%Y-%m-%d %H:%M:%S %Z\"").output().unwrap();
    let build_time = String::from_utf8(output.stdout).unwrap();
    println!("cargo:rustc-env=BUILD_TIME={}", build_time);
}
