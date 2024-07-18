// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use alloc::string::String;
use alloc::vec::Vec;
use alloc::sync::Arc;

use crate::arch::is_boot_core;
use crate::arch::{PTE_S2_DEVICE, PTE_S2_NORMAL, PTE_S2_NORMALNOCACHE};
#[cfg(not(feature = "gicv3"))]
use crate::board::*;
use crate::config::vm_cfg_entry;
use crate::config::VmConfigEntry;
use crate::device::create_fdt;
use crate::device::EmuDeviceType::*;
use crate::kernel::iommmu_vm_init;
use crate::kernel::ipi_send_msg;
use crate::kernel::IpiVmmPercoreMsg;
use crate::kernel::{add_async_used_info, cpu_idle, current_cpu, VmPa, VmType, iommu_add_device, IpiType, IpiInnerMsg};
use crate::kernel::mem_vm_region_alloc;
#[cfg(target_arch = "riscv64")]
use crate::kernel::mem_pages_alloc_align;
#[cfg(target_arch = "aarch64")]
use crate::kernel::mem_page_alloc;
use crate::kernel::Vm;
use crate::kernel::{active_vcpu_id, vcpu_run};
use crate::kernel::interrupt_vm_register;
use crate::kernel::VM_NUM_MAX;
use crate::utils::trace;
use crate::error::Result;
use crate::vmm::VmmPercoreEvent;
use fdt::*;

use fdt::binding::*;

#[cfg(feature = "ramdisk")]
pub static CPIO_RAMDISK: &[u8] = include_bytes!("../../image/net_rootfs.cpio");
#[cfg(not(feature = "ramdisk"))]
pub static CPIO_RAMDISK: &[u8] = &[];

fn vmm_init_memory(vm: Arc<Vm>) -> bool {
    let vm_id = vm.id();
    let config = vm.config();

    // The aarch64 root page table only needs to allocate one page
    #[cfg(target_arch = "aarch64")]
    let result = mem_page_alloc();

    // The riscv64 root page table needs to be allocated 4 consecutive 16KB aligned pages
    #[cfg(target_arch = "riscv64")]
    let result = mem_pages_alloc_align(4, 4);

    if let Ok(pt_dir_frame) = result {
        vm.set_pt(pt_dir_frame);
        vm.set_mem_region_num(config.memory_region().len());
    } else {
        error!("vmm_init_memory: page alloc failed");
        return false;
    }

    for vm_region in config.memory_region() {
        let pa = mem_vm_region_alloc(vm_region.length);

        if pa == 0 {
            error!("vmm_init_memory: vm memory region is not large enough");
            return false;
        }

        info!(
            "VM {} memory region: ipa=<0x{:x}>, pa=<0x{:x}>, size=<0x{:x}>",
            vm_id, vm_region.ipa_start, pa, vm_region.length
        );
        vm.pt_map_range(vm_region.ipa_start, vm_region.length, pa, PTE_S2_NORMAL, false);
        vm.add_region(VmPa {
            pa_start: pa,
            pa_length: vm_region.length,
            offset: vm_region.ipa_start as isize - pa as isize,
        });
        info!("successfully add a region!");
    }

    true
}

pub fn vmm_load_image(vm: &Vm, bin: &[u8]) {
    let size = bin.len();
    let config = vm.config();
    let load_ipa = config.kernel_load_ipa();
    for (idx, region) in config.memory_region().iter().enumerate() {
        if load_ipa < region.ipa_start || load_ipa + size > region.ipa_start + region.length {
            continue;
        }

        let offset = load_ipa - region.ipa_start;
        info!(
            "VM {} loads kernel: ipa=<0x{:x}>, pa=<0x{:x}>, size=<{}K>",
            vm.id(),
            load_ipa,
            vm.pa_start(idx) + offset,
            size / 1024
        );
        if trace() && vm.pa_start(idx) + offset < 0x1000 {
            panic!("illegal addr {:x}", vm.pa_start(idx) + offset);
        }
        // SAFETY:
        // The 'vm.pa_start(idx) + offset' is in range of our memory configuration.
        // The 'size' is the length of Image binary.
        let dst = unsafe { core::slice::from_raw_parts_mut((vm.pa_start(idx) + offset) as *mut u8, size) };
        dst.clone_from_slice(bin);

        trace!(
            "image dst bytes: {:#x} {:#x} {:#x} {:#x}",
            dst[0],
            dst[1],
            dst[2],
            dst[3]
        );

        return;
    }
    panic!("vmm_load_image: Image config conflicts with memory config");
}

fn overlay_fdt(vm: &Vm, dtb: &[u8], overlay: &mut [u8]) -> Result<FdtBuf> {
    let fdt = Fdt::from_bytes(dtb)?;
    debug!("VM[{}] dtb old size {}", vm.id(), fdt.len());
    let mut buf = FdtBuf::from_fdt_capacity(fdt, (dtb.len() + overlay.len()) * 2)?;
    let fdt_overlay = Fdt::from_bytes_mut(overlay)?;
    buf.overlay_apply(fdt_overlay)?;
    buf.pack()?;
    debug!("VM[{}] dtb new size {}", vm.id(), buf.len());
    Ok(buf)
}

pub fn vmm_init_image(vm: &Vm) -> bool {
    let vm_id = vm.id();
    let config = vm.config();

    if config.kernel_load_ipa() == 0 {
        error!("vmm_init_image: kernel load ipa is null");
        return false;
    }

    // Only load MVM kernel image "L4T" from binding.
    // Load GVM kernel image from shyper-cli, you may check it for more information.
    if config.os_type == VmType::VmTOs {
        match vm.config().kernel_img_name() {
            Some(name) => {
                #[cfg(feature = "tx2")]
                if name == "L4T" {
                    info!("MVM {} loading Image", vm.id());
                    // vmm_load_image(vm.clone(), include_bytes!("../../image/L4T"));
                    extern "C" {
                        fn _binary_vm0img_start();
                        fn _binary_vm0img_size();
                    }
                    // SAFETY:
                    // The '_binary_vm0img_start' and '_binary_vm0img_size' are valid from linker script.
                    let vm0image = unsafe {
                        core::slice::from_raw_parts(
                            _binary_vm0img_start as usize as *const u8,
                            _binary_vm0img_size as usize,
                        )
                    };
                    vmm_load_image(vm, vm0image);
                } else if name == "Image_vanilla" {
                    info!("VM {} loading default Linux Image", vm.id());
                    #[cfg(feature = "static-config")]
                    vmm_load_image(vm, include_bytes!("../../image/Image_vanilla"));
                    #[cfg(not(feature = "static-config"))]
                    info!("*** Please enable feature `static-config`");
                } else {
                    warn!("Image {} is not supported", name);
                }
                #[cfg(any(feature = "qemu", feature = "pi4"))]
                if name.is_empty() {
                    panic!("kernel image name empty")
                } else {
                    extern "C" {
                        fn _binary_vm0img_start();
                        fn _binary_vm0img_size();
                    }
                    info!("MVM {} loading Image", vm.id());
                    // SAFETY:
                    // The '_binary_vm0img_start' and '_binary_vm0img_size' are valid from linker script.
                    let vm0image = unsafe {
                        core::slice::from_raw_parts(
                            _binary_vm0img_start as usize as *const u8,
                            _binary_vm0img_size as usize,
                        )
                    };
                    vmm_load_image(vm, vm0image);
                }
                #[cfg(feature = "rk3588")]
                if name == "Linux-5.10" {
                    info!("MVM {} loading Image", vm.id());
                    extern "C" {
                        fn _binary_vm0img_start();
                        fn _binary_vm0img_size();
                    }
                    // SAFETY:
                    // The '_binary_vm0img_start' and '_binary_vm0img_size' are valid from linker script.
                    let vm0image = unsafe {
                        core::slice::from_raw_parts(
                            _binary_vm0img_start as usize as *const u8,
                            _binary_vm0img_size as usize,
                        )
                    };
                    vmm_load_image(vm, vm0image);
                } else if name == "Image_vanilla" {
                    info!("VM {} loading default Linux Image", vm.id());
                    #[cfg(feature = "static-config")]
                    vmm_load_image(vm, include_bytes!("../../image/Image_vanilla"));
                    #[cfg(not(feature = "static-config"))]
                    info!("*** Please enable feature `static-config`");
                } else {
                    panic!("kernel image name empty")
                }
            }
            None => {
                // nothing to do, its a dynamic configuration
            }
        }
    }

    if config.device_tree_load_ipa() != 0 {
        // Init dtb for Linux.
        if vm_id == 0 {
            // Init dtb for MVM.
            use crate::SYSTEM_FDT;
            let offset = config.device_tree_load_ipa() - config.memory_region()[0].ipa_start;
            // SAFETY:
            // Offset is computed from config.device_tree_load_ipa() and config.memory_region()[0].ipa_start which are both valid.
            // The 'vm.pa_start(0) + offset' is in range of our memory configuration.
            // The 'dtb' have been set to vm.pa_start(0) + offset which is in range of our memory configuration.
            unsafe {
                let src = SYSTEM_FDT.get().unwrap();
                let len = src.len();
                trace!("fdt_len: {:08x}", len);
                let dst = core::slice::from_raw_parts_mut((vm.pa_start(0) + offset) as *mut u8, len);
                dst.clone_from_slice(src);
                vmm_setup_fdt(vm.config(), dst.as_mut_ptr() as *mut _);
            }
        } else {
            // Init dtb for GVM.
            match create_fdt(config) {
                Ok(dtb) => {
                    let mut overlay = config.fdt_overlay.clone();
                    let offset = config.device_tree_load_ipa() - vm.config().memory_region()[0].ipa_start;
                    let target = (vm.pa_start(0) + offset) as *mut u8;
                    debug!(
                        "GVM[{}] dtb addr 0x{:x} overlay {}",
                        vm.id(),
                        target as usize,
                        overlay.len()
                    );
                    if overlay.is_empty() {
                        // SAFETY:
                        // The 'target' is in range of our memory configuration.
                        // The 'src' is a temporary buffer and is valid.
                        unsafe {
                            core::ptr::copy_nonoverlapping(dtb.as_ptr(), target, dtb.len());
                        }
                    } else {
                        let buf = match overlay_fdt(vm, &dtb, &mut overlay) {
                            Ok(x) => x,
                            Err(e) => {
                                error!("overlay_fdt failed: {:?}", e);
                                return false;
                            }
                        };
                        overlay.clear();
                        overlay.shrink_to_fit();
                        // SAFETY:
                        // The 'target' is in range of our memory configuration.
                        // The 'buf' is a vaild value from stack.
                        unsafe {
                            core::ptr::copy_nonoverlapping(buf.as_ptr(), target, buf.len());
                        }
                    }
                }
                Err(err) => {
                    panic!("vmm_setup_config: create fdt for vm{} fail, err: {}", vm.id(), err);
                }
            }
        }
    } else {
        warn!(
            "VM {} id {} device tree load ipa is not set",
            vm_id,
            vm.config().vm_name()
        );
    }

    // ...
    // Todo: support loading ramdisk from MVM shyper-cli.
    // ...
    if config.ramdisk_load_ipa() != 0 {
        info!("VM {} use ramdisk CPIO_RAMDISK", vm_id);
        let offset = config.ramdisk_load_ipa() - config.memory_region()[0].ipa_start;
        let len = CPIO_RAMDISK.len();
        // SAFETY:
        // The 'vm.pa_start(0) + offset' is in range of our memory configuration.
        // The 'len' is the length of CPIO_RAMDISK binary.
        let dst = unsafe { core::slice::from_raw_parts_mut((vm.pa_start(0) + offset) as *mut u8, len) };
        dst.clone_from_slice(CPIO_RAMDISK);
    }

    true
}

fn vmm_init_passthrough_device(vm: Arc<Vm>) -> bool {
    for region in vm.config().passthrough_device_regions() {
        // TODO: specify the region property more accurately.
        // The 'dev_property' in a device region means cacheable here.
        if region.dev_property {
            vm.pt_map_range(region.ipa, region.length, region.pa, PTE_S2_DEVICE, true);
        } else {
            vm.pt_map_range(region.ipa, region.length, region.pa, PTE_S2_NORMALNOCACHE, true);
        }

        debug!(
            "VM {} registers passthrough device: ipa=<0x{:x}>, pa=<0x{:x}>, size=<0x{:x}>, {}",
            vm.id(),
            region.ipa,
            region.pa,
            region.length,
            if region.dev_property { "device" } else { "normal" }
        );
    }
    for irq in vm.config().passthrough_device_irqs() {
        if !interrupt_vm_register(&vm, *irq) {
            return false;
        }
    }
    true
}

fn vmm_init_iommu_device(vm: Arc<Vm>) -> bool {
    for emu_cfg in vm.config().emulated_device_list().iter() {
        if emu_cfg.emu_type == EmuDeviceTIOMMU {
            if !iommmu_vm_init(&vm) {
                return false;
            } else {
                break;
            }
        }
    }
    for stream_id in vm.config().passthrough_device_stread_ids() {
        if *stream_id == 0 {
            break;
        }
        if !iommu_add_device(&vm, *stream_id) {
            return false;
        }
    }
    true
}

/// Add a virtio node to fdt for riscv64
/// # Safety:
/// 1. 'dtb' is a valid pointer to a device tree blob
/// 2. 'name' is a string not too long
/// 3. 'irq_id' is a valid interrupt id
/// 4. 'base_ipa' is a valid ipa
unsafe fn fdt_add_virtio_riscv64(
    dtb: *mut fdt::myctypes::c_void,
    name: String,
    irq_id: u32,
    base_ipa: u64,
    length: u64,
) {
    let node = fdt_create_node(dtb, "/soc\0".as_ptr(), name.as_ptr());
    if node < 0 {
        panic!("fdt_create_node failed {}", node);
    }

    let ret = fdt_add_property_u32(dtb, node, "interrupts\0".as_ptr(), irq_id);
    if ret < 0 {
        panic!("fdt_add_property_u32 failed {}", ret);
    }

    let ret = fdt_add_property_u32(
        dtb,
        node,
        "interrupt-parent\0".as_ptr(),
        9_u32, // plic phandle id
    );
    if ret < 0 {
        panic!("fdt_add_property_u32 failed {}", ret);
    }

    let mut regs = [base_ipa, length];
    let ret = fdt_add_property_u64_array(dtb, node, "reg\0".as_ptr(), regs.as_mut_ptr(), 2);
    if ret < 0 {
        panic!("fdt_add_property_u64_array failed {}", ret);
    }

    fdt_add_property_string(dtb, node, "compatible\0".as_ptr(), "virtio,mmio\0".as_ptr());
    trace!("fdt_add_virtio: {} irq = {}", name, irq_id);
}

/// Add a vm_service node to fdt for riscv64
/// # Safety:
/// 1. 'dtb' is a valid pointer to a device tree blob
/// 2. 'irq_id' is a valid interrupt id
/// 3. 'base_ipa' is a valid ipa
unsafe fn fdt_add_vm_service_riscv64(dtb: *mut fdt::myctypes::c_void, irq_id: u32, base_ipa: u64, length: u64) {
    let node = fdt_create_node(dtb, "/soc\0".as_ptr(), "vm_service\0".as_ptr());
    if node < 0 {
        panic!("fdt_create_node failed {}", node);
    }

    let ret = fdt_add_property_string(dtb, node, "compatible\0".as_ptr(), "shyper\0".as_ptr());
    if ret < 0 {
        panic!("fdt_add_property_string failed {}", ret);
    }

    let ret = fdt_add_property_u32(dtb, node, "interrupts\0".as_ptr(), irq_id);
    if ret < 0 {
        panic!("fdt_add_property_u32 failed {}", ret);
    }

    let mut regs = [base_ipa, length];
    let ret = fdt_add_property_u64_array(dtb, node, "reg\0".as_ptr(), regs.as_mut_ptr(), 2);
    if ret < 0 {
        panic!("fdt_add_property_u64_array failed {}", ret);
    }

    let ret = fdt_add_property_u32(
        dtb,
        node,
        "interrupt-parent\0".as_ptr(),
        9_u32, // plic phandle id
    );
    if ret < 0 {
        panic!("fdt_add_property_u32 failed {}", ret);
    }
}

// Here is used to write vm0 edit fdt function, mainly used to add virtual fdt item
/// # Safety:
/// This function is unsafe because it trusts the caller to pass a valid pointer to a valid dtb.
/// So the caller must ensure that the vm.dtb() have configured correctly before calling this function.
pub unsafe fn vmm_setup_fdt(config: &VmConfigEntry, dtb: *mut fdt::myctypes::c_void) {
    use fdt::*;
    let mut mr = Vec::new();
    for r in config.memory_region() {
        mr.push(region {
            ipa_start: r.ipa_start as u64,
            length: r.length as u64,
        });
    }
    #[cfg(feature = "tx2")]
    fdt_set_memory(dtb, mr.len() as u64, mr.as_ptr(), "memory@90000000\0".as_ptr());
    #[cfg(feature = "pi4")]
    fdt_set_memory(dtb, mr.len() as u64, mr.as_ptr(), "memory@200000\0".as_ptr());
    #[cfg(all(feature = "qemu", target_arch = "aarch64"))]
    fdt_set_memory(dtb, mr.len() as u64, mr.as_ptr(), "memory@50000000\0".as_ptr());
    #[cfg(all(feature = "qemu", target_arch = "riscv64"))]
    fdt_set_memory(dtb, mr.len() as u64, mr.as_ptr(), "memory@90000000\0".as_ptr());
    #[cfg(feature = "rk3588")]
    fdt_set_memory(dtb, mr.len() as u64, mr.as_ptr(), "memory@10000000\0".as_ptr());
    // FDT+TIMER
    //fdt_add_timer(dtb, 0x04);
    // FDT+BOOTCMD
    fdt_set_bootcmd(dtb, config.cmdline.as_ptr());
    #[cfg(feature = "tx2")]
    fdt_set_stdout_path(dtb, "/serial@3100000\0".as_ptr());
    #[cfg(feature = "rk3588")]
    fdt_set_stdout_path(dtb, "/serial@feba0000\0".as_ptr());

    if !config.emulated_device_list().is_empty() {
        for emu_cfg in config.emulated_device_list() {
            match emu_cfg.emu_type {
                EmuDeviceTGicd | EmuDeviceTGPPT => {
                    #[cfg(not(feature = "gicv3"))]
                    #[cfg(any(feature = "tx2", feature = "qemu"))]
                    fdt_setup_gic(
                        dtb,
                        Platform::GICD_BASE as u64,
                        Platform::GICC_BASE as u64,
                        emu_cfg.name.as_ptr(),
                    );
                    #[cfg(feature = "pi4")]
                    let _r = fdt_setup_gic(
                        dtb,
                        (Platform::GICD_BASE | 0xF_0000_0000) as u64,
                        (Platform::GICC_BASE | 0xF_0000_0000) as u64,
                        emu_cfg.name.as_ptr(),
                    );
                }
                EmuDeviceTVirtioNet | EmuDeviceTVirtioConsole => {
                    cfg_if::cfg_if! {
                        if #[cfg(all(any(feature = "tx2", feature = "qemu", feature = "rk3588"), target_arch = "aarch64"))] {
                            fdt_add_virtio(
                                dtb,
                                emu_cfg.name.as_ptr(),
                                emu_cfg.irq_id as u32 - 0x20,
                                emu_cfg.base_ipa as u64,
                            );
                            info!("apply aarch64");
                        } else if #[cfg(target_arch = "riscv64")] {
                            fdt_add_virtio_riscv64(
                                dtb,
                                emu_cfg.name.clone(),
                                emu_cfg.irq_id as u32,
                                emu_cfg.base_ipa as u64,
                                emu_cfg.length as u64,
                            );
                        }
                    }
                }
                EmuDeviceTShyper => {
                    // Add vm_service node, in order to provide kernel module information about irq_id
                    info!("fdt add vm_service irq = {}", emu_cfg.irq_id);

                    cfg_if::cfg_if! {
                        if #[cfg(all(any(feature = "tx2", feature = "qemu", feature = "rk3588"), target_arch = "aarch64"))] {
                            fdt_add_vm_service(
                                dtb,
                                emu_cfg.irq_id as u32 - 0x20,
                                emu_cfg.base_ipa as u64,
                                emu_cfg.length as u64,
                            );
                        } else if #[cfg(target_arch = "riscv64")] {
                            fdt_add_vm_service_riscv64(
                                dtb,
                                emu_cfg.irq_id as u32,
                                emu_cfg.base_ipa as u64,
                                emu_cfg.length as u64,
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }
    debug!("after dtb size {}", fdt_size(dtb));
}

/* Setup VM Configuration before boot.
 * Only VM0 will call this function.
 * This func should run 1 time for each vm.
 *
 * @param[in] vm_id: target VM id to set up config.
 */
pub fn vmm_setup_config(vm: Arc<Vm>) {
    let vm_id = vm.id();
    let config = match vm_cfg_entry(vm_id) {
        Some(config) => config,
        None => {
            panic!("vmm_setup_config vm id {} config doesn't exist", vm_id);
        }
    };

    debug!(
        "vmm_setup_config VM[{}] name {:?} current core {}",
        vm_id,
        config.name,
        current_cpu().id
    );

    // need ipi, must after push to global list
    vmm_init_cpu(vm.clone());

    if vm_id >= VM_NUM_MAX {
        panic!("vmm_setup_config: out of vm");
    }
    if !vmm_init_memory(vm.clone()) {
        panic!("vmm_setup_config: vmm_init_memory failed");
    }

    if !vmm_init_image(&vm) {
        panic!("vmm_setup_config: vmm_init_image failed");
    }
    if !vmm_init_passthrough_device(vm.clone()) {
        panic!("vmm_setup_config: vmm_init_passthrough_device failed");
    }
    if !vmm_init_iommu_device(vm.clone()) {
        panic!("vmm_setup_config: vmm_init_iommu_device failed");
    }

    add_async_used_info(vm_id);
    info!("VM {} id {} init ok", vm.id(), vm.config().name);
}

fn vmm_init_cpu(vm: Arc<Vm>) {
    let vm_id = vm.id();
    info!(
        "VM {} init cpu: cores=<{}>, allocat_bits=<{:#b}>",
        vm.id(),
        vm.config().cpu_num(),
        vm.config().cpu_allocated_bitmap()
    );

    for vcpu in vm.vcpu_list() {
        let target_cpu_id = vcpu.phys_id();
        if target_cpu_id != current_cpu().id {
            let m = IpiVmmPercoreMsg {
                vm: vm.clone(),
                event: VmmPercoreEvent::VmmAssignCpu,
            };
            if !ipi_send_msg(target_cpu_id, IpiType::IpiTVMM, IpiInnerMsg::VmmPercoreMsg(m)) {
                error!("vmm_init_cpu: failed to send ipi to Core {}", target_cpu_id);
            }
        } else {
            vmm_assign_vcpu_percore(&vm);
        }
    }

    info!("vmm_init_cpu: VM [{}] is ready", vm_id);
}

pub fn vmm_assign_vcpu_percore(vm: &Vm) {
    let cpu_id = current_cpu().id;
    if current_cpu().assigned() {
        debug!("vmm_cpu_assign_vcpu vm[{}] cpu {} is assigned", vm.id(), cpu_id);
    }

    for vcpu in vm.vcpu_list() {
        if vcpu.phys_id() == current_cpu().id {
            if vcpu.id() == 0 {
                info!("* Core {} is assigned => vm {}, vcpu {}", cpu_id, vm.id(), vcpu.id());
            } else {
                info!("Core {} is assigned => vm {}, vcpu {}", cpu_id, vm.id(), vcpu.id());
            }
            current_cpu().vcpu_array.append_vcpu(vcpu.clone());
            break;
        }
    }
}

pub fn vm_init() {
    if is_boot_core(current_cpu().id) {
        // Set up basic config.
        crate::config::mvm_config_init();
        // Add VM 0
        super::vmm_init_gvm(0);
        #[cfg(feature = "static-config")]
        {
            #[cfg(not(feature = "gicv3"))]
            crate::config::init_tmp_config_for_vm1();
            #[cfg(feature = "gicv3")]
            crate::config::init_gicv3_config_for_vm1();
            super::vmm_init_gvm(1);
        }
    }
}

pub fn vmm_boot() {
    if current_cpu().assigned() && active_vcpu_id() == 0 {
        info!("Core {} start running", current_cpu().id);
        vcpu_run(false);
    } else {
        // If there is no available vm(vcpu), just go idle
        info!("Core {} idle", current_cpu().id);
        cpu_idle();
    }
}
