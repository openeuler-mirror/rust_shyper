// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use alloc::vec::Vec;

use crate::arch::traits::InterruptController;
use crate::arch::{power_arch_vm_shutdown_secondary_cores, IntCtrl};
use crate::board::PLATFORM_CPU_NUM_MAX;
use crate::config::{vm_id_list, vm_num};
// use crate::config::{init_tmp_config_for_bma1, init_tmp_config_for_bma2, init_tmp_config_for_vm1, init_tmp_config_for_vm2};
use crate::config::NAME_MAX_LEN;
use crate::config::vm_cfg_entry;
use crate::config::vm_type;
use crate::kernel::{
    active_vcpu_id, active_vm, current_cpu, push_vm, vm, Vm, vm_if_get_state, vm_if_set_ivc_arg, vm_if_set_ivc_arg_ptr,
    vm_ipa2pa, VM_NUM_MAX, Scheduler,
};
use crate::kernel::{active_vm_id, vm_if_get_cpu_id};
use crate::kernel::{ipi_send_msg, IpiInnerMsg, IpiMessage, IpiType, IpiVmmMsg};
use crate::kernel::{hvc_send_msg_to_vm, HvcGuestMsg, HvcManageMsg};
use crate::kernel::HVC_CONFIG;
use crate::kernel::HVC_CONFIG_UPLOAD_KERNEL_IMAGE;
use crate::kernel::HVC_VMM;
use crate::kernel::HVC_VMM_REBOOT_VM;
use crate::utils::{bit_extract, memcpy_safe, memset_safe};
use crate::vmm::{vmm_cpu_assign_vcpu, vmm_boot, vmm_init_image, vmm_setup_config, vmm_cpu_remove_vcpu};

#[derive(Copy, Clone)]
pub enum VmmEvent {
    VmmBoot,
    VmmReboot,
    VmmShutdown,
    VmmAssignCpu,
    VmmAssignVcpu,
    VmmRemoveCpu,
}

pub fn vmm_shutdown_secondary_vm() {
    info!("Shutting down all VMs...");
}

/* Generate VM structure and push it to VM.
 *
 * @param[in]  vm_id: new added VM id.
 */
pub fn vmm_push_vm(vm_id: usize) {
    info!("vmm_push_vm: add vm {} on cpu {}", vm_id, current_cpu().id);
    if push_vm(vm_id).is_err() {
        return;
    }
    let vm = vm(vm_id).unwrap();
    let vm_cfg = match vm_cfg_entry(vm_id) {
        Some(vm_cfg) => vm_cfg,
        None => {
            error!("vmm_push_vm: failed to find config for vm {}", vm_id);
            return;
        }
    };
    vm.set_config_entry(Some(vm_cfg));

    use crate::kernel::vm_if_set_type;
    vm_if_set_type(vm_id, vm_type(vm_id));
}

pub fn vmm_alloc_vcpu(vm_id: usize) {
    let vm = match vm(vm_id) {
        None => {
            panic!(
                "vmm_alloc_vcpu: on core {}, VM [{}] is not added yet",
                current_cpu().id,
                vm_id
            );
        }
        Some(vm) => vm,
    };

    for i in 0..vm.config().cpu_num() {
        use crate::kernel::vcpu_alloc;
        if let Some(vcpu) = vcpu_alloc() {
            vcpu.init(vm.clone(), i);
            vm.push_vcpu(vcpu.clone());
        } else {
            error!("failed to allocte vcpu");
            return;
        }
    }

    info!(
        "VM {} init cpu: cores=<{}>, allocat_bits=<0b{:b}>",
        vm.id(),
        vm.config().cpu_num(),
        vm.config().cpu_allocated_bitmap()
    );
}

/* Finish cpu assignment before set up VM config.
 * Only VM0 will go through this function.
 *
 * @param[in] vm_id: new added VM id.
 */
pub fn vmm_set_up_cpu(vm_id: usize) {
    info!("vmm_set_up_cpu: set up vm {} on cpu {}", vm_id, current_cpu().id);
    let vm = match vm(vm_id) {
        None => {
            panic!(
                "vmm_set_up_cpu: on core {}, VM [{}] is not added yet",
                current_cpu().id,
                vm_id
            );
        }
        Some(vm) => vm,
    };

    vmm_alloc_vcpu(vm_id);

    let mut cpu_allocate_bitmap = vm.config().cpu_allocated_bitmap();
    let mut target_cpu_id = 0;
    let mut cpu_num = 0;
    while cpu_allocate_bitmap != 0 && target_cpu_id < PLATFORM_CPU_NUM_MAX {
        if cpu_allocate_bitmap & 1 != 0 {
            info!("vmm_set_up_cpu: vm {} physical cpu id {}", vm_id, target_cpu_id);
            cpu_num += 1;

            if cfg!(feature = "secondary_start") {
                // Judge if current cpu is allocated.
                let vcpu = match vm.select_vcpu2assign(target_cpu_id) {
                    None => panic!("core {} vm {} cannot find proper vcpu to assign", target_cpu_id, vm_id),
                    Some(vcpu) => vcpu,
                };
                if vcpu.id() == 0 {
                    info!(
                        "* Core {} is assigned => vm {}, vcpu {}",
                        target_cpu_id,
                        vm_id,
                        vcpu.id()
                    );
                } else {
                    info!("Core {} is assigned => vm {}, vcpu {}", target_cpu_id, vm_id, vcpu.id());
                }
                vcpu.set_phys_id(target_cpu_id);

                let cfg_master = vm.config().cpu_master();
                if target_cpu_id == cfg_master && target_cpu_id == current_cpu().id {
                    current_cpu().vcpu_array.append_vcpu(vcpu);
                }
            } else if target_cpu_id != current_cpu().id {
                let m = IpiVmmMsg {
                    vmid: vm_id,
                    event: VmmEvent::VmmAssignCpu,
                };
                if !ipi_send_msg(target_cpu_id, IpiType::IpiTVMM, IpiInnerMsg::VmmMsg(m)) {
                    error!("vmm_set_up_cpu: failed to send ipi to Core {}", target_cpu_id);
                }
            } else {
                vmm_cpu_assign_vcpu(vm_id);
            }
        }
        cpu_allocate_bitmap >>= 1;
        target_cpu_id += 1;
    }
    info!(
        "vmm_set_up_cpu: vm {} total physical cpu num {} bitmap {:#b}",
        vm_id,
        cpu_num,
        vm.config().cpu_allocated_bitmap()
    );

    // Waiting till others set up.
    info!(
        "vmm_set_up_cpu: on core {}, waiting VM [{}] to be set up",
        current_cpu().id,
        vm_id
    );
    #[cfg(not(feature = "secondary_start"))]
    while !vm.ready() {
        use crate::utils::sleep;
        sleep(10);
    }
    info!("vmm_set_up_cpu: VM [{}] is ready", vm_id);
}

/* Init VM before boot.
 * Only VM0 will call this function.
 *
 * @param[in] vm_id: target VM id to boot.
 */
pub fn vmm_init_gvm(vm_id: usize) {
    // Before boot, we need to set up the VM config.
    if current_cpu().id == 0 || (active_vm_id() == 0 && active_vm_id() != vm_id) {
        vmm_push_vm(vm_id);

        vmm_set_up_cpu(vm_id);

        vmm_setup_config(vm_id);
    } else {
        error!(
            "VM[{}] Core {} should not init VM [{}]",
            active_vm_id(),
            current_cpu().id,
            vm_id
        );
    }
}

/* Boot Guest VM.
 *
 * @param[in] vm_id: target VM id to boot.
 */
pub fn vmm_boot_vm(vm_id: usize) {
    let phys_id = vm_if_get_cpu_id(vm_id);
    if phys_id != current_cpu().id {
        use crate::kernel::{CPU_IF_LIST, CpuState};
        use crate::arch::psci_vm_maincpu_on;
        let state = CPU_IF_LIST.lock().get(phys_id).unwrap().state_for_start;
        if state == CpuState::CpuInv {
            let vmpidr = vm(vm_id).unwrap().pcpuid_to_vcpuid(phys_id).unwrap();
            info!("now start cpu on! vmpidr={vmpidr}");
            psci_vm_maincpu_on(vmpidr, vmm_boot_vm as usize, 0, vm_id);
        } else {
            let m = IpiVmmMsg {
                vmid: vm_id,
                event: VmmEvent::VmmBoot,
            };
            if !ipi_send_msg(phys_id, IpiType::IpiTVMM, IpiInnerMsg::VmmMsg(m)) {
                error!("vmm_boot_vm: failed to send ipi to Core {}", phys_id);
            }
        }
    } else {
        if current_cpu().vcpu_array.pop_vcpu_through_vmid(vm_id).is_none() {
            let vm = vm(vm_id).unwrap();
            let vcpuid = vm.pcpuid_to_vcpuid(phys_id).unwrap();
            let vcpu = vm.vcpuid_to_vcpu(vcpuid);
            current_cpu().vcpu_array.append_vcpu(vcpu.unwrap());
        };
        let vcpu = current_cpu().vcpu_array.pop_vcpu_through_vmid(vm_id).unwrap();
        IntCtrl::clear();
        // TODO: try to use `wakeup` (still bugs when booting multi-shared-core VM using wakeup)
        current_cpu().scheduler().yield_to(vcpu);
        vmm_boot();
    }
}

/**
 * Reboot target vm according to arguments
 *
 * @param arg force ~ (31, 16) ~ [soft shutdown or hard shutdown]
 *            vmid ~ (15, 0) ~ [target vm id]
 */
pub fn vmm_reboot_vm(arg: usize) {
    let vm_id = bit_extract(arg, 0, 16);
    let force = bit_extract(arg, 16, 16) != 0;
    let cur_vm = active_vm().unwrap();

    info!("vmm_reboot VM [{}] force:{}", vm_id, force);

    if force {
        if cur_vm.id() == vm_id {
            vmm_reboot();
        } else {
            let cpu_trgt = vm_if_get_cpu_id(vm_id);
            let m = IpiVmmMsg {
                vmid: vm_id,
                event: VmmEvent::VmmReboot,
            };
            if !ipi_send_msg(cpu_trgt, IpiType::IpiTVMM, IpiInnerMsg::VmmMsg(m)) {
                error!("vmm_reboot_vm: failed to send ipi to Core {}", cpu_trgt);
            }
        }
        return;
    }

    let msg = HvcManageMsg {
        fid: HVC_VMM,
        event: HVC_VMM_REBOOT_VM,
        vm_id,
    };
    if !hvc_send_msg_to_vm(vm_id, &HvcGuestMsg::Manage(msg)) {
        error!("vmm_reboot_vm: failed to notify VM 0");
    }
}

/* Reset vm os at current core.
 *
 * @param[in] vm : target VM structure to be reboot.
 */
pub fn vmm_reboot() {
    let vm = active_vm().unwrap();
    // If running MVM, reboot the whole system.
    if vm.id() == 0 {
        vmm_shutdown_secondary_vm();
        use crate::board::{PlatOperation, Platform};
        Platform::sys_reboot();
    }

    // Reset GVM.
    let vcpu = current_cpu().active_vcpu.clone().unwrap();
    info!("VM [{}] reset...", vm.id());
    power_arch_vm_shutdown_secondary_cores(vm.clone());
    info!(
        "Core {} (VM [{}] vcpu {}) shutdown ok",
        current_cpu().id,
        vm.id(),
        active_vcpu_id()
    );

    // Clear memory region.
    for idx in 0..vm.mem_region_num() {
        info!(
            "Core {} (VM [{}] vcpu {}) reset mem region start {:x} size {:x}",
            current_cpu().id,
            vm.id(),
            active_vcpu_id(),
            vm.pa_start(idx),
            vm.pa_length(idx)
        );
        memset_safe(vm.pa_start(idx) as *mut u8, 0, vm.pa_length(idx));
    }

    // Reset image.
    if !vmm_init_image(vm.clone()) {
        panic!("vmm_reboot: vmm_init_image failed");
    }

    // Reset ivc arg.
    vm_if_set_ivc_arg(vm.id(), 0);
    vm_if_set_ivc_arg_ptr(vm.id(), 0);

    IntCtrl::clear();
    crate::arch::vcpu_arch_init(vm.clone(), vm.vcpu(0).unwrap());
    vcpu.reset_context();

    vmm_load_image_from_mvm(vm);

    // vcpu_run();
}

pub fn vmm_load_image_from_mvm(vm: Vm) {
    let vm_id = vm.id();
    let msg = HvcManageMsg {
        fid: HVC_CONFIG,
        event: HVC_CONFIG_UPLOAD_KERNEL_IMAGE,
        vm_id,
    };
    // println!("mediated_blk_write send msg to vm0");
    if !hvc_send_msg_to_vm(0, &HvcGuestMsg::Manage(msg)) {
        error!("vmm_load_image_from_mvm: failed to notify VM 0");
    }
}

/* Get current VM id.
 *
 * @param[in] id_ipa : vm id ipa.
 */
pub fn get_vm_id(id_ipa: usize) -> bool {
    let vm = active_vm().unwrap();
    let id_pa = vm_ipa2pa(vm.clone(), id_ipa);
    if id_pa == 0 {
        error!("illegal id_pa {:x}", id_pa);
        return false;
    }
    unsafe {
        *(id_pa as *mut usize) = vm.id();
    }
    true
}

#[repr(C)]
struct VMInfo {
    pub id: u32,
    pub vm_name: [u8; NAME_MAX_LEN],
    pub vm_type: u32,
    pub vm_state: u32,
}

#[repr(C)]
struct VMInfoList {
    pub vm_num: usize,
    pub info_list: [VMInfo; VM_NUM_MAX],
}

/* List VM info in hypervisor.
 *
 * @param[in] vm_info_ipa : vm info list ipa.
 */
pub fn vmm_list_vm(vm_info_ipa: usize) -> Result<usize, ()> {
    let vm_info_pa = vm_ipa2pa(active_vm().unwrap(), vm_info_ipa);
    if vm_info_pa == 0 {
        error!("illegal vm_info_ipa {:x}", vm_info_ipa);
        return Err(());
    }

    let vm_info = unsafe { &mut *(vm_info_pa as *mut VMInfoList) };

    // Get VM num.
    vm_info.vm_num = vm_num();

    for (idx, vmid) in vm_id_list().iter().enumerate() {
        let vm_cfg = match vm_cfg_entry(*vmid) {
            Some(vm_cfg) => vm_cfg,
            None => {
                error!("Failed to get VM config entry for VM[{}]", *vmid);
                continue;
            }
        };
        // Get VM type.
        let vm_type = vm_type(*vmid);
        // Get VM State.
        let vm_state = vm_if_get_state(*vmid);

        vm_info.info_list[idx].id = *vmid as u32;
        vm_info.info_list[idx].vm_type = vm_type as u32;
        vm_info.info_list[idx].vm_state = vm_state as u32;

        let vm_name_u8: Vec<u8> = vm_cfg.vm_name().as_bytes().to_vec();
        memcpy_safe(
            vm_info.info_list[idx].vm_name.as_ptr(),
            vm_name_u8.as_ptr(),
            NAME_MAX_LEN,
        );
        vm_info.info_list[idx].vm_name[vm_name_u8.len()] = 0;
    }
    Ok(0)
}

pub fn vmm_ipi_handler(msg: &IpiMessage) {
    match msg.ipi_message {
        IpiInnerMsg::VmmMsg(vmm) => match vmm.event {
            VmmEvent::VmmBoot => {
                vmm_boot_vm(vmm.vmid);
            }
            VmmEvent::VmmReboot => {
                vmm_reboot();
            }
            VmmEvent::VmmAssignCpu => {
                info!(
                    "vmm_ipi_handler: core {} receive assign vcpu request for vm[{}]",
                    current_cpu().id,
                    vmm.vmid
                );
                vmm_cpu_assign_vcpu(vmm.vmid);
            }
            VmmEvent::VmmRemoveCpu => {
                info!(
                    "vmm_ipi_handler: core {} remove vcpu for vm[{}]",
                    current_cpu().id,
                    vmm.vmid
                );
                vmm_cpu_remove_vcpu(vmm.vmid);
            }
            _ => {
                todo!();
            }
        },
        _ => {
            error!("vmm_ipi_handler: illegal ipi type");
        }
    }
}
