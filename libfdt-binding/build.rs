// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

extern crate bindgen;

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-changed=./libfdt");
    println!("cargo:rerun-if-changed=build.rs");

    // compile libfdt-bingding
    let fdt_dirs = ["./", "./libfdt"];
    let c_files = fdt_dirs.iter().flat_map(|path| {
        std::fs::read_dir(path).unwrap().filter_map(|f| {
            let f = f.unwrap();
            if f.file_type().unwrap().is_file() && matches!(f.path().extension(), Some(ext) if ext == "c") {
                Some(f.path())
            } else {
                None
            }
        })
    });
    cc::Build::new()
        .compiler("aarch64-none-elf-gcc")
        .includes(fdt_dirs)
        .files(c_files)
        .flag("-w")
        .compile("fdt-binding");

    let bindings = bindgen::Builder::default()
        .use_core()
        .ctypes_prefix("myctypes")
        .header("wrapper.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
