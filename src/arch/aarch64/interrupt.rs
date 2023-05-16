// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use tock_registers::interfaces::Readable;

use crate::arch::{gic_cpu_reset, gicc_clear_current_irq};
use crate::board::{Platform, PlatOperation};
use crate::kernel::{current_cpu, Vcpu, Vm};

use super::GIC_SGIS_NUM;
use super::GICD;

pub const INTERRUPT_IRQ_HYPERVISOR_TIMER: usize = 26;
pub const INTERRUPT_IRQ_IPI: usize = 1;

pub fn interrupt_arch_init() {
    use crate::arch::{gic_cpu_init, gic_glb_init, gic_maintenance_handler};
    use crate::kernel::{interrupt_reserve_int, InterruptHandler};

    crate::lib::barrier();

    if current_cpu().id == 0 {
        gic_glb_init();
    }

    gic_cpu_init();

    use crate::board::PLAT_DESC;

    let int_id = PLAT_DESC.arch_desc.gic_desc.maintenance_int_id;
    interrupt_reserve_int(int_id, InterruptHandler::GicMaintenanceHandler(gic_maintenance_handler));
    interrupt_arch_enable(int_id, true);
}

pub fn interrupt_arch_enable(int_id: usize, en: bool) {
    #[cfg(feature = "gicv3")]
    {
        use crate::arch::{gic_set_enable, gic_set_prio};
        gic_set_enable(int_id, en);
        gic_set_prio(int_id, 0x1);
        GICD.set_route(int_id, cortex_a::registers::MPIDR_EL1.get() as usize);
    }
    #[cfg(not(feature = "gicv3"))]
    {
        GICD.set_enable(int_id, en);
        GICD.set_prio(int_id, 0x1);
        GICD.set_trgt(int_id, 1 << Platform::cpuid_to_cpuif(current_cpu().id));
    }
}

pub fn interrupt_arch_ipi_send(cpu_id: usize, ipi_id: usize) {
    if ipi_id < GIC_SGIS_NUM {
        GICD.send_sgi(Platform::cpuid_to_cpuif(cpu_id), ipi_id);
    }
}

pub fn interrupt_arch_vm_register(vm: Vm, id: usize) {
    super::vgic_set_hw_int(vm, id);
}

pub fn interrupt_arch_vm_inject(vm: Vm, vcpu: Vcpu, int_id: usize) {
    let vgic = vm.vgic();
    // println!("int {}, cur vcpu vm {}, trgt vcpu vm {}", int_id, active_vm_id(), vcpu.vm_id());
    // restore_vcpu_gic(current_cpu().active_vcpu.clone(), vcpu.clone());
    if let Some(cur_vcpu) = current_cpu().active_vcpu.clone() {
        if cur_vcpu.vm_id() == vcpu.vm_id() {
            vgic.inject(vcpu, int_id);
            return;
        }
    }

    vcpu.push_int(int_id);
    // save_vcpu_gic(current_cpu().active_vcpu.clone(), vcpu.clone());
}

pub fn interrupt_arch_clear() {
    gic_cpu_reset();
    gicc_clear_current_irq(true);
}
