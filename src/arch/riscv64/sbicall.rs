/// This file is temporarily abandoned and is currently in use sbicall_legacy.rs
use core::{arch::asm, panic};

use alloc::vec::Vec;
/// This file provides the interface that the VM accesses to the upper-layer SBI.
/// For some SBI operations, Hypervisor emulation is required instead of
/// directly invoking the M-state SBI software.
use rustsbi::{
    spec::{
        binary::SbiRet,
        hsm::{HART_STATE_STARTED, HART_STATE_STOPPED},
    },
    Hsm, Ipi, MachineInfo, Pmu, Reset, RustSBI, Timer,
};
use sbi::HartMask;
use spin::Mutex;
use timer::timer_arch_get_counter;

use crate::{
    arch::{
        power_arch_cpu_on,
        riscv64::{cpu, vcpu},
        timer,
    },
    kernel::{
        active_vm, current_cpu, ipi_send_msg, CpuState, IpiInnerMsg, IpiIntInjectMsg, IpiMessage, IpiPowerMessage,
        IpiType, PowerEvent, StartReason, VcpuState, Vm, CPU_IF_LIST,
    },
};
use crate::kernel::IpiType::IpiTIntInject;
use crate::kernel::IpiInnerMsg::IntInjectMsg;

use super::{IRQ_IPI, NUM_CORE};
use crate::kernel::Scheduler;

pub struct VmHart {
    pub env: Mutex<RustSBI<VTimer, VIpi, VRfnc, VHsm, VSrst, VPmu>>,
}

#[derive(Default)]
struct VTimer {}

impl Timer for VTimer {
    fn set_timer(&self, stime_value: u64) {
        info!("set_timer: {}, current_time: {}", stime_value, timer_arch_get_counter());

        // Clear the current hart clock interrupt (triggered by setting the next timer)
        riscv::register::hvip::clear_timing_interrupt();

        // SAFETY: Enable timer interrupt
        unsafe {
            riscv::register::sie::set_stimer();
        }

        // Set the time for the next vcpu clock interruption at the VCPU level
        current_cpu()
            .active_vcpu
            .as_ref()
            .unwrap()
            .inner
            .inner_mut
            .lock()
            .vm_ctx
            .next_timer_intr = stime_value;
    }
}

#[derive(Default)]
struct VIpi {}

impl Ipi for VIpi {
    fn send_ipi(&self, hart_mask: rustsbi::HartMask) -> rustsbi::spec::binary::SbiRet {
        info!("sbi_send_ipi: {:?}", hart_mask);
        let vm = current_cpu().active_vcpu.as_ref().unwrap().vm().unwrap();
        let vm_id = vm.id();
        let (pcpu_ids, valid) = get_pcpu_ids(vm, hart_mask);

        if !valid {
            SbiRet::invalid_param()
        } else {
            for pcpu in pcpu_ids {
                // Send remote IPI request for injection interrupt
                let ok = ipi_send_msg(
                    pcpu,
                    IpiTIntInject,
                    IntInjectMsg(IpiIntInjectMsg { vm_id, int_id: IRQ_IPI }),
                );
                if !ok {
                    warn!("send ipi failed!");
                    return SbiRet::failed();
                }
            }
            SbiRet::success(0)
        }
    }
}

#[inline(always)]
fn get_pcpu_ids(vm: Vm, hart_mask: rustsbi::HartMask) -> (Vec<usize>, bool) {
    let mut ret: Vec<usize> = Vec::new();
    let nvcpu = vm.inner().lock().vcpu_list.len();
    let mut valid = true;

    for i in 0..64 {
        if hart_mask.has_bit(i) {
            // If the number of cpus selected by Hart Mask exceeds the number of vcpus, an error is returned
            if i >= nvcpu {
                valid = false;
                break;
            }

            let pcpu = vm.vcpu(i).unwrap().phys_id();
            ret.push(pcpu);
        }
    }

    (ret, valid)
}

#[derive(Default)]
struct VRfnc {}

#[inline(always)]
fn rustsbi_hart_mask_to_sbi(hart_mask: rustsbi::HartMask) -> sbi::HartMask {
    let mut base: usize = 0;
    for i in 0..64 {
        if hart_mask.has_bit(i) {
            base = i;
            break;
        }
    }

    let mask = sbi::HartMask::new(base);
    for i in 0..64 {
        if hart_mask.has_bit(i) {
            mask.with(i);
        }
    }

    mask
}

impl rustsbi::Fence for VRfnc {
    fn remote_fence_i(&self, hart_mask: rustsbi::HartMask) -> rustsbi::spec::binary::SbiRet {
        sbi::rfence::remote_fence_i(rustsbi_hart_mask_to_sbi(hart_mask)).map_or_else(
            |err| SbiRet {
                error: (-(err as isize)) as usize,
                value: 0,
            },
            |x| SbiRet { error: 0, value: 0 },
        )
    }

    fn remote_sfence_vma(
        &self,
        hart_mask: rustsbi::HartMask,
        start_addr: usize,
        size: usize,
    ) -> rustsbi::spec::binary::SbiRet {
        let sbi_mask = rustsbi_hart_mask_to_sbi(hart_mask);
        // On harts specified by hart_mask，execute hfence.vvma(vmid, start_addr, size) （vmid is from current cpu's hgatp）
        sbi::rfence::remote_hfence_vvma(sbi_mask, start_addr, size).map_or_else(
            |err| SbiRet {
                error: (-(err as isize)) as usize,
                value: 0,
            },
            |x| SbiRet { error: 0, value: 0 },
        )
    }

    fn remote_sfence_vma_asid(
        &self,
        hart_mask: rustsbi::HartMask,
        start_addr: usize,
        size: usize,
        asid: usize,
    ) -> rustsbi::spec::binary::SbiRet {
        let sbi_mask = rustsbi_hart_mask_to_sbi(hart_mask);
        sbi::rfence::remote_hfence_vvma_asid(sbi_mask, start_addr, size, asid).map_or_else(
            |err| SbiRet {
                error: (-(err as isize)) as usize,
                value: 0,
            },
            |x| SbiRet { error: 0, value: 0 },
        )
    }
}

#[derive(Default)]
struct VHsm {}

impl Hsm for VHsm {
    // TODO: needs to handle the cpu hart stop restart. This is not a real stop,
    // but into the sleep state, need to call PsciIpiCpuOn
    // similar to psci guest cpu on this function (in fact, it is also copied from the aarch64 function)
    fn hart_start(&self, hartid: usize, start_addr: usize, opaque: usize) -> rustsbi::spec::binary::SbiRet {
        let vm = active_vm().unwrap();
        let physical_linear_id = vm.vcpuid_to_pcpuid(hartid);

        if hartid > vm.cpu_num() || physical_linear_id.is_err() {
            warn!("hart_start: invalid hartid {}", hartid);
            return SbiRet::invalid_param();
        }

        let cpu_idx = physical_linear_id.unwrap();

        debug!("[hart_start] {}, {}", hartid, start_addr);

        // Get physical cpu's current status
        let state = CPU_IF_LIST.lock().get(cpu_idx).unwrap().state_for_start;

        let mut r = 0;
        if state == CpuState::CpuInv {
            // If a pcpu is in the closed state, schedule a vcpu to start the pcpu
            let mut cpu_if_list = CPU_IF_LIST.lock();
            if let Some(cpu_if) = cpu_if_list.get_mut(cpu_idx) {
                cpu_if.ctx = opaque as u64;
                cpu_if.entry = start_addr as u64;
                cpu_if.vm_id = vm.id();
                cpu_if.state_for_start = CpuState::CpuIdle;
                cpu_if.vcpuid = hartid;
                cpu_if.start_reason = StartReason::SecondaryCore;
            }
            drop(cpu_if_list);

            let entry_point = crate::arch::_secondary_start as usize;

            // SAFETY:
            // It attempts to power on the CPU specified by the cpu_idx parameter.
            // The entry is the address of function _secondary_start.
            // The ctx here is the cpu_idx.
            unsafe {
                r = power_arch_cpu_on(hartid, entry_point, cpu_idx);
            }
            debug!(
                "start to power_arch_cpu_on! hartid={:X}, entry_point={:X}",
                hartid, entry_point
            );
        } else {
            // Pass a message so that the corresponding core is started
            // Predetermined assumption: The corresponding core has been started (non-secondary start)
            let m = IpiPowerMessage {
                src: vm.id(),
                vcpuid: hartid,
                event: PowerEvent::PsciIpiVcpuAssignAndCpuOn,
                entry: start_addr,
                context: opaque,
            };

            if !ipi_send_msg(physical_linear_id.unwrap(), IpiType::IpiTPower, IpiInnerMsg::Power(m)) {
                warn!("psci_guest_cpu_on: fail to send msg");
                return SbiRet::failed();
            }
        }

        if r == 0 {
            SbiRet::success(0)
        } else {
            SbiRet::failed()
        }
    }

    fn hart_stop(&self) -> rustsbi::spec::binary::SbiRet {
        // Note: copy from aarch64 code
        // save the vcpu context for resume
        current_cpu().active_vcpu.clone().unwrap().reset_context();

        // There is no need to explicitly call do schedule
        // because sleep automatically schedules the next vcpu
        // when it detects that sleep has an active vcpu
        current_cpu()
            .scheduler()
            .sleep(current_cpu().active_vcpu.clone().unwrap());

        SbiRet::success(0)
    }

    fn hart_get_status(&self, hartid: usize) -> rustsbi::spec::binary::SbiRet {
        let vm = active_vm().unwrap();
        let vcpu_ = vm.vcpu(hartid);
        if let Some(vcpu) = vcpu_ {
            let state = vcpu.state();
            match state {
                // Both the running state and the ready state are HART_STATE_STARTED
                VcpuState::Running => SbiRet::success(HART_STATE_STARTED),
                VcpuState::Ready => SbiRet::success(HART_STATE_STARTED),
                // The inactive state and the sleep state (which is no longer functioning) are both HART_STATE_STOPPED
                VcpuState::Invalid => SbiRet::success(HART_STATE_STOPPED),
                VcpuState::Sleep => SbiRet::success(HART_STATE_STOPPED),
            }
        } else {
            SbiRet::invalid_param()
        }
    }
}

#[derive(Default)]
struct VSrst {}

impl Reset for VSrst {
    fn system_reset(&self, reset_type: u32, reset_reason: u32) -> rustsbi::spec::binary::SbiRet {
        todo!()
    }
}

#[derive(Default)]
struct VPmu {}

impl Pmu for VPmu {
    fn num_counters(&self) -> usize {
        todo!()
    }

    fn counter_get_info(&self, counter_idx: usize) -> rustsbi::spec::binary::SbiRet {
        todo!()
    }

    fn counter_config_matching(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        config_flags: usize,
        event_idx: usize,
        event_data: u64,
    ) -> rustsbi::spec::binary::SbiRet {
        todo!()
    }

    fn counter_start(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        start_flags: usize,
        initial_value: u64,
    ) -> rustsbi::spec::binary::SbiRet {
        todo!()
    }

    fn counter_stop(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        stop_flags: usize,
    ) -> rustsbi::spec::binary::SbiRet {
        todo!()
    }

    fn counter_fw_read(&self, counter_idx: usize) -> rustsbi::spec::binary::SbiRet {
        todo!()
    }
}

// The mutual exclusion of SBICALL is maintained by the **underlying function**, and the Hypervisor does not hold the lock
impl VmHart {
    pub fn new() -> Self {
        let info = MachineInfo {
            mvendorid: 0,
            marchid: 0,
            mimpid: 0,
        };
        VmHart {
            env: Mutex::new(RustSBI::with_machine_info(
                VTimer::default(),
                VIpi::default(),
                VRfnc::default(),
                VHsm::default(),
                VSrst::default(),
                VPmu::default(),
                info,
            )),
        }
    }

    /// ecall dispatch function
    #[inline(always)]
    pub fn handle_ecall(&self, extension: usize, function: usize, param: [usize; 6]) -> SbiRet {
        self.env.lock().handle_ecall(extension, function, param)
    }
}
