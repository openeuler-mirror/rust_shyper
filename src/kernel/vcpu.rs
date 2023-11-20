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
use alloc::vec::Vec;
use core::mem::size_of;
use spin::Mutex;

use crate::arch::{
    ContextFrame, ContextFrameTrait, cpu_interrupt_unmask, GicContext, GICD, VmContext, timer_arch_get_counter,
};
use crate::board::{Platform, PlatOperation, PLATFORM_VCPU_NUM_MAX};
use crate::kernel::{current_cpu, interrupt_vm_inject, vm_if_set_state};
use crate::kernel::{active_vcpu_id, active_vm_id};
use crate::utils::memcpy_safe;

use super::{CpuState, Vm, VmType};

#[derive(Clone, Copy, Debug)]
pub enum VcpuState {
    Invalid = 0,
    Ready = 1,
    Running = 2,
    Sleep = 3,
}

#[derive(Clone)]
pub struct Vcpu {
    pub inner: Arc<Mutex<VcpuInner>>,
}

impl Vcpu {
    pub fn default() -> Vcpu {
        Vcpu {
            inner: Arc::new(Mutex::new(VcpuInner::default())),
        }
    }

    pub fn init(&self, vm: Vm, vcpu_id: usize) {
        let mut inner = self.inner.lock();
        inner.vm = Some(vm.clone());
        inner.id = vcpu_id;
        inner.phys_id = 0;
        drop(inner);
        crate::arch::vcpu_arch_init(vm, self.clone());
        self.reset_context();
    }

    pub fn shutdown(&self) {
        info!(
            "Core {} (vm {} vcpu {}) shutdown ok",
            current_cpu().id,
            active_vm_id(),
            active_vcpu_id()
        );
        crate::board::Platform::cpu_shutdown();
    }

    pub fn migrate_vm_ctx_save(&self, cache_pa: usize) {
        let inner = self.inner.lock();
        memcpy_safe(
            cache_pa as *const u8,
            &(inner.vm_ctx) as *const _ as *const u8,
            size_of::<VmContext>(),
        );
    }

    pub fn migrate_vcpu_ctx_save(&self, cache_pa: usize) {
        let inner = self.inner.lock();
        memcpy_safe(
            cache_pa as *const u8,
            &(inner.vcpu_ctx) as *const _ as *const u8,
            size_of::<ContextFrame>(),
        );
    }

    pub fn migrate_gic_ctx_save(&self, cache_pa: usize) {
        let inner = self.inner.lock();
        memcpy_safe(
            cache_pa as *const u8,
            &(inner.gic_ctx) as *const _ as *const u8,
            size_of::<GicContext>(),
        );
    }

    pub fn migrate_vm_ctx_restore(&self, cache_pa: usize) {
        let inner = self.inner.lock();
        memcpy_safe(
            &(inner.vm_ctx) as *const _ as *const u8,
            cache_pa as *const u8,
            size_of::<VmContext>(),
        );
    }

    pub fn migrate_vcpu_ctx_restore(&self, cache_pa: usize) {
        let inner = self.inner.lock();
        memcpy_safe(
            &(inner.vcpu_ctx) as *const _ as *const u8,
            cache_pa as *const u8,
            size_of::<ContextFrame>(),
        );
    }

    pub fn migrate_gic_ctx_restore(&self, cache_pa: usize) {
        let inner = self.inner.lock();
        memcpy_safe(
            &(inner.gic_ctx) as *const _ as *const u8,
            cache_pa as *const u8,
            size_of::<GicContext>(),
        );
    }

    pub fn context_vm_store(&self) {
        self.save_cpu_ctx();

        let mut inner = self.inner.lock();
        inner.vm_ctx.ext_regs_store();
        inner.vm_ctx.fpsimd_save_context();
        inner.vm_ctx.gic_save_state();
    }

    pub fn context_gic_irqs_store(&self) {
        let mut inner = self.inner.lock();
        let vm = inner.vm.clone().unwrap();
        inner.gic_ctx;
        for irq in vm.config().passthrough_device_irqs() {
            inner.gic_ctx.add_irq(irq as u64);
        }
        inner.gic_ctx.add_irq(25);
        let gicv_ctlr = unsafe { &*(Platform::GICV_BASE as *const u32) };
        inner.gic_ctx.set_gicv_ctlr(*gicv_ctlr);
        let gicv_pmr = unsafe { &*((Platform::GICV_BASE + 0x4) as *const u32) };
        inner.gic_ctx.set_gicv_pmr(*gicv_pmr);
    }

    pub fn context_gic_irqs_restore(&self) {
        let inner = self.inner.lock();

        for irq_state in inner.gic_ctx.irq_state.iter() {
            if irq_state.id != 0 {
                #[cfg(feature = "gicv3")]
                {
                    use crate::arch::{gic_set_enable, gic_set_prio};
                    gic_set_enable(irq_state.id as usize, irq_state.enable != 0);
                    gic_set_prio(irq_state.id as usize, irq_state.priority);
                    GICD.set_route(irq_state.id as usize, 1 << Platform::cpuid_to_cpuif(current_cpu().id));
                }
                #[cfg(not(feature = "gicv3"))]
                {
                    GICD.set_enable(irq_state.id as usize, irq_state.enable != 0);
                    GICD.set_prio(irq_state.id as usize, irq_state.priority);
                    GICD.set_trgt(irq_state.id as usize, 1 << Platform::cpuid_to_cpuif(current_cpu().id));
                }
            }
        }

        let gicv_pmr = unsafe { &mut *((Platform::GICV_BASE + 0x4) as *mut u32) };
        *gicv_pmr = inner.gic_ctx.gicv_pmr();
        // println!("Core[{}] save gic context", current_cpu().id);
        let gicv_ctlr = unsafe { &mut *(Platform::GICV_BASE as *mut u32) };
        *gicv_ctlr = inner.gic_ctx.gicv_ctlr();
        // show_vcpu_reg_context();
    }

    pub fn context_vm_restore(&self) {
        self.restore_cpu_ctx();

        let inner = self.inner.lock();
        // restore vm's VFP and SIMD
        inner.vm_ctx.fpsimd_restore_context();
        inner.vm_ctx.gic_restore_state();
        inner.vm_ctx.ext_regs_restore();
        drop(inner);

        self.inject_int_inlist();
    }

    pub fn gic_restore_context(&self) {
        let inner = self.inner.lock();
        inner.vm_ctx.gic_restore_state();
    }

    pub fn gic_save_context(&self) {
        let mut inner = self.inner.lock();
        inner.vm_ctx.gic_save_state();
    }

    pub fn save_cpu_ctx(&self) {
        let inner = self.inner.lock();
        match current_cpu().ctx_ptr() {
            None => {
                error!("save_cpu_ctx: cpu{} ctx is NULL", current_cpu().id);
            }
            Some(ctx) => {
                memcpy_safe(
                    &(inner.vcpu_ctx) as *const _ as *const u8,
                    ctx as *const u8,
                    size_of::<ContextFrame>(),
                );
            }
        }
    }

    fn restore_cpu_ctx(&self) {
        let inner = self.inner.lock();
        match current_cpu().ctx_ptr() {
            None => {
                error!("restore_cpu_ctx: cpu{} ctx is NULL", current_cpu().id);
            }
            Some(ctx) => {
                memcpy_safe(
                    ctx as *const u8,
                    &(inner.vcpu_ctx) as *const _ as *const u8,
                    size_of::<ContextFrame>(),
                );
            }
        }
    }

    pub fn set_phys_id(&self, phys_id: usize) {
        let mut inner = self.inner.lock();
        debug!("set vcpu {} phys id {}", inner.id, phys_id);
        inner.phys_id = phys_id;
    }

    pub fn set_gich_ctlr(&self, ctlr: u32) {
        let mut inner = self.inner.lock();
        inner.vm_ctx.gic_state.ctlr = ctlr;
    }

    pub fn set_hcr(&self, hcr: u64) {
        let mut inner = self.inner.lock();
        inner.vm_ctx.hcr_el2 = hcr;
    }

    pub fn state(&self) -> VcpuState {
        let inner = self.inner.lock();
        inner.state.clone()
    }

    pub fn set_state(&self, state: VcpuState) {
        let mut inner = self.inner.lock();
        inner.state = state;
    }

    pub fn id(&self) -> usize {
        let inner = self.inner.lock();
        inner.id
    }

    pub fn vm(&self) -> Option<Vm> {
        let inner = self.inner.lock();
        inner.vm.clone()
    }

    pub fn phys_id(&self) -> usize {
        let inner = self.inner.lock();
        inner.phys_id
    }

    pub fn vm_id(&self) -> usize {
        self.vm().unwrap().id()
    }

    pub fn vm_pt_dir(&self) -> usize {
        self.vm().unwrap().pt_dir()
    }

    pub fn reset_context(&self) {
        let mut inner = self.inner.lock();
        inner.reset_context();
    }

    pub fn reset_vmpidr(&self) {
        let mut inner = self.inner.lock();
        inner.reset_vmpidr();
    }

    pub fn reset_vtimer_offset(&self) {
        let mut inner = self.inner.lock();
        inner.reset_vtimer_offset();
    }

    pub fn context_ext_regs_store(&self) {
        let mut inner = self.inner.lock();
        inner.context_ext_regs_store();
    }

    pub fn vcpu_ctx_addr(&self) -> usize {
        let inner: spin::MutexGuard<VcpuInner> = self.inner.lock();
        inner.vcpu_ctx_addr()
    }

    pub fn set_elr(&self, elr: usize) {
        let mut inner = self.inner.lock();
        inner.set_elr(elr);
    }

    pub fn elr(&self) -> usize {
        let inner = self.inner.lock();
        inner.vcpu_ctx.exception_pc()
    }

    pub fn set_gpr(&self, idx: usize, val: usize) {
        let mut inner = self.inner.lock();
        inner.set_gpr(idx, val);
    }

    pub fn show_ctx(&self) {
        let inner = self.inner.lock();
        inner.show_ctx();
    }

    pub fn push_int(&self, int: usize) {
        let mut inner = self.inner.lock();
        if !inner.int_list.contains(&int) {
            inner.int_list.push(int);
        }
    }

    fn inject_int_inlist(&self) {
        match self.vm() {
            None => {}
            Some(vm) => {
                let mut inner = self.inner.lock();
                let int_list = inner.int_list.clone();
                inner.int_list.clear();
                drop(inner);
                for int in int_list {
                    // println!("schedule: inject int {} for vm {}", int, vm.id());
                    interrupt_vm_inject(vm.clone(), self.clone(), int, 0);
                }
            }
        }
    }

    pub fn get_vmpidr(&self) -> usize {
        let inner = self.inner.lock();
        inner.vm_ctx.vmpidr_el2 as usize
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
    crate::utils::memcpy_safe(
        current_cpu().ctx as *const u8,
        &(IDLE_THREAD.ctx) as *const _ as *const u8,
        core::mem::size_of::<ContextFrame>(),
    );
}

pub fn find_vcpu_by_id(id: usize) -> Option<Vcpu> {
    let vcpu_list = VCPU_LIST.lock();
    vcpu_list.iter().find(|&x| x.id() == id).cloned()
}

pub struct VcpuInner {
    pub id: usize,
    pub phys_id: usize,
    pub state: VcpuState,
    pub vm: Option<Vm>,
    pub int_list: Vec<usize>,
    pub vcpu_ctx: ContextFrame,
    pub vm_ctx: VmContext,
    pub gic_ctx: GicContext,
}

impl VcpuInner {
    pub fn default() -> VcpuInner {
        VcpuInner {
            id: 0,
            phys_id: 0,
            state: VcpuState::Invalid,
            vm: None,
            int_list: vec![],
            vcpu_ctx: ContextFrame::default(),
            vm_ctx: VmContext::default(),
            gic_ctx: GicContext::default(),
        }
    }

    fn vcpu_ctx_addr(&self) -> usize {
        &(self.vcpu_ctx) as *const _ as usize
    }

    fn vm_id(&self) -> usize {
        let vm = self.vm.as_ref().unwrap();
        vm.id()
    }

    fn arch_ctx_reset(&mut self) {
        // let migrate = self.vm.as_ref().unwrap().migration_state();
        // if !migrate {
        self.vm_ctx.cntvoff_el2 = 0;
        self.vm_ctx.sctlr_el1 = 0x30C50830;
        self.vm_ctx.cntkctl_el1 = 0;
        self.vm_ctx.pmcr_el0 = 0;
        if cfg!(feature = "lvl4") {
            use cortex_a::registers::VTCR_EL2;
            let vtcr = (1 << 31)
                + (VTCR_EL2::PS::PA_44B_16TB
                    + VTCR_EL2::TG0::Granule4KB
                    + VTCR_EL2::SH0::Inner
                    + VTCR_EL2::ORGN0::NormalWBRAWA
                    + VTCR_EL2::IRGN0::NormalWBRAWA
                    + VTCR_EL2::SL0.val(0b10) // 10: If TG0 is 00 (4KB granule), start at level 0.
                    + VTCR_EL2::T0SZ.val(64 - 44))
                .value;
            self.vm_ctx.vtcr_el2 = vtcr;
        } else {
            self.vm_ctx.vtcr_el2 = 0x8001355c;
        }
        self.reset_vmpidr()
    }

    fn reset_vmpidr(&mut self) {
        let mut vmpidr = 0;
        vmpidr |= 1 << 31; //bit[31]:res1

        if self.vm.as_ref().unwrap().config().cpu_num() == 1 {
            vmpidr |= 1 << 30; //bit[30]: Indicates a Uniprocessor system
        }

        #[cfg(feature = "tx2")]
        if self.vm_id() == 0 {
            // A57 is cluster #1 for L4T
            vmpidr |= 0x100;
        }

        vmpidr |= if cfg!(feature = "rk3588") {
            0x100_0000 | (self.id << 8)
        } else {
            self.id
        };
        self.vm_ctx.vmpidr_el2 = vmpidr as u64;
    }

    fn reset_vtimer_offset(&mut self) {
        let curpct = timer_arch_get_counter() as u64;
        self.vm_ctx.cntvoff_el2 = curpct - self.vm_ctx.cntvct_el0;
    }

    fn reset_context(&mut self) {
        // let migrate = self.vm.as_ref().unwrap().migration_state();
        self.arch_ctx_reset();
        // if !migrate {
        self.gic_ctx_reset();
        // }
        use crate::kernel::vm_if_get_type;
        match vm_if_get_type(self.vm_id()) {
            VmType::VmTBma => {
                debug!("vm {} bma ctx restore", self.vm_id());
                self.reset_vm_ctx();
                self.context_ext_regs_store();
            }
            _ => {}
        }
    }

    fn gic_ctx_reset(&mut self) {
        use crate::arch::gich_lrs_num;
        for i in 0..gich_lrs_num() {
            self.vm_ctx.gic_state.lr[i] = 0;
        }
        self.vm_ctx.gic_state.hcr |= 1 << 2; // init hcr
    }

    fn context_ext_regs_store(&mut self) {
        self.vm_ctx.ext_regs_store();
    }

    fn reset_vm_ctx(&mut self) {
        self.vm_ctx.reset();
    }

    fn set_elr(&mut self, elr: usize) {
        self.vcpu_ctx.set_exception_pc(elr);
    }

    fn set_gpr(&mut self, idx: usize, val: usize) {
        self.vcpu_ctx.set_gpr(idx, val);
    }

    fn show_ctx(&self) {
        info!(
            "cntvoff_el2 {:x}, sctlr_el1 {:x}, cntkctl_el1 {:x}, pmcr_el0 {:x}, vtcr_el2 {:x} x0 {:x}",
            self.vm_ctx.cntvoff_el2,
            self.vm_ctx.sctlr_el1,
            self.vm_ctx.cntkctl_el1,
            self.vm_ctx.pmcr_el0,
            self.vm_ctx.vtcr_el2,
            self.vcpu_ctx.gpr(0)
        );
        info!("id {} vm_ctx {:x?}", self.id, self.vm_ctx);
    }
}

pub static VCPU_LIST: Mutex<Vec<Vcpu>> = Mutex::new(Vec::new());

pub fn vcpu_alloc() -> Option<Vcpu> {
    let mut vcpu_list = VCPU_LIST.lock();
    if vcpu_list.len() >= PLATFORM_VCPU_NUM_MAX {
        return None;
    }

    let val = Vcpu::default();
    vcpu_list.push(val.clone());
    Some(val)
}

pub fn vcpu_remove(vcpu: Vcpu) {
    let mut vcpu_list = VCPU_LIST.lock();
    for (idx, core) in vcpu_list.iter().enumerate() {
        if core.id() == vcpu.id() && core.vm_id() == vcpu.vm_id() {
            vcpu_list.remove(idx);
            return;
        }
    }
    panic!("illegal vm{} vcpu{}, not exist in vcpu_list", vcpu.vm_id(), vcpu.id());
}

pub fn vcpu_idle(_vcpu: Vcpu) -> ! {
    cpu_interrupt_unmask();
    loop {
        // TODO: replace it with an Arch function `arch_idle`
        cortex_a::asm::wfi();
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
            crate::device::virtio_net_announce(vm);
        }
        // tlb_invalidate_guest_all();
        // for i in 0..vm.mem_region_num() {
        //     unsafe {
        //         cache_invalidate_d(vm.pa_start(i), vm.pa_length(i));
        //     }
        // }
    }
    extern "C" {
        fn context_vm_entry(ctx: usize) -> !;
    }
    unsafe {
        context_vm_entry(current_cpu().ctx as usize);
    }
}
