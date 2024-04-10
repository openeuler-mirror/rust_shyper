// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use std::env::var;
use std::process::Command;

// get text_start by arch and platform
fn get_text_start() -> u64 {
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();

    if target_arch == "aarch64" {
        if cfg!(feature = "tx2") {
            0x83000000_u64
        } else if cfg!(feature = "pi4") {
            0xf0080000_u64
        } else if cfg!(feature = "qemu") {
            0x40080000_u64
        } else if cfg!(feature = "rk3588") {
            0x00400000_u64
        } else {
            panic!("Unsupported platform!");
        }
    } else if target_arch == "riscv64" {
        0x80200000_u64
    } else {
        panic!("Unsupported arch {}!", target_arch);
    }
}

fn main() {
    let arch = var("CARGO_CFG_TARGET_ARCH").unwrap();

    let text_start = get_text_start();
    println!("cargo:rustc-link-arg=-Tlinkers/{arch}.ld");
    println!("cargo:rustc-link-arg=--defsym=TEXT_START={text_start}");

    let commit_hash = Command::new("git").args(["rev-parse", "HEAD"]).output().unwrap();
    let commit_hash = String::from_utf8(commit_hash.stdout).unwrap();
    println!("cargo:rustc-env=GIT_COMMIT={}", commit_hash.trim());

    // note: add error checking yourself.
    let build_time = chrono::offset::Local::now().format("%Y-%m-%d %H:%M:%S %Z");
    println!("cargo:rustc-env=BUILD_TIME={}", build_time);
    let hostname = gethostname::gethostname();
    println!("cargo:rustc-env=HOSTNAME={}", hostname.into_string().unwrap());
    built::write_built_file().expect("Failed to acquire build-time information");
}
