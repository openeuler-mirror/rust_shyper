// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

//! Rust-Shyper is a type-1 hypervisor based on Rust. It now supports Aarch64 architecture.
//! The introduces of all modules are showed below:
//! * [arch]: The architecture-dependent code, including the code for the Aarch64 architecture.
//! * [board]: The code for the specific board, including the code for the Raspberry Pi 4B, tx2 and so on.
//! * [config]: The configuration file for the hypervisor.
//! * [device]: The emulation of the devices, including virtio emulation.
//! * [driver]: The driver for the devices, including gpio and uart.
//! * [kernel]: The rust-shyper hypervisor kernel code, manages the virtual machines, interrupts, address translation, vcpu scheduling and so on.
//! * [mm]: The memory management code, including the code for the page frame allocator and rust global allocator.
//! * [utils]: The utility code, including the code for the barrier, bitmap and so on.
//! * [vmm]: The virtual machine monitor code, including the code for virtual machine management such as creation, startup, shutdown and removal.
//! * [macros]: Defines the macros for the hypervisor.
//! * error: Defines the error type for the hypervisor.
//! * panic: Defines the panic handler for the hypervisor.

#![no_std]
#![no_main]
#![feature(core_intrinsics)]
#![feature(alloc_error_handler)]
#![feature(extract_if)]
#![feature(inline_const)]
#![feature(naked_functions)]
#![feature(stdsimd)]
// use hlv.w instr
#![cfg_attr(target_arch = "riscv64", feature(riscv_ext_intrinsics))]
#![feature(asm_const)]
#![feature(error_in_core)]
#![feature(slice_group_by)]
#![feature(c_str_literals)]
#![allow(unused_doc_comments)]
#![allow(special_module_name)]
#![allow(clippy::enum_variant_names)]
#![allow(clippy::module_inception)]
#![allow(clippy::wrong_self_convention)]
#![allow(clippy::mut_from_ref)]
#![allow(clippy::upper_case_acronyms)]
#![allow(clippy::modulo_one)]
#![allow(clippy::needless_range_loop)]

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

#[cfg(feature = "doc")]
#[macro_use]
pub mod macros;
#[allow(dead_code)]
#[cfg(feature = "doc")]
pub mod arch;
#[allow(dead_code)]
#[cfg(feature = "doc")]
pub mod board;
#[allow(dead_code)]
#[cfg(feature = "doc")]
pub mod config;
#[allow(dead_code)]
#[cfg(feature = "doc")]
pub mod device;
#[allow(dead_code)]
#[cfg(feature = "doc")]
pub mod driver;
#[allow(dead_code)]
#[cfg(feature = "doc")]
pub mod kernel;
#[allow(dead_code)]
#[cfg(feature = "doc")]
pub mod mm;
#[allow(dead_code)]
#[cfg(feature = "doc")]
pub mod panic;
#[allow(dead_code)]
#[cfg(feature = "doc")]
pub mod utils;
#[allow(dead_code)]
#[cfg(feature = "doc")]
pub mod vmm;

#[cfg(feature = "doc")]
pub mod error;
#[cfg(feature = "doc")]
pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

#[macro_use]
#[cfg(not(feature = "doc"))]
mod macros;
#[allow(dead_code)]
#[cfg(not(feature = "doc"))]
mod arch;
#[allow(dead_code)]
#[cfg(not(feature = "doc"))]
mod board;
#[allow(dead_code)]
#[cfg(not(feature = "doc"))]
mod config;
#[allow(dead_code)]
#[cfg(not(feature = "doc"))]
mod device;
#[allow(dead_code)]
#[cfg(not(feature = "doc"))]
mod driver;
#[cfg(not(feature = "doc"))]
mod error;
#[allow(dead_code)]
#[cfg(not(feature = "doc"))]
mod kernel;
#[allow(dead_code)]
#[cfg(not(feature = "doc"))]
mod mm;
#[allow(dead_code)]
#[cfg(not(feature = "doc"))]
mod panic;
#[allow(dead_code)]
#[cfg(not(feature = "doc"))]
mod utils;
#[allow(dead_code)]
#[cfg(not(feature = "doc"))]
mod vmm;
#[cfg(not(feature = "doc"))]
mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

// use lib::{BitAlloc, BitAlloc256};

pub static SYSTEM_FDT: spin::Once<alloc::vec::Vec<u8>> = spin::Once::new();

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

// Only core 0 will execute this function
#[no_mangle]
pub fn init(dtb: &mut fdt::myctypes::c_void) {
    print_built_info();

    #[cfg(feature = "pi4")]
    {
        crate::driver::gpio_select_function(0, 4);
        crate::driver::gpio_select_function(1, 4);
    }

    heap_init();
    kernel::logger_init().unwrap();
    mem_init();
    // SAFETY:
    // DTB is saved value from boot_stage
    // And it is passed by bootloader
    unsafe {
        init_vm0_dtb(dtb).unwrap();
    }
    iommu_init();
    cpu_init();
    info!("cpu init ok");

    interrupt_init();
    info!("interrupt init ok");

    timer_init();
    cpu_sched_init();
    info!("sched init ok");

    #[cfg(not(feature = "secondary_start"))]
    crate::utils::barrier();

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

// Other cores will execute this function
pub fn secondary_init(mpidr: usize) {
    info!("secondary core {:#x} init", mpidr);
    cpu_init();
    interrupt_init();
    info!("secondary core {:#x} interrupt init", mpidr);
    timer_init();
    cpu_sched_init();

    info!("[boot] sched init ok at core {:#x}", mpidr);

    #[cfg(not(feature = "secondary_start"))]
    crate::utils::barrier();
    use crate::arch::guest_cpu_on;
    guest_cpu_on(mpidr);
    crate::kernel::cpu_idle();
}
