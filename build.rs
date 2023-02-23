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

fn main() {
    println!("cargo:rustc-link-search=native={}/lib", var("PWD").unwrap());
    println!("cargo:rustc-link-lib=static=fdt-binding");
    // note: add error checking yourself.
    let output = Command::new("date").arg("+\"%Y-%m-%d %H:%M:%S %Z\"").output().unwrap();
    let build_time = String::from_utf8(output.stdout).unwrap();
    println!("cargo:rustc-env=BUILD_TIME={}", build_time);
}
