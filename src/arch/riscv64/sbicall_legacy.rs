use alloc::vec::Vec;
/// This file provides the interface that the VM accesses to the upper-layer SBI.
/// For some SBI operations, Hypervisor emulation is required instead of
/// directly invoking the M-state SBI software.
use rustsbi::{
    init_hsm, init_ipi, init_pmu, init_remote_fence, init_reset, init_timer,
    spec::{
        base::{impl_id::KVM, EID_BASE, GET_MARCHID, GET_MIMPID, GET_MVENDORID},
        binary::SbiRet,
        hsm::{HART_STATE_STARTED, HART_STATE_STOPPED},
    },
    Hsm, Ipi, Pmu, Reset, Timer,
};
use rustsbi::legacy_stdio::LegacyStdio;
use rustsbi::legacy_stdio::init_legacy_stdio;

use crate::kernel::{
    active_vm, current_cpu, ipi_send_msg, IpiInnerMsg, IpiIntInjectMsg, IpiPowerMessage, IpiType, PowerEvent,
    VcpuState, Vm,
};
use crate::kernel::IpiType::IpiTIntInject;
use crate::kernel::IpiInnerMsg::IntInjectMsg;

use super::IRQ_IPI;
use crate::kernel::Scheduler;

#[derive(Default)]
struct VTimer {}

impl Timer for VTimer {
    fn set_timer(&self, stime_value: u64) {
        // Clear the current hart clock interrupt (triggered by setting the next timer)
        riscv::register::hvip::clear_timing_interrupt();

        // SAFETY: Enable timer interrupt
        unsafe { riscv::register::sie::set_stimer() };

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
        let vm = current_cpu().active_vcpu.as_ref().unwrap().vm().unwrap();
        let vm_id = vm.id();
        let (pcpu_ids, valid) = get_pcpu_ids(&vm, hart_mask);

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
fn get_pcpu_ids(vm: &Vm, hart_mask: rustsbi::HartMask) -> (Vec<usize>, bool) {
    let mut ret: Vec<usize> = Vec::new();
    let nvcpu = vm.cpu_num();
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

// Convert the hart mask in rustsbi format to the hart mask in sbi format
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
            let _ = mask.with(i);
        }
    }

    mask
}

// Converts the vCPU-based hart mask to the hart mask of the pcpu corresponding to the vcpu of the current vm
// If an error occurs (for example, the vcpu core number exceeds the number of Vcpus on the vm), return an empty HartMask
#[inline(always)]
fn vcpu_hart_mask_to_pcpu_mask(hart_mask: rustsbi::HartMask) -> sbi::HartMask {
    let (pcpus, valid) = get_pcpu_ids(&active_vm().unwrap(), hart_mask.clone());
    if valid {
        let mask = sbi::HartMask::new(0);
        for pcpu in pcpus {
            let _ = mask.with(pcpu);
        }
        mask
    } else {
        // no core selected
        warn!(
            "vcpu_hart_mask_to_pcpu_mask: no core selected since invalid hart_mask: {:?}!",
            hart_mask
        );
        sbi::HartMask::new(0)
    }
}

// hart mask is parsed by vcpu, not by pcpu
impl rustsbi::Fence for VRfnc {
    fn remote_fence_i(&self, hart_mask: rustsbi::HartMask) -> rustsbi::spec::binary::SbiRet {
        sbi::rfence::remote_fence_i(vcpu_hart_mask_to_pcpu_mask(hart_mask)).map_or_else(
            |err| SbiRet {
                error: (-(err as isize)) as usize,
                value: 0,
            },
            |_x| SbiRet { error: 0, value: 0 },
        )
    }

    fn remote_sfence_vma(
        &self,
        hart_mask: rustsbi::HartMask,
        start_addr: usize,
        size: usize,
    ) -> rustsbi::spec::binary::SbiRet {
        let sbi_mask = vcpu_hart_mask_to_pcpu_mask(hart_mask);
        // On harts specified by hart_mask，execute hfence.vvma(vmid, start_addr, size) （vmid is from current cpu's hgatp）
        sbi::rfence::remote_hfence_vvma(sbi_mask, start_addr, size).map_or_else(
            |err| SbiRet {
                error: (-(err as isize)) as usize,
                value: 0,
            },
            |_x| SbiRet { error: 0, value: 0 },
        )
    }

    fn remote_sfence_vma_asid(
        &self,
        hart_mask: rustsbi::HartMask,
        start_addr: usize,
        size: usize,
        asid: usize,
    ) -> rustsbi::spec::binary::SbiRet {
        let sbi_mask = vcpu_hart_mask_to_pcpu_mask(hart_mask);
        sbi::rfence::remote_hfence_vvma_asid(sbi_mask, start_addr, size, asid).map_or_else(
            |err| SbiRet {
                error: (-(err as isize)) as usize,
                value: 0,
            },
            |_x| SbiRet { error: 0, value: 0 },
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
        info!("hart_start: {}, {:08x}, {}", hartid, start_addr, opaque);

        let vcpu_id = hartid;
        let vm = active_vm().unwrap();
        let physical_linear_id = vm.vcpuid_to_pcpuid(vcpu_id);

        if vcpu_id >= vm.cpu_num() || physical_linear_id.is_err() {
            warn!("hart_start: target vcpu {} not exist", vcpu_id);
            return SbiRet::invalid_param();
        }

        let m = IpiPowerMessage {
            src: vm.id(),
            vcpuid: 0,
            event: PowerEvent::PsciIpiCpuOn,
            entry: start_addr,
            context: opaque,
        };

        // Receiver and handler are in psci_ipi_handler function of `interrupt.rs`
        if !ipi_send_msg(physical_linear_id.unwrap(), IpiType::IpiTPower, IpiInnerMsg::Power(m)) {
            warn!("psci_guest_cpu_on: fail to send msg");
            return SbiRet::failed();
        }

        SbiRet::success(0)
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
    #[allow(unused_variables)]
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

    #[allow(unused_variables)]
    fn counter_get_info(&self, counter_idx: usize) -> rustsbi::spec::binary::SbiRet {
        todo!()
    }

    #[allow(unused_variables)]
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

    #[allow(unused_variables)]
    fn counter_start(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        start_flags: usize,
        initial_value: u64,
    ) -> rustsbi::spec::binary::SbiRet {
        todo!()
    }

    #[allow(unused_variables)]
    fn counter_stop(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        stop_flags: usize,
    ) -> rustsbi::spec::binary::SbiRet {
        todo!()
    }

    #[allow(unused_variables)]
    fn counter_fw_read(&self, counter_idx: usize) -> rustsbi::spec::binary::SbiRet {
        todo!()
    }
}

#[derive(Default)]
struct VLegacyStdio {}

const GETC_EMPTY: u8 = 255;

impl LegacyStdio for VLegacyStdio {
    fn getchar(&self) -> u8 {
        match sbi::legacy::console_getchar() {
            Some(c) => c,
            None => GETC_EMPTY,
        }
    }

    fn putchar(&self, ch: u8) {
        sbi::legacy::console_putchar(ch);
    }
}
static TIMER: VTimer = VTimer {};
static IPI: VIpi = VIpi {};
static RFNC: VRfnc = VRfnc {};
static HSM: VHsm = VHsm {};
static SRST: VSrst = VSrst {};
static PMU: VPmu = VPmu {};
static STDIO: VLegacyStdio = VLegacyStdio {};

// The mutual exclusion of SBICALL is maintained by the **underlying function**, and the Hypervisor does not hold the lock
pub fn init_ecall_handler() {
    init_timer(&TIMER);
    init_ipi(&IPI);
    init_remote_fence(&RFNC);
    init_hsm(&HSM);
    init_reset(&SRST);
    init_pmu(&PMU);
    init_legacy_stdio(&STDIO);
}

/// ecall dispatch function
#[inline(always)]
pub fn hypervisor_handle_ecall(extension: usize, function: usize, param: [usize; 6]) -> SbiRet {
    // patch
    if extension == EID_BASE {
        match function {
            GET_MARCHID => {
                let marchid = 0;
                SbiRet::success(marchid)
            }
            GET_MIMPID => SbiRet::success(KVM),
            GET_MVENDORID => SbiRet::success(0),
            _ => rustsbi::ecall(extension, function, param),
        }
    } else {
        rustsbi::ecall(extension, function, param)
    }
}
