use crate::{
    arch::get_trapframe_for_hart,
    kernel::{current_cpu, CpuState, Scheduler, Vcpu, Vm},
};
use riscv::register::hstatus;
use sbi::{
    system_reset::{ResetType, ResetReason},
    hart_state_management::{hart_stop, hart_start},
};

use super::{A0_NUM, A1_NUM};

/// Start the given hart, letting it to jump to the 'entry'.
/// # Safety:
/// 1. 'hart_id' must be a valid hart id, within the range of 0..=max_hart_id.
/// 2. 'entry' must be a valid address for cpu to jump.
pub unsafe fn power_arch_cpu_on(hart_id: usize, entry: usize, ctx: usize) -> usize {
    // a1 = ctx
    if let Err(e) = hart_start(hart_id, entry, ctx) {
        warn!("hart {} start failed! {}", hart_id, e);
        // use non-0 number when error happens
        1
    } else {
        0
    }
}

pub fn power_arch_cpu_shutdown() {
    // TODO: Maybe just leave the cpu idle?
    match hart_stop() {
        Ok(_) => {}
        Err(e) => {
            panic!("hart_stop failed! {}", e);
        }
    }
}

/// Reset the system.
/// # Safety:
/// The platform must support reset operation.
pub unsafe fn power_arch_sys_reset() {
    sbi::system_reset::system_reset(ResetType::ColdReboot, ResetReason::NoReason).unwrap();
}

pub fn power_arch_sys_shutdown() {
    let _ = sbi::system_reset::system_reset(ResetType::Shutdown, ResetReason::NoReason);
}

#[allow(unused_variables)]
pub fn psci_vm_maincpu_on(vmpidr: usize, entry: usize, ctx: usize, vm_id: usize) -> usize {
    todo!()
}

// Reference: During secondary startup, the vcpu executes the corresponding vm context vm entry.
// Carried from the implementation of aarch64
pub fn psci_vcpu_on(vcpu: &Vcpu, entry: usize, ctx: usize) {
    if vcpu.phys_id() != current_cpu().id {
        panic!(
            "cannot psci on vcpu on cpu {} by cpu {}",
            vcpu.phys_id(),
            current_cpu().id
        );
    }
    current_cpu().cpu_state = CpuState::CpuRun;
    vcpu.reset_context();
    vcpu.set_gpr(A0_NUM, vcpu.id());
    vcpu.set_gpr(A1_NUM, ctx);
    vcpu.set_elr(entry);

    // Just wake up the vcpu and
    // invoke current_cpu().sched.schedule()
    // let the scheduler enable or disable timer
    current_cpu().scheduler().wakeup(vcpu.clone());
    current_cpu().scheduler().do_schedule();

    // Note: There is no need to turn on the clock interrupt here, because it is already turned on with interrupt init

    #[cfg(target_arch = "riscv64")]
    {
        // Set sscratch for saving VM's TrapFrame
        current_cpu().ctx_mut().unwrap().sscratch = get_trapframe_for_hart(current_cpu().id);
    }

    info!(
        "(Secondary vcpu start) vcpu on cpu {} begins running: \nhstatus: {:#x}, entry: {:#x}, use ctx:\n{}",
        current_cpu().id,
        hstatus::read(),
        entry,
        current_cpu().ctx_mut().unwrap()
    );

    // VM's vcpu's first booting must be entering context_vm_entry
    // TODO: The vcpu cannot be started after hart is shut down
    extern "C" {
        fn context_vm_entry(ctx: usize) -> !;
    }
    // SAFETY: current_cpu().ctx_ptr() is a valid pointer to a ContextFrame, which was inited by previous code.
    unsafe {
        context_vm_entry(current_cpu().ctx_ptr().unwrap() as usize);
    }
}

#[allow(unused_variables)]
pub fn power_arch_vm_shutdown_secondary_cores(vm: &Vm) {
    todo!()
}

#[cfg(not(feature = "secondary_start"))]
pub fn guest_cpu_on(_mpidr: usize) {}

#[cfg(feature = "secondary_start")]
pub fn guest_cpu_on(_mpidr: usize) {
    todo!()
}
