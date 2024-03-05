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
use core::ptr;

use spin::Mutex;

use crate::arch::{PAGE_SIZE, set_current_cpu};

use crate::arch::ContextFrame;
use crate::arch::{wfi, isb};
use crate::arch::ContextFrameTrait;
// use core::ops::{Deref, DerefMut};
use crate::arch::{cpu_interrupt_unmask, current_cpu_arch};
use crate::board::{PLATFORM_CPU_NUM_MAX, Platform, PlatOperation};
use crate::kernel::{SchedType, Vcpu, VcpuArray, VcpuState, Vm, Scheduler};
use crate::kernel::IpiMessage;
use crate::utils::trace;

pub const CPU_MASTER: usize = 0;
pub const CPU_STACK_SIZE: usize = PAGE_SIZE * 128;
pub const CONTEXT_GPR_NUM: usize = 31;
pub const CPU_STACK_OFFSET: usize = offset_of!(Cpu, stack);

#[derive(Copy, Clone, Debug, Eq)]
/// CPU state Enum
pub enum CpuState {
    CpuInv = 0,
    CpuIdle = 1,
    CpuRun = 2,
}

impl PartialEq for CpuState {
    fn eq(&self, other: &Self) -> bool {
        *self as usize == *other as usize
    }
}

#[derive(Copy, Clone, Debug)]
pub enum StartReason {
    MainCore,
    SecondaryCore,
    None,
}

/// A struct to store the information of a CPU
pub struct CpuIf {
    pub msg_queue: Vec<IpiMessage>,
    pub entry: u64,
    pub ctx: u64,
    pub vm_id: usize,
    pub state_for_start: CpuState,
    pub vcpuid: usize,
    pub start_reason: StartReason,
}

impl CpuIf {
    pub fn default() -> CpuIf {
        CpuIf {
            msg_queue: Vec::new(),
            entry: 0,
            ctx: 0,
            vm_id: 0,
            state_for_start: CpuState::CpuInv,
            vcpuid: 0,
            start_reason: StartReason::None,
        }
    }

    pub fn push(&mut self, ipi_msg: IpiMessage) {
        self.msg_queue.push(ipi_msg);
    }

    pub fn pop(&mut self) -> Option<IpiMessage> {
        self.msg_queue.pop()
    }
}

/// stores the information of all CPUs, which count is the number of CPU on the platform
pub static CPU_IF_LIST: Mutex<Vec<CpuIf>> = Mutex::new(Vec::new());

fn cpu_if_init() {
    let mut cpu_if_list = CPU_IF_LIST.lock();
    for _ in 0..PLATFORM_CPU_NUM_MAX {
        cpu_if_list.push(CpuIf::default());
    }
}

#[repr(C, align(4096))]
struct CpuStack([u8; CPU_STACK_SIZE]);

impl core::ops::Deref for CpuStack {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[repr(C, align(4096))]
pub struct Cpu {
    pub id: usize,
    pub cpu_state: CpuState,
    pub active_vcpu: Option<Vcpu>,
    pub ctx: *mut ContextFrame,

    pub sched: SchedType,
    pub vcpu_array: VcpuArray,
    pub current_irq: usize,
    stack: CpuStack,
}

impl Cpu {
    const fn default() -> Cpu {
        Cpu {
            id: 0,
            cpu_state: CpuState::CpuInv,
            active_vcpu: None,
            ctx: ptr::null_mut(),
            sched: SchedType::None,
            vcpu_array: VcpuArray::new(),
            current_irq: 0,
            stack: CpuStack([0; CPU_STACK_SIZE]),
        }
    }

    /// # Safety:
    /// The caller must ensure that the `ctx` is valid.
    /// ctx must be aligned to 8 bytes
    pub unsafe fn set_ctx(&mut self, ctx: *mut ContextFrame) {
        self.ctx = ctx;
    }

    pub fn clear_ctx(&mut self) {
        self.ctx = ptr::null_mut();
    }

    pub fn ctx(&self) -> Option<&ContextFrame> {
        self.ctx_ptr().map(|addr| unsafe { &*addr })
    }

    pub fn ctx_mut(&self) -> Option<&mut ContextFrame> {
        self.ctx_ptr().map(|addr| unsafe { &mut *addr })
    }

    pub fn ctx_ptr(&self) -> Option<*mut ContextFrame> {
        if self.ctx.is_null() {
            None
        } else {
            if trace() && (self.ctx as usize) < 0x1000 {
                panic!("illegal ctx addr {:p}", self.ctx);
            }
            Some(self.ctx)
        }
    }

    pub fn set_gpr(&self, idx: usize, val: usize) {
        if idx >= CONTEXT_GPR_NUM {
            return;
        }
        self.ctx_mut().unwrap().set_gpr(idx, val)
    }

    pub fn get_gpr(&self, idx: usize) -> usize {
        if idx >= CONTEXT_GPR_NUM {
            return 0;
        }
        self.ctx_mut().unwrap().gpr(idx)
    }

    pub fn get_elr(&self) -> usize {
        self.ctx().unwrap().exception_pc()
    }

    pub fn get_spsr(&self) -> usize {
        self.ctx().unwrap().spsr as usize
    }

    pub fn set_elr(&self, val: usize) {
        self.ctx_mut().unwrap().set_exception_pc(val)
    }

    /// set a active vcpu for this physical cpu
    pub fn set_active_vcpu(&mut self, active_vcpu: Option<Vcpu>) {
        self.active_vcpu = active_vcpu.clone();
        match active_vcpu {
            None => {}
            Some(vcpu) => {
                vcpu.set_state(VcpuState::Running);
            }
        }
    }

    /// schedule a vcpu to run on this physical cpu
    pub fn schedule_to(&mut self, next_vcpu: Vcpu) {
        if let Some(prev_vcpu) = &self.active_vcpu {
            if prev_vcpu.vm_id() != next_vcpu.vm_id() {
                prev_vcpu.set_state(VcpuState::Ready);
                prev_vcpu.context_vm_store();
            }
        }
        // NOTE: Must set active first and then restore context!!!
        //      because context restore while inject pending interrupt for VM
        //      and will judge if current active vcpu
        self.set_active_vcpu(Some(next_vcpu.clone()));
        next_vcpu.context_vm_restore();
        // restore vm's Stage2 MMU context
        let vttbr = (next_vcpu.vm_id() << 48) | next_vcpu.vm_pt_dir();
        // TODO: replace the arch related expr
        // SAFETY: 'vttbr' is saved in the vcpu struct when last scheduled
        unsafe {
            core::arch::asm!("msr VTTBR_EL2, {0}", in(reg) vttbr);
            isb();
        }
    }

    /// get this cpu's scheduler
    pub fn scheduler(&mut self) -> &mut impl Scheduler {
        match &mut self.sched {
            SchedType::None => panic!("scheduler is None"),
            SchedType::SchedRR(rr) => rr,
        }
    }

    /// check whether this cpu is assigned to one or more vm
    pub fn assigned(&self) -> bool {
        self.vcpu_array.vcpu_num() != 0
    }

    pub fn stack_top(&self) -> usize {
        self.stack.as_ptr_range().end as usize
    }
}

pub fn current_cpu() -> &'static mut Cpu {
    // SAFETY: The value of current_cpu_arch() is valid setted by cpu_map_self at boot_stage
    unsafe { &mut *(current_cpu_arch() as *mut Cpu) }
}

pub fn active_vcpu_id() -> usize {
    match current_cpu().active_vcpu.clone() {
        Some(active_vcpu) => active_vcpu.id(),
        None => 0xFFFFFFFF,
    }
}

pub fn active_vm_id() -> usize {
    let vm = active_vm().unwrap();
    vm.id()
}

pub fn active_vm() -> Option<alloc::sync::Arc<Vm>> {
    match current_cpu().active_vcpu.as_ref() {
        None => None,
        Some(active_vcpu) => active_vcpu.vm(),
    }
}

pub fn active_vm_ncpu() -> usize {
    match active_vm() {
        Some(vm) => vm.ncpu(),
        None => 0,
    }
}

/// initialize the CPU
pub fn cpu_init() {
    let cpu_id = current_cpu().id;
    if cpu_id == 0 {
        cpu_if_init();
        if cfg!(not(feature = "secondary_start")) {
            Platform::power_on_secondary_cores();
        }
    }

    let state = CpuState::CpuIdle;
    current_cpu().cpu_state = state;
    let sp = current_cpu().stack.as_ptr() as usize + CPU_STACK_SIZE;
    let size = core::mem::size_of::<ContextFrame>();
    // SAFETY: Sp is valid when boot_stage setting
    unsafe {
        current_cpu().set_ctx((sp - size) as *mut _);
    }
    info!("Core {} init ok", cpu_id);

    if cfg!(not(feature = "secondary_start")) {
        crate::utils::barrier();
        // println!("after barrier cpu init");
        use crate::board::PLAT_DESC;
        if cpu_id == 0 {
            info!("Bring up {} cores", PLAT_DESC.cpu_desc.num);
            info!("Cpu init ok");
        }
    }
}

/// make the current cpu idle
pub fn cpu_idle() -> ! {
    let state = CpuState::CpuIdle;
    current_cpu().cpu_state = state;
    cpu_interrupt_unmask();
    loop {
        wfi();
    }
}

/// store all cpu's CPU struct in this array
pub static mut CPU_LIST: [Cpu; PLATFORM_CPU_NUM_MAX] = [const { Cpu::default() }; PLATFORM_CPU_NUM_MAX];
pub extern "C" fn cpu_map_self(mpidr: usize) {
    let cpu_id = Platform::mpidr2cpuid(mpidr);
    // SAFETY:
    // One core only call this function once
    // And it will get the reference of the CPU_LIST[cpu_id] by cpu_id
    // So it won't influence other cores
    let cpu = unsafe { &mut CPU_LIST[cpu_id] };
    cpu.id = cpu_id;
    // SAFETY:
    // The 'cpu' is a valid reference of CPU_LIST[cpu_id]
    unsafe {
        set_current_cpu(cpu as *const _ as u64);
    }
}
