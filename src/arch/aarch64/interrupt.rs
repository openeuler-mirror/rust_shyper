// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use crate::arch::traits::InterruptController;
use crate::arch::{gic_cpu_reset, GIC_PRIVINT_NUM, gicc_clear_current_irq};
#[cfg(not(feature = "gicv3"))]
use crate::board::{Platform, PlatOperation};
use crate::kernel::{current_cpu, Vcpu, Vm};

use super::{GIC_SGIS_NUM, GICD, gicc_get_current_irq};

const INTERRUPT_NUM_MAX: usize = 1024;
const INTERRUPT_IRQ_HYPERVISOR_TIMER: usize = 26;
const INTERRUPT_IRQ_GUEST_TIMER: usize = 27;
const INTERRUPT_IRQ_IPI: usize = 1;

pub struct IntCtrl;

impl InterruptController for IntCtrl {
    const NUM_MAX: usize = INTERRUPT_NUM_MAX;

    const PRI_NUN_MAX: usize = GIC_PRIVINT_NUM;

    const IRQ_IPI: usize = INTERRUPT_IRQ_IPI;

    const IRQ_HYPERVISOR_TIMER: usize = INTERRUPT_IRQ_HYPERVISOR_TIMER;

    const IRQ_GUEST_TIMER: usize = INTERRUPT_IRQ_GUEST_TIMER;

    fn init() {
        use crate::arch::{gic_cpu_init, gic_glb_init, gic_maintenance_handler};
        use crate::kernel::interrupt_reserve_int;

        #[cfg(not(feature = "secondary_start"))]
        crate::utils::barrier();

        if current_cpu().id == 0 {
            gic_glb_init();
        }

        gic_cpu_init();

        use crate::board::PLAT_DESC;

        let int_id = PLAT_DESC.arch_desc.gic_desc.maintenance_int_id;
        interrupt_reserve_int(int_id, gic_maintenance_handler);
        Self::enable(int_id, true);
    }

    fn enable(int_id: usize, en: bool) {
        #[cfg(feature = "gicv3")]
        {
            use tock_registers::interfaces::Readable;
            use crate::arch::{gic_set_enable, gic_set_prio};
            gic_set_enable(int_id, en);
            gic_set_prio(int_id, 0x1);
            GICD.set_route(int_id, cortex_a::registers::MPIDR_EL1.get() as usize);
        }
        #[cfg(not(feature = "gicv3"))]
        {
            if en {
                GICD.set_prio(int_id, 0x1);
                GICD.set_trgt(int_id, 1 << Platform::cpuid_to_cpuif(current_cpu().id));
                GICD.set_enable(int_id, en);
            } else {
                GICD.set_enable(int_id, en);
            }
        }
    }

    fn fetch() -> Option<usize> {
        gicc_get_current_irq()
    }

    fn clear() {
        gic_cpu_reset();
        gicc_clear_current_irq(true);
    }

    fn finish(_int_id: usize) {
        todo!()
    }

    fn ipi_send(cpu_id: usize, ipi_id: usize) {
        if ipi_id < GIC_SGIS_NUM {
            #[cfg(not(feature = "gicv3"))]
            GICD.send_sgi(Platform::cpuid_to_cpuif(cpu_id), ipi_id);
            #[cfg(feature = "gicv3")]
            GICD.send_sgi(cpu_id, ipi_id);
        }
    }

    fn vm_inject(vm: &Vm, vcpu: &Vcpu, int_id: usize) {
        let vgic = vm.vgic();
        if let Some(cur_vcpu) = current_cpu().active_vcpu.as_ref() {
            if cur_vcpu == vcpu {
                vgic.inject(vcpu, int_id);
                return;
            }
        }

        vcpu.push_int(int_id);
    }

    fn vm_register(vm: &Vm, int_id: usize) {
        super::vgic_set_hw_int(vm, int_id);
    }

    fn clear_current_irq(for_hypervisor: bool) {
        gicc_clear_current_irq(for_hypervisor);
    }
}
