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
#![feature(default_alloc_error_handler)]
#![feature(alloc_error_handler)]
#![feature(const_btree_new)]
#![feature(drain_filter)]
#![feature(inline_const)]
#![feature(naked_functions)]
#![feature(asm_sym)]
#![feature(asm_const)]
#![allow(unused_doc_comments)]
#![allow(special_module_name)]

#[macro_use]
extern crate alloc;
extern crate fdt;
#[macro_use]
// extern crate lazy_static;
extern crate log;
#[macro_use]
extern crate memoffset;

// extern crate rlibc;

use device::{init_vm0_dtb, mediated_dev_init};
use kernel::{cpu_init, interrupt_init, mem_init, timer_init};
use mm::heap_init;
use vmm::{vm_init, vmm_boot_vm};

use crate::kernel::{cpu_sched_init, hvc_init, iommu_init};

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::utils::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

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
        "Built at {build_time} by {hostname}\nCompiler: {rustc_version}\nFeatures: {features:?}",
        build_time = env!("BUILD_TIME"),
        hostname = env!("HOSTNAME"),
        rustc_version = built_info::RUSTC_VERSION,
        features = built_info::FEATURES_LOWERCASE_STR
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
        let _ = kernel::logger_init();
        mem_init();
        init_vm0_dtb(dtb);
        hvc_init();
        iommu_init();
    }
    cpu_init();
    interrupt_init();
    timer_init();
    cpu_sched_init();
    if cpu_id == 0 {
        mediated_dev_init();
    }

    #[cfg(not(feature = "secondary_start"))]
    crate::utils::barrier();

    if cpu_id != 0 {
        crate::kernel::cpu_idle();
    }
    vm_init();
    println!("Rust-Shyper Hypervisor init ok\n\nStart booting Monitor VM ...");
    vmm_boot_vm(0);

    loop {
        core::hint::spin_loop();
    }
}

#[no_mangle]
pub fn secondary_init(mpidr: usize) {
    cpu_init();
    interrupt_init();
    timer_init();
    cpu_sched_init();
    use crate::arch::guest_cpu_on;
    guest_cpu_on(mpidr);
    loop {}
}
