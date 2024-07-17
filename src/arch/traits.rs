// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.
use crate::kernel::Vm;
use crate::kernel::Vcpu;

/// Architecture-independent ContextFrame trait.
pub trait ContextFrameTrait {
    fn new(pc: usize, sp: usize, arg: usize) -> Self;

    fn exception_pc(&self) -> usize;
    fn set_exception_pc(&mut self, pc: usize);
    fn stack_pointer(&self) -> usize;
    fn set_stack_pointer(&mut self, sp: usize);
    fn set_argument(&mut self, arg: usize);
    fn set_gpr(&mut self, index: usize, val: usize);
    fn gpr(&self, index: usize) -> usize;
}

pub trait ArchTrait {
    fn wait_for_interrupt();
    fn install_vm_page_table(base: usize, vmid: usize);
}

/// Architecture-independent PageTableEntry trait.
pub trait ArchPageTableEntryTrait {
    fn from_pte(value: usize) -> Self;
    fn from_pa(pa: usize) -> Self;
    fn to_pte(&self) -> usize;
    fn to_pa(&self) -> usize;
    fn valid(&self) -> bool;
    fn entry(&self, index: usize) -> Self;
    unsafe fn set_entry(&self, index: usize, value: Self);
    fn make_table(frame_pa: usize) -> Self;
}

/// Architecture-independent Interrupt Context trait.
/// Interrupt Context: the context of interrupt controller of specific VM
pub trait InterruptContextTrait: Default {
    fn save_state(&mut self);
    fn restore_state(&self);
}

pub trait InterruptController {
    const NUM_MAX: usize;
    const PRI_NUN_MAX: usize;
    const IRQ_IPI: usize;
    const IRQ_HYPERVISOR_TIMER: usize;
    const IRQ_GUEST_TIMER: usize;

    fn init();
    fn enable(int_id: usize, en: bool);
    fn fetch() -> Option<usize>;
    fn clear();
    fn finish(int_id: usize);
    fn ipi_send(cpu_id: usize, ipi_id: usize);
    fn vm_inject(vm: &Vm, vcpu: &Vcpu, int_id: usize);
    fn vm_register(vm: &Vm, int_id: usize);
    fn clear_current_irq(for_hypervisor: bool);
}

pub trait VmContextTrait {
    fn reset(&mut self);
    fn ext_regs_store(&mut self);
    fn ext_regs_restore(&self);
    fn fpsimd_save_context(&mut self);
    fn fpsimd_restore_context(&self);
    fn gic_save_state(&mut self);
    fn gic_restore_state(&self);
    fn gic_ctx_reset(&mut self);
    fn reset_vtimer_offset(&mut self);
}
