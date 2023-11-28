// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

#![no_std]
#![no_main]
#![feature(core_intrinsics)]
#![feature(alloc_error_handler)]
#![feature(extract_if)]
#![feature(inline_const)]
#![feature(naked_functions)]
#![feature(asm_const)]
#![feature(error_in_core)]
#![feature(slice_group_by)]
#![feature(c_str_literals)]
#![allow(unused_doc_comments)]
#![allow(special_module_name)]

#[macro_use]
extern crate alloc;
extern crate fdt;
#[macro_use]
extern crate log;
#[macro_use]
extern crate memoffset;

use device::init_vm0_dtb;
use kernel::{cpu_init, interrupt_init, mem_init, timer_init};
use mm::heap_init;
use vmm::{vm_init, vmm_boot_vm};

use crate::kernel::{cpu_sched_init, iommu_init};

#[macro_use]
mod macros;

#[allow(dead_code)]
mod arch;
#[allow(dead_code)]
mod board;
#[allow(dead_code)]
mod config;
#[allow(dead_code)]
mod device;
#[allow(dead_code)]
mod driver;
#[allow(dead_code)]
mod kernel;
#[allow(dead_code)]
mod mm;
#[allow(dead_code)]
mod panic;
#[allow(dead_code)]
mod utils;
#[allow(dead_code)]
mod vmm;

mod error;

// use lib::{BitAlloc, BitAlloc256};

pub static SYSTEM_FDT: spin::Once<alloc::vec::Vec<u8>> = spin::Once::new();

mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

fn print_built_info() {
    println!(
        "Welcome to {} {} {}!",
        env!("BOARD"),
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );
    println!(
        "Built at {build_time} by {hostname}\nCompiler: {rustc_version}\nFeatures: {features:?}\nCommit: {commit_hash}",
        build_time = env!("BUILD_TIME"),
        hostname = env!("HOSTNAME"),
        commit_hash = env!("GIT_COMMIT"),
        rustc_version = built_info::RUSTC_VERSION,
        features = built_info::FEATURES_LOWERCASE_STR,
    );
}

#[no_mangle]
pub fn init(cpu_id: usize, dtb: *mut fdt::myctypes::c_void) {
    // #[cfg(feature="qemu")]
    // board::Platform::parse_dtb(dtb);

    if cpu_id == 0 {
        print_built_info();

        #[cfg(feature = "pi4")]
        {
            crate::driver::gpio_select_function(0, 4);
            crate::driver::gpio_select_function(1, 4);
        }

        heap_init();
        kernel::logger_init().unwrap();
        mem_init();
        init_vm0_dtb(dtb).unwrap();
        iommu_init();
    }
    cpu_init();
    interrupt_init();
    timer_init();
    cpu_sched_init();

    #[cfg(not(feature = "secondary_start"))]
    crate::utils::barrier();

    if cpu_id != 0 {
        crate::kernel::cpu_idle();
    }
    vm_init();
    info!(
        "{} Hypervisor init ok\n\nStart booting Monitor VM ...",
        env!("CARGO_PKG_NAME")
    );
    vmm_boot_vm(0);

    loop {
        core::hint::spin_loop();
    }
}

pub fn secondary_init(mpidr: usize) {
    cpu_init();
    interrupt_init();
    timer_init();
    cpu_sched_init();
    use crate::arch::guest_cpu_on;
    guest_cpu_on(mpidr);
    loop {
        core::hint::spin_loop();
    }
}
