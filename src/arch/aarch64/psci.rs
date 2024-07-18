// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use crate::arch::{gic_cpu_init, gicc_clear_current_irq};
use crate::board::{Platform, PlatOperation};
use crate::kernel::{cpu_idle, current_cpu, ipi_intra_broadcast_msg, Scheduler, timer_enable, Vcpu, VcpuState, Vm};
use crate::kernel::{active_vm, ipi_send_msg, IpiInnerMsg, IpiPowerMessage, IpiType, PowerEvent};
use crate::kernel::CpuState;
use crate::kernel::IpiMessage;
use crate::vmm::vmm_reboot;
use crate::kernel::vm;
use smccc::psci::{LowestAffinityLevel, SuspendMode};
use smccc::{self, Smc};

use super::smc::smc_call;

// RKNPU requires this feature to be reported by the PSCI_FEATURES call.
const SMCCC_VERSION: usize = 0x80000000;

pub const PSCI_VERSION: usize = 0x84000000;
pub const PSCI_CPU_SUSPEND_32: usize = 0x84000001;
pub const PSCI_CPU_SUSPEND_64: usize = 0xC4000001;
pub const PSCI_CPU_OFF: usize = 0x84000002;
pub const PSCI_CPU_ON_32: usize = 0x84000003;
pub const PSCI_CPU_ON_64: usize = 0xC4000003;
pub const PSCI_AFFINITY_INFO_32: usize = 0x84000004;
pub const PSCI_AFFINITY_INFO_64: usize = 0xC4000004;
pub const PSCI_MIGRATE_32: usize = 0x84000005;
pub const PSCI_MIGRATE_64: usize = 0xC4000005;
pub const PSCI_MIGRATE_INFO_TYPE: usize = 0x84000006;
pub const PSCI_MIGRATE_INFO_UP_CPU_32: usize = 0x84000007;
pub const PSCI_MIGRATE_INFO_UP_CPU_64: usize = 0xC4000007;
pub const PSCI_SYSTEM_OFF: usize = 0x84000008;
pub const PSCI_SYSTEM_RESET: usize = 0x84000009;
pub const PSCI_SYSTEM_RESET2_32: usize = 0x84000012;
pub const PSCI_SYSTEM_RESET2_64: usize = 0xC4000012;
pub const PSCI_MEM_PROTECT: usize = 0x84000013;
pub const PSCI_MEM_PROTECT_CHECK_RANGE_32: usize = 0x84000014;
pub const PSCI_MEM_PROTECT_CHECK_RANGE_64: usize = 0xC4000014;
pub const PSCI_FEATURES: usize = 0x8400000A;
pub const PSCI_CPU_FREEZE: usize = 0x8400000B;
pub const PSCI_CPU_DEFAULT_SUSPEND_32: usize = 0x8400000C;
pub const PSCI_CPU_DEFAULT_SUSPEND_64: usize = 0xC400000C;
pub const PSCI_NODE_HW_STATE_32: usize = 0x8400000D;
pub const PSCI_NODE_HW_STATE_64: usize = 0xC400000D;
pub const PSCI_SYSTEM_SUSPEND_32: usize = 0x8400000E;
pub const PSCI_SYSTEM_SUSPEND_64: usize = 0xC400000E;
pub const PSCI_SET_SUSPEND_MODE: usize = 0x8400000F;
pub const PSCI_STAT_RESIDENCY_32: usize = 0x84000010;
pub const PSCI_STAT_RESIDENCY_64: usize = 0xC4000010;
pub const PSCI_STAT_COUNT_32: usize = 0x84000011;
pub const PSCI_STAT_COUNT_64: usize = 0xC4000011;

pub const PSCI_E_SUCCESS: usize = 0;
pub const PSCI_E_NOT_SUPPORTED: usize = usize::MAX;

#[cfg(feature = "tx2")]
const TEGRA_SIP_GET_ACTMON_CLK_COUNTERS: usize = 0xC2FFFE02;

pub const PSCI_TOS_NOT_PRESENT_MP: usize = 2;

pub fn power_arch_vm_shutdown_secondary_cores(vm: &Vm) {
    let m = IpiPowerMessage {
        src: vm.id(),
        vcpuid: 0,
        event: PowerEvent::PsciIpiCpuReset,
        entry: 0,
        context: 0,
    };

    if !ipi_intra_broadcast_msg(vm, IpiType::IpiTPower, IpiInnerMsg::Power(m)) {
        warn!("power_arch_vm_shutdown_secondary_cores: fail to ipi_intra_broadcast_msg");
    }
}

/// # Safety:
/// It attempts to power on the CPU specified by the mpidr parameter.
/// The entry is the vaild entry point of the CPU to be powered on.
/// The ctx here is the cpu_idx.
pub unsafe fn power_arch_cpu_on(mpidr: usize, entry: usize, ctx: usize) -> usize {
    use crate::kernel::CPU_IF_LIST;

    let cpu_idx = Platform::mpidr2cpuid(mpidr);
    let mut cpu_if_list = CPU_IF_LIST.lock();
    if let Some(cpu_if) = cpu_if_list.get_mut(cpu_idx) {
        cpu_if.state_for_start = CpuState::CpuIdle;
    }
    drop(cpu_if_list);

    smc_call(PSCI_CPU_ON_64, mpidr, entry, ctx).0
}

pub fn power_arch_cpu_shutdown() {
    gic_cpu_init();
    gicc_clear_current_irq(true);
    timer_enable(false);
    cpu_idle();
}

/// # Safety:
/// It attempts to reset the system.
/// So the caller must ensure that the system can be reset.
pub unsafe fn power_arch_sys_reset() {
    smc_call(PSCI_SYSTEM_RESET, 0, 0, 0);
}

/// # Safety:
/// It attempts to shutdown the system.
/// So the caller must ensure that the system can be shutdown.
pub unsafe fn power_arch_sys_shutdown() {
    smc_call(PSCI_SYSTEM_OFF, 0, 0, 0);
}

fn psci_guest_sys_reset() {
    vmm_reboot();
}

#[inline(never)]
pub fn smc_guest_handler(fid: usize, x1: usize, x2: usize, x3: usize) -> bool {
    debug!(
        "smc_guest_handler: fid 0x{:x}, x1 0x{:x}, x2 0x{:x}, x3 0x{:x}",
        fid, x1, x2, x3
    );
    let r = match fid {
        PSCI_VERSION => smccc::psci::version::<Smc>() as usize,
        PSCI_CPU_SUSPEND_64 => {
            // save the vcpu contex for resume
            current_cpu().active_vcpu.clone().unwrap().reset_context();
            current_cpu().active_vcpu.clone().unwrap().set_gpr(0, x3);
            current_cpu().active_vcpu.clone().unwrap().set_elr(x2);
            current_cpu()
                .scheduler()
                .sleep(current_cpu().active_vcpu.clone().unwrap());
            PSCI_E_SUCCESS
        }
        PSCI_CPU_OFF => match smccc::psci::cpu_off::<Smc>() {
            Ok(()) => PSCI_E_SUCCESS,
            _ => PSCI_E_NOT_SUPPORTED,
        },
        PSCI_CPU_ON_64 => psci_guest_cpu_on(x1, x2, x3),
        PSCI_AFFINITY_INFO_64 => {
            let lowest = match x2 {
                0 => LowestAffinityLevel::All,
                1 => LowestAffinityLevel::Aff0Ignored,
                2 => LowestAffinityLevel::Aff0Aff1Ignored,
                _ => LowestAffinityLevel::Aff0Aff1Aff2Ignored,
            };
            match smccc::psci::affinity_info::<Smc>(x1 as u64, lowest) {
                Ok(affinity_state) => affinity_state as usize,
                _ => PSCI_E_NOT_SUPPORTED,
            }
        }
        PSCI_MIGRATE_64 => match smccc::psci::migrate::<Smc>(x1 as u64) {
            Ok(()) => PSCI_E_SUCCESS,
            _ => PSCI_E_NOT_SUPPORTED,
        },
        PSCI_MIGRATE_INFO_TYPE => match smccc::psci::migrate_info_type::<Smc>() {
            Ok(migrate_type) => migrate_type as usize,
            _ => PSCI_E_NOT_SUPPORTED,
        },
        PSCI_MIGRATE_INFO_UP_CPU_64 => smccc::psci::migrate_info_up_cpu::<Smc>() as usize,
        PSCI_SYSTEM_OFF => match smccc::psci::system_off::<Smc>() {
            Ok(()) => PSCI_E_SUCCESS,
            _ => PSCI_E_NOT_SUPPORTED,
        },
        PSCI_SYSTEM_RESET => {
            psci_guest_sys_reset();
            match smccc::psci::system_reset::<Smc>() {
                Ok(()) => PSCI_E_SUCCESS,
                _ => PSCI_E_NOT_SUPPORTED,
            }
        }
        PSCI_SYSTEM_RESET2_64 => match smccc::psci::system_reset2::<Smc>(x1 as u32, x2 as u64) {
            Ok(()) => PSCI_E_SUCCESS,
            _ => PSCI_E_NOT_SUPPORTED,
        },
        PSCI_MEM_PROTECT => match smccc::psci::mem_protect::<Smc>(x1 != 0) {
            Ok(res) => res as usize,
            _ => PSCI_E_NOT_SUPPORTED,
        },
        PSCI_MEM_PROTECT_CHECK_RANGE_64 => match smccc::psci::mem_protect_check_range::<Smc>(x1 as u64, x2 as u64) {
            Ok(()) => PSCI_E_SUCCESS,
            _ => PSCI_E_NOT_SUPPORTED,
        },
        #[cfg(feature = "tx2")]
        TEGRA_SIP_GET_ACTMON_CLK_COUNTERS => {
            let result = unsafe { smc_call(fid, x1, x2, x3) };
            current_cpu().set_gpr(1, result.1);
            current_cpu().set_gpr(2, result.2);
            result.0
        }
        PSCI_FEATURES => match x1 {
            PSCI_VERSION | PSCI_CPU_ON_64 | PSCI_FEATURES | SMCCC_VERSION => PSCI_E_SUCCESS,
            _ => PSCI_E_NOT_SUPPORTED,
        },
        PSCI_CPU_FREEZE => match smccc::psci::cpu_freeze::<Smc>() {
            Ok(()) => PSCI_E_SUCCESS,
            _ => PSCI_E_NOT_SUPPORTED,
        },
        PSCI_CPU_DEFAULT_SUSPEND_64 => match smccc::psci::cpu_default_suspend::<Smc>(x1 as u64, x2 as u64) {
            Ok(()) => PSCI_E_SUCCESS,
            _ => PSCI_E_NOT_SUPPORTED,
        },
        PSCI_NODE_HW_STATE_64 => match smccc::psci::node_hw_state::<Smc>(x1 as u64, x2 as u32) {
            Ok(res) => res as usize,
            _ => PSCI_E_NOT_SUPPORTED,
        },
        PSCI_SYSTEM_SUSPEND_64 => match smccc::psci::system_suspend::<Smc>(x1 as u64, x2 as u64) {
            Ok(()) => PSCI_E_SUCCESS,
            _ => PSCI_E_NOT_SUPPORTED,
        },
        PSCI_SET_SUSPEND_MODE => {
            let mode = match x1 {
                0 => SuspendMode::PlatformCoordinated,
                _ => SuspendMode::OsInitiated,
            };
            match smccc::psci::set_suspend_mode::<Smc>(mode) {
                Ok(()) => PSCI_E_SUCCESS,
                _ => PSCI_E_NOT_SUPPORTED,
            }
        }
        PSCI_STAT_RESIDENCY_64 => smccc::psci::stat_residency::<Smc>(x1 as u64, x2 as u32) as usize,
        PSCI_STAT_COUNT_64 => smccc::psci::stat_count::<Smc>(x1 as u64, x2 as u32) as usize,
        _ => {
            // unimplemented!();
            return false;
        }
    };

    let idx = 0;
    let val = r;
    current_cpu().set_gpr(idx, val);

    true
}

fn psci_vcpu_on(vcpu: &Vcpu, entry: usize, ctx: usize) {
    if vcpu.phys_id() != current_cpu().id {
        panic!(
            "cannot psci on vcpu on cpu {} by cpu {}",
            vcpu.phys_id(),
            current_cpu().id
        );
    }
    current_cpu().cpu_state = CpuState::CpuRun;
    vcpu.reset_context();
    vcpu.set_gpr(0, ctx);
    vcpu.set_elr(entry);
    // Just wake up the vcpu and
    // invoke current_cpu().sched.schedule()
    // let the scheduler enable or disable timer
    current_cpu().scheduler().wakeup(vcpu.clone());
    current_cpu().scheduler().do_schedule();

    if cfg!(feature = "secondary_start") {
        extern "C" {
            fn context_vm_entry(ctx: usize) -> !;
        }
        unsafe {
            context_vm_entry(current_cpu().ctx_ptr().unwrap() as usize);
        }
    }
}

// Todo: need to support more vcpu in one Core
pub fn psci_ipi_handler(msg: IpiMessage) {
    match msg.ipi_message {
        IpiInnerMsg::Power(power_msg) => {
            if let PowerEvent::PsciIpiVcpuAssignAndCpuOn = power_msg.event {
                trace!("receive PsciIpiVcpuAssignAndCpuOn msg");
                let vm = vm(power_msg.src).unwrap();
                let vcpu = vm.vcpuid_to_vcpu(power_msg.vcpuid).unwrap();
                current_cpu().vcpu_array.append_vcpu(vcpu);
            }
            let trgt_vcpu = match current_cpu().vcpu_array.pop_vcpu_through_vmid(power_msg.src) {
                None => {
                    warn!(
                        "Core {} failed to find target vcpu, source vmid {}",
                        current_cpu().id,
                        power_msg.src
                    );
                    return;
                }
                Some(vcpu) => vcpu,
            };
            match power_msg.event {
                PowerEvent::PsciIpiCpuOn => {
                    if trgt_vcpu.state() as usize != VcpuState::Invalid as usize {
                        warn!(
                            "psci_ipi_handler: target VCPU {} in VM {} is already running",
                            trgt_vcpu.id(),
                            trgt_vcpu.vm().unwrap().id()
                        );
                        return;
                    }
                    info!(
                        "Core {} (vm {}, vcpu {}) is woke up",
                        current_cpu().id,
                        trgt_vcpu.vm().unwrap().id(),
                        trgt_vcpu.id()
                    );
                    psci_vcpu_on(trgt_vcpu, power_msg.entry, power_msg.context);
                }
                PowerEvent::PsciIpiCpuOff => {
                    unimplemented!("PowerEvent::PsciIpiCpuOff")
                }
                PowerEvent::PsciIpiCpuReset => {
                    let vcpu = current_cpu().active_vcpu.as_ref().unwrap();
                    vcpu.init_boot_info(active_vm().unwrap().config());
                }
                _ => {}
            }
        }
        _ => {
            panic!(
                "psci_ipi_handler: cpu{} receive illegal psci ipi type",
                current_cpu().id
            );
        }
    }
}

#[cfg(feature = "secondary_start")]
pub fn psci_guest_cpu_on(vmpidr: usize, entry: usize, ctx: usize) -> usize {
    let vcpu_id = Platform::vmpidr2vcpuid(vmpidr);
    let vm = active_vm().unwrap();
    let physical_linear_id = vm.vcpuid_to_pcpuid(vcpu_id);

    if vcpu_id >= vm.cpu_num() || physical_linear_id.is_err() {
        warn!("psci_guest_cpu_on: target vcpu {} not exist", vcpu_id);
        return usize::MAX - 1;
    }

    let cpu_idx = physical_linear_id.unwrap();

    debug!("[psci_guest_cpu_on] {vmpidr}, {cpu_idx}");

    use crate::kernel::CPU_IF_LIST;
    use crate::kernel::StartReason;
    let state = CPU_IF_LIST.lock().get(cpu_idx).unwrap().state_for_start;
    let mut r = 0;
    if state == CpuState::CpuInv {
        let mut cpu_if_list = CPU_IF_LIST.lock();
        if let Some(cpu_if) = cpu_if_list.get_mut(cpu_idx as usize) {
            cpu_if.ctx = ctx as u64;
            cpu_if.entry = entry as u64;
            cpu_if.vm_id = vm.id();
            cpu_if.state_for_start = CpuState::CpuIdle;
            cpu_if.vcpuid = vcpu_id;
            cpu_if.start_reason = StartReason::SecondaryCore;
        }
        drop(cpu_if_list);
        let mpidr = Platform::cpuid2mpidr(cpu_idx);

        let entry_point = crate::arch::_secondary_start as usize;
        // SAFETY:
        // It attempts to power on the CPU specified by the mpidr parameter.
        // The entry is the address of function _secondary_start.
        // The ctx here is the cpu_idx.
        unsafe {
            r = power_arch_cpu_on(mpidr, entry_point, cpu_idx);
        }
        debug!(
            "start to power_arch_cpu_on! mpidr={:X}, entry_point={:X}",
            mpidr, entry_point
        );
    } else {
        let m = IpiPowerMessage {
            src: vm.id(),
            vcpuid: vcpu_id,
            event: PowerEvent::PsciIpiVcpuAssignAndCpuOn,
            entry,
            context: ctx,
        };

        if !ipi_send_msg(physical_linear_id.unwrap(), IpiType::IpiTPower, IpiInnerMsg::Power(m)) {
            warn!("psci_guest_cpu_on: fail to send msg");
            return usize::MAX - 1;
        }
    }
    r
}

#[cfg(not(feature = "secondary_start"))]
pub fn psci_guest_cpu_on(vmpidr: usize, entry: usize, ctx: usize) -> usize {
    let vcpu_id = Platform::vmpidr2vcpuid(vmpidr);
    let vm = active_vm().unwrap();
    let physical_linear_id = vm.vcpuid_to_pcpuid(vcpu_id);

    if vcpu_id >= vm.cpu_num() || physical_linear_id.is_err() {
        warn!("psci_guest_cpu_on: target vcpu {} not exist", vcpu_id);
        return usize::MAX - 1;
    }
    #[cfg(feature = "tx2")]
    {
        let cluster = (vmpidr >> 8) & 0xff;
        if vm.id() == 0 && cluster != 1 {
            warn!("psci_guest_cpu_on: L4T only support cluster #1");
            return usize::MAX - 1;
        }
    }

    let m = IpiPowerMessage {
        src: vm.id(),
        vcpuid: 0,
        event: PowerEvent::PsciIpiCpuOn,
        entry,
        context: ctx,
    };

    if !ipi_send_msg(physical_linear_id.unwrap(), IpiType::IpiTPower, IpiInnerMsg::Power(m)) {
        warn!("psci_guest_cpu_on: fail to send msg");
        return usize::MAX - 1;
    }

    0
}

pub fn psci_vm_maincpu_on(vmpidr: usize, entry: usize, ctx: usize, vm_id: usize) -> usize {
    let vcpu_id = Platform::vmpidr2vcpuid(vmpidr);
    let vm = vm(vm_id).unwrap();
    let physical_linear_id = vm.vcpuid_to_pcpuid(vcpu_id);

    if vcpu_id >= vm.cpu_num() || physical_linear_id.is_err() {
        warn!("psci_vm_maincpu_on: target vcpu {} not exist", vcpu_id);
        return usize::MAX - 1;
    }

    let cpu_idx = physical_linear_id.unwrap();

    use crate::kernel::{CPU_IF_LIST, StartReason};
    let state = CPU_IF_LIST.lock().get(cpu_idx).unwrap().state_for_start;
    let mut r = 0;
    if state == CpuState::CpuInv {
        let mut cpu_if_list = CPU_IF_LIST.lock();
        if let Some(cpu_if) = cpu_if_list.get_mut(cpu_idx as usize) {
            cpu_if.ctx = ctx as u64;
            cpu_if.entry = entry as u64;
            cpu_if.vm_id = vm.id();
            cpu_if.state_for_start = CpuState::CpuIdle;
            cpu_if.vcpuid = vcpu_id;
            cpu_if.start_reason = StartReason::MainCore;
        }
        drop(cpu_if_list);

        let mpidr = Platform::cpuid2mpidr(cpu_idx);

        let entry_point = crate::arch::_secondary_start as usize;
        // SAFETY:
        // It attempts to power on the CPU specified by the mpidr parameter.
        // The entry is the address of function _secondary_start.
        // The ctx here is the cpu_idx.
        r = unsafe { power_arch_cpu_on(mpidr, entry_point, cpu_idx) };
        debug!(
            "start to power_arch_cpu_on! mpidr={:X}, entry_point={:X}",
            mpidr, entry_point
        );
    } else {
        let m = IpiPowerMessage {
            src: vm.id(),
            vcpuid: vcpu_id,
            event: PowerEvent::PsciIpiVcpuAssignAndCpuOn,
            entry,
            context: ctx,
        };

        if !ipi_send_msg(physical_linear_id.unwrap(), IpiType::IpiTPower, IpiInnerMsg::Power(m)) {
            warn!("psci_guest_cpu_on: fail to send msg");
            return usize::MAX - 1;
        }
    }

    r
}

#[cfg(feature = "secondary_start")]
pub fn guest_cpu_on(mpidr: usize) {
    let cpu_id = Platform::mpidr2cpuid(mpidr);

    use crate::kernel::{CPU_IF_LIST, StartReason};
    let cpu_if_list = CPU_IF_LIST.lock();
    let cpu_if = &cpu_if_list[cpu_id];

    let vm_id = cpu_if.vm_id;
    let entry = cpu_if.entry;
    let ctx = cpu_if.ctx;
    let vcpuid = cpu_if.vcpuid;
    let start_reason = cpu_if.start_reason;

    drop(cpu_if_list);

    let vm = vm(vm_id).unwrap();
    let vcpu = vm.vcpuid_to_vcpu(vcpuid).unwrap();

    debug!("now add vcpu={} to mpidr={:X}", vcpu.id(), mpidr);

    current_cpu().vcpu_array.append_vcpu(vcpu);

    match start_reason {
        StartReason::MainCore => {
            use crate::vmm::vmm_boot_vm;
            vmm_boot_vm(vm_id);
        }
        StartReason::SecondaryCore => {
            if let Some(trgt_vcpu) = current_cpu().vcpu_array.pop_vcpu_through_vmid(vm_id) {
                psci_vcpu_on(trgt_vcpu, entry as usize, ctx as usize);
            } else {
                error!("pop_vcpu_through_vmid error!");
            }
        }
        _ => {
            todo!()
        }
    }
}

#[cfg(not(feature = "secondary_start"))]
pub fn guest_cpu_on(_mpidr: usize) {}
