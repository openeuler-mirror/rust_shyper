// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::mem::size_of;
use spin::Mutex;

use crate::arch::{ContextFrame, ContextFrameTrait, GicContext, VmContext, timer_arch_get_counter};
use crate::board::PlatOperation;
use crate::config::VmConfigEntry;
use crate::kernel::{current_cpu, interrupt_vm_inject, vm_if_set_state};
use crate::kernel::{active_vcpu_id, active_vm_id};
use crate::utils::memcpy;

use super::{CpuState, Vm, VmType};

#[derive(Clone, Copy, Debug)]
/// Vcpu state Enum
pub enum VcpuState {
    Invalid = 0,
    Ready = 1,
    Running = 2,
    Sleep = 3,
}

struct VcpuInnerConst {
    id: usize,      // vcpu id
    vm: Weak<Vm>,   // weak ref to related vm
    phys_id: usize, // binding physical CPU's id
}

pub struct VcpuInner {
    inner_const: VcpuInnerConst,
    pub inner_mut: Mutex<VcpuInnerMut>,
}

#[derive(Clone)]
/// Vcpu struct
pub struct Vcpu {
    pub inner: Arc<VcpuInner>,
}

impl PartialEq for Vcpu {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Vcpu {
    pub fn new(vm: Weak<Vm>, vcpu_id: usize, phys_id: usize) -> Self {
        Self {
            inner: Arc::new(VcpuInner {
                inner_const: VcpuInnerConst {
                    id: vcpu_id,
                    vm,
                    phys_id,
                },
                inner_mut: Mutex::new(VcpuInnerMut::new()),
            }),
        }
    }

    pub fn init(&self, config: &VmConfigEntry) {
        self.init_boot_info(config);
        self.init_spsr();
        self.reset_context();
    }

    pub fn init_boot_info(&self, config: &VmConfigEntry) {
        let arg = match config.os_type {
            VmType::VmTOs => config.device_tree_load_ipa(),
            _ => {
                let arg = &config.memory_region()[0];
                arg.ipa_start + arg.length
            }
        };
        let mut inner = self.inner.inner_mut.lock();
        inner.vcpu_ctx.set_argument(arg);
        inner.vcpu_ctx.set_exception_pc(config.kernel_entry_point());
    }

    /// shutdown this vcpu
    pub fn shutdown(&self) {
        info!(
            "Core {} (vm {} vcpu {}) shutdown ok",
            current_cpu().id,
            active_vm_id(),
            active_vcpu_id()
        );
        crate::board::Platform::cpu_shutdown();
    }

    pub fn context_vm_store(&self) {
        self.save_cpu_ctx();

        let mut inner = self.inner.inner_mut.lock();
        inner.vm_ctx.ext_regs_store();
        inner.vm_ctx.fpsimd_save_context();
        inner.vm_ctx.gic_save_state();
    }

    pub fn context_vm_restore(&self) {
        self.restore_cpu_ctx();

        let inner = self.inner.inner_mut.lock();
        // restore vm's VFP and SIMD
        inner.vm_ctx.fpsimd_restore_context();
        inner.vm_ctx.gic_restore_state();
        inner.vm_ctx.ext_regs_restore();
        drop(inner);

        self.inject_int_inlist();
    }

    pub fn gic_restore_context(&self) {
        let inner = self.inner.inner_mut.lock();
        inner.vm_ctx.gic_restore_state();
    }

    pub fn gic_save_context(&self) {
        let mut inner = self.inner.inner_mut.lock();
        inner.vm_ctx.gic_save_state();
    }

    pub fn save_cpu_ctx(&self) {
        let inner = self.inner.inner_mut.lock();
        match current_cpu().ctx_ptr() {
            None => {
                error!("save_cpu_ctx: cpu{} ctx is NULL", current_cpu().id);
            }
            // SAFETY:
            // We have both read and write access to the src and dst memory regions.
            // The copied size will not exceed the memory region.
            Some(ctx) => unsafe {
                memcpy(
                    &(inner.vcpu_ctx) as *const _ as *const u8,
                    ctx as *const u8,
                    size_of::<ContextFrame>(),
                );
            },
        }
    }

    fn restore_cpu_ctx(&self) {
        let inner = self.inner.inner_mut.lock();
        match current_cpu().ctx_ptr() {
            None => {
                error!("restore_cpu_ctx: cpu{} ctx is NULL", current_cpu().id);
            }
            // SAFETY:
            // We have both read and write access to the src and dst memory regions.
            // The copied size will not exceed the memory region.
            Some(ctx) => unsafe {
                memcpy(
                    ctx as *const u8,
                    &(inner.vcpu_ctx) as *const _ as *const u8,
                    size_of::<ContextFrame>(),
                );
            },
        }
    }

    pub fn state(&self) -> VcpuState {
        let inner = self.inner.inner_mut.lock();
        inner.state
    }

    pub fn set_state(&self, state: VcpuState) {
        let mut inner = self.inner.inner_mut.lock();
        inner.state = state;
    }

    pub fn id(&self) -> usize {
        self.inner.inner_const.id
    }

    pub fn vm(&self) -> Option<Arc<Vm>> {
        self.inner.inner_const.vm.upgrade()
    }

    #[inline]
    pub fn phys_id(&self) -> usize {
        self.inner.inner_const.phys_id
    }

    pub fn vm_id(&self) -> usize {
        self.vm().unwrap().id()
    }

    pub fn vm_pt_dir(&self) -> usize {
        self.vm().unwrap().pt_dir()
    }

    pub fn reset_context(&self) {
        let mut inner = self.inner.inner_mut.lock();
        inner.vm_ctx.vmpidr_el2 = self.get_vmpidr() as u64;
        let vm_id = self.vm().unwrap().id();
        use crate::kernel::vm_if_get_type;
        if vm_if_get_type(vm_id) == VmType::VmTBma {
            debug!("vm {} bma ctx restore", vm_id);
            inner.vm_ctx.reset();
            drop(inner);
            self.context_ext_regs_store();
        }
    }

    pub fn reset_vtimer_offset(&self) {
        let mut inner = self.inner.inner_mut.lock();
        let curpct = timer_arch_get_counter() as u64;
        inner.vm_ctx.cntvoff_el2 = curpct - inner.vm_ctx.cntvct_el0;
    }

    pub fn context_ext_regs_store(&self) {
        let mut inner = self.inner.inner_mut.lock();
        inner.vm_ctx.ext_regs_store();
    }

    pub fn vcpu_ctx_addr(&self) -> usize {
        let inner: spin::MutexGuard<VcpuInnerMut> = self.inner.inner_mut.lock();
        &(inner.vcpu_ctx) as *const _ as usize
    }

    pub fn set_elr(&self, elr: usize) {
        let mut inner = self.inner.inner_mut.lock();
        inner.vcpu_ctx.set_exception_pc(elr);
    }

    pub fn elr(&self) -> usize {
        let inner = self.inner.inner_mut.lock();
        inner.vcpu_ctx.exception_pc()
    }

    pub fn set_gpr(&self, idx: usize, val: usize) {
        let mut inner = self.inner.inner_mut.lock();
        inner.vcpu_ctx.set_gpr(idx, val);
    }

    pub fn push_int(&self, int: usize) {
        let mut inner = self.inner.inner_mut.lock();
        if !inner.int_list.contains(&int) {
            inner.int_list.push(int);
        }
    }

    fn inject_int_inlist(&self) {
        match self.vm() {
            None => {}
            Some(vm) => {
                let mut inner = self.inner.inner_mut.lock();
                let int_list = inner.int_list.clone();
                inner.int_list.clear();
                drop(inner);
                for int in int_list {
                    interrupt_vm_inject(&vm, self, int);
                }
            }
        }
    }

    pub fn get_vmpidr(&self) -> usize {
        1 << 31
            | if cfg!(feature = "rk3588") {
                0x100_0000 | (self.id() << 8)
            } else {
                self.id()
            }
    }
}

struct IdleThread {
    pub ctx: ContextFrame,
}

fn idle_thread() {
    loop {
        cortex_a::asm::wfi();
    }
}

static IDLE_THREAD: spin::Lazy<IdleThread> = spin::Lazy::new(|| {
    let mut ctx = ContextFrame::new(idle_thread as usize, current_cpu().stack_top(), 0);
    use cortex_a::registers::SPSR_EL2;
    ctx.set_exception_pc(idle_thread as usize);
    ctx.spsr = (SPSR_EL2::M::EL2h + SPSR_EL2::F::Masked + SPSR_EL2::A::Masked + SPSR_EL2::D::Masked).value;
    IdleThread { ctx }
});

pub fn run_idle_thread() {
    trace!("Core {} idle", current_cpu().id);
    current_cpu().cpu_state = CpuState::CpuIdle;
    // SAFETY:
    // We have both read and write access to the src and dst memory regions.
    // The copied size will not exceed the memory region.
    unsafe {
        crate::utils::memcpy(
            current_cpu().ctx as *const u8,
            &(IDLE_THREAD.ctx) as *const _ as *const u8,
            core::mem::size_of::<ContextFrame>(),
        );
    }
}

pub struct VcpuInnerMut {
    pub state: VcpuState,
    pub int_list: Vec<usize>,
    pub vcpu_ctx: ContextFrame,
    pub vm_ctx: VmContext,
    pub gic_ctx: GicContext,
}

impl VcpuInnerMut {
    fn new() -> VcpuInnerMut {
        VcpuInnerMut {
            state: VcpuState::Invalid,
            int_list: vec![],
            vcpu_ctx: ContextFrame::default(),
            vm_ctx: VmContext::default(),
            gic_ctx: GicContext::default(),
        }
    }

    fn gic_ctx_reset(&mut self) {
        use crate::arch::gich_lrs_num;
        for i in 0..gich_lrs_num() {
            self.vm_ctx.gic_state.lr[i] = 0;
        }
        self.vm_ctx.gic_state.hcr |= 1 << 2; // init hcr
    }
}

// WARNING: No Auto `drop` in this function
pub fn vcpu_run(announce: bool) -> ! {
    {
        let vcpu = current_cpu().active_vcpu.clone().unwrap();
        let vm = vcpu.vm().unwrap();

        current_cpu().cpu_state = CpuState::CpuRun;
        vm_if_set_state(active_vm_id(), super::VmState::VmActive);

        vcpu.context_vm_restore();
        if announce {
            crate::device::virtio_net_announce(vm.clone());
        }
        crate::arch::tlb_invalidate_guest_all();
        for i in 0..vm.mem_region_num() {
            unsafe {
                crate::arch::cache_invalidate_d(vm.pa_start(i), vm.pa_length(i));
            }
        }
    }
    extern "C" {
        fn context_vm_entry(ctx: usize) -> !;
    }
    // SAFETY: `Cpu` maintains the underlying `ctx`.
    unsafe {
        context_vm_entry(current_cpu().ctx as usize);
    }
}
