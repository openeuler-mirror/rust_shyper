// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use alloc::sync::Arc;

use crate::arch::{gicc_clear_current_irq, IntCtrl, InterruptController, GIC_SGIS_NUM};
use crate::config::vm_cfg_remove_vm_entry;
use crate::kernel::{
    current_cpu, interrupt_vm_remove, ipi_send_msg, IpiInnerMsg, IpiType, mem_vm_region_free, remove_async_used_info,
    remove_vm, remove_vm_async_task, vm, Vm, Scheduler, cpu_idle, IpiVmmPercoreMsg,
};
use crate::kernel::vm_if_reset;
use crate::utils::memset;

use super::VmmPercoreEvent;

pub fn vmm_remove_vm(vm_id: usize) {
    if vm_id == 0 {
        warn!("Rust-Shyper do not support remove vm0");
        return;
    }

    let vm = match vm(vm_id) {
        None => {
            error!("vmm_remove_vm: vm[{}] not exist", vm_id);
            return;
        }
        Some(vm) => vm,
    };

    // vcpu
    vmm_remove_vcpu(&vm);
    // reset vm interface
    vm_if_reset(vm_id);
    // free mem
    for idx in 0..vm.region_num() {
        // SAFETY:
        // The 'vm_pa_region' is writable for the Hypervisor in EL2.
        unsafe {
            memset(vm.pa_start(idx) as *mut u8, 0, vm.pa_length(idx));
        }
        mem_vm_region_free(vm.pa_start(idx), vm.pa_length(idx));
    }
    // passthrough dev
    vmm_remove_passthrough_device(&vm);
    // clear async task list
    remove_vm_async_task(vm_id);
    // virtio nic
    crate::device::remove_virtio_nic(vm_id);
    // async used info
    remove_async_used_info(vm_id);
    // remove vm: page table / mmio / vgic will be removed with struct vm
    vmm_remove_vm_list(vm_id);
    // remove vm cfg
    vm_cfg_remove_vm_entry(vm_id);
    // remove vm unilib
    #[cfg(feature = "unilib")]
    crate::utils::unilib::unilib_fs_remove(vm_id);
    info!("remove vm[{}] successfully", vm_id);
}

fn vmm_remove_vm_list(vm_id: usize) {
    remove_vm(vm_id);
}

pub fn vmm_cpu_remove_vcpu(vmid: usize) {
    let vcpu = current_cpu().vcpu_array.remove_vcpu(vmid);
    if let Some(vcpu) = vcpu {
        // remove vcpu from scheduler
        current_cpu().scheduler().sleep(vcpu);
    }
    if current_cpu().vcpu_array.vcpu_num() == 0 {
        gicc_clear_current_irq(true);
        cpu_idle();
    }
}

fn vmm_remove_vcpu(vm: &Arc<Vm>) {
    for vcpu in vm.vcpu_list() {
        if vcpu.phys_id() == current_cpu().id {
            vmm_remove_vcpu_percore(vm);
        } else {
            let m = IpiVmmPercoreMsg {
                vm: vm.clone(),
                event: VmmPercoreEvent::VmmRemoveCpu,
            };
            if !ipi_send_msg(vcpu.phys_id(), IpiType::IpiTVMM, IpiInnerMsg::VmmPercoreMsg(m)) {
                warn!("vmm_remove_vcpu: failed to send ipi to Core {}", vcpu.phys_id());
            }
        }
    }
}

pub fn vmm_remove_vcpu_percore(vm: &Vm) {
    current_cpu().vcpu_array.remove_vcpu(vm.id());
    if !current_cpu().assigned() {
        IntCtrl::enable(IntCtrl::IRQ_GUEST_TIMER, false);
        IntCtrl::clear();
    }
}

fn vmm_remove_passthrough_device(vm: &Vm) {
    for irq in vm.config().passthrough_device_irqs() {
        if *irq > GIC_SGIS_NUM {
            interrupt_vm_remove(vm, *irq);
        }
    }
}
