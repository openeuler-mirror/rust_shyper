// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use alloc::collections::BTreeMap;

use spin::Mutex;

use crate::arch::traits::InterruptController;
use crate::arch::{IntCtrl, is_boot_core};
use crate::kernel::{current_cpu, ipi_irq_handler, IpiInnerMsg, IpiMessage, Vcpu, VcpuState};
use crate::kernel::Vm;
use crate::utils::{BitAlloc, BitAlloc256, BitAlloc4K, BitMap};
use super::Scheduler;

pub static INTERRUPT_GLB_BITMAP: Mutex<BitMap<BitAlloc256>> = Mutex::new(BitAlloc4K::default());
pub static INTERRUPT_HANDLERS: Mutex<BTreeMap<usize, fn()>> = Mutex::new(BTreeMap::new());

pub fn interrupt_cpu_ipi_send(target_cpu: usize, ipi_id: usize) {
    IntCtrl::ipi_send(target_cpu, ipi_id);
}

pub fn interrupt_reserve_int(int_id: usize, handler: fn()) {
    if int_id < IntCtrl::NUM_MAX {
        INTERRUPT_HANDLERS.lock().insert(int_id, handler);
        INTERRUPT_GLB_BITMAP.lock().set(int_id);
    }
}

fn interrupt_is_reserved(int_id: usize) -> Option<fn()> {
    INTERRUPT_HANDLERS.lock().get(&int_id).cloned()
}

/// enable or disable specific interrupt on current cpu
pub fn interrupt_cpu_enable(int_id: usize, en: bool) {
    IntCtrl::enable(int_id, en);
}

/// perform interrupt initialization
pub fn interrupt_init() {
    IntCtrl::init();

    let cpu_id = current_cpu().id;
    if is_boot_core(cpu_id) {
        interrupt_reserve_int(IntCtrl::IRQ_IPI, ipi_irq_handler);

        info!("Interrupt init ok");
    }
    interrupt_cpu_enable(IntCtrl::IRQ_IPI, true);
}

/// register a new interrupt for specific vm
pub fn interrupt_vm_register(vm: &Vm, id: usize) -> bool {
    // println!("VM {} register interrupt {}", vm.id(), id);
    let mut glb_bitmap_lock = INTERRUPT_GLB_BITMAP.lock();
    if glb_bitmap_lock.get(id) != 0 && id >= IntCtrl::PRI_NUN_MAX {
        warn!("interrupt_vm_register: VM {} interrupts conflict, id = {}", vm.id(), id);
        return false;
    }

    IntCtrl::vm_register(vm, id);
    glb_bitmap_lock.set(id);
    true
}

/// remove interrupt for specific vm
pub fn interrupt_vm_remove(_vm: &Vm, id: usize) {
    let mut glb_bitmap_lock = INTERRUPT_GLB_BITMAP.lock();
    // vgic and vm will be removed with struct vm
    glb_bitmap_lock.clear(id);
    // todo: for interrupt 16~31, need to check by vm config
    if id >= IntCtrl::PRI_NUN_MAX {
        interrupt_cpu_enable(id, false);
    }
}

pub fn interrupt_vm_inject(vm: &Vm, vcpu: &Vcpu, int_id: usize) {
    if vcpu.phys_id() != current_cpu().id {
        error!(
            "interrupt_vm_inject: Core {} failed to find target (VCPU {} VM {})",
            current_cpu().id,
            vcpu.id(),
            vm.id()
        );
        return;
    }
    if let VcpuState::Sleep = vcpu.state() {
        current_cpu().cpu_state = crate::kernel::CpuState::CpuRun;
        current_cpu().scheduler().wakeup(vcpu.clone());
    }
    IntCtrl::vm_inject(vm, vcpu, int_id);
}

pub fn interrupt_handler(int_id: usize) -> bool {
    if let Some(irq_handler) = interrupt_is_reserved(int_id) {
        irq_handler();
        return true;
    }

    if (16..32).contains(&int_id) {
        if let Some(vcpu) = &current_cpu().active_vcpu {
            if let Some(active_vm) = vcpu.vm() {
                if active_vm.has_interrupt(int_id) {
                    interrupt_vm_inject(&active_vm, vcpu, int_id);
                    return false;
                } else {
                    return true;
                }
            }
        }
    }

    for vcpu in current_cpu().vcpu_array.iter().flatten() {
        if let Some(vm) = vcpu.vm() {
            if vm.has_interrupt(int_id) {
                if vcpu.state() as usize == VcpuState::Invalid as usize
                    || vcpu.state() as usize == VcpuState::Sleep as usize
                {
                    return true;
                }

                interrupt_vm_inject(&vm, vcpu, int_id);
                return false;
            }
        }
    }

    warn!(
        "interrupt_handler: core {} receive unsupported int {}",
        current_cpu().id,
        int_id
    );
    true
}

/// ipi interrupt handler entry
pub fn interrupt_inject_ipi_handler(msg: IpiMessage) {
    match msg.ipi_message {
        IpiInnerMsg::IntInjectMsg(int_msg) => {
            let vm_id = int_msg.vm_id;
            let int_id = int_msg.int_id;
            match current_cpu().vcpu_array.pop_vcpu_through_vmid(vm_id) {
                None => {
                    panic!("inject int {} to illegal cpu {}", int_id, current_cpu().id);
                }
                Some(vcpu) => {
                    interrupt_vm_inject(&vcpu.vm().unwrap(), vcpu, int_id);
                }
            }
        }
        _ => {
            error!("interrupt_inject_ipi_handler: illegal ipi type");
        }
    }
}
