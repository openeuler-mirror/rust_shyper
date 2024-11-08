use alloc::vec::Vec;
/// This file provides the interface that the VM accesses to the upper-layer SBI.
/// For some SBI operations, Hypervisor emulation is required instead of
/// directly invoking the M-state SBI software.
use rustsbi::{RustSBI, Console, Hsm, Ipi, Pmu, Reset, Timer, EnvInfo, HartMask};
use sbi_spec::binary::{Physical, SbiRet};
use sbi_spec::base::{impl_id::KVM, EID_BASE, PROBE_EXTENSION};
use sbi_spec::hsm::{HART_START, HART_STOP};

use spin::Mutex;
// use timer::timer_arch_get_counter;

use crate::{
    arch::power_arch_cpu_on,
    kernel::{
        active_vm, current_cpu, ipi_send_msg, CpuState, IpiInnerMsg, IpiIntInjectMsg, IpiPowerMessage,
        IpiType, PowerEvent, StartReason, VcpuState, Vm, CPU_IF_LIST,
    },
};
use crate::kernel::IpiType::IpiTIntInject;
use crate::kernel::IpiInnerMsg::IntInjectMsg;

use super::IRQ_IPI;
use crate::kernel::Scheduler;

#[derive(Default)]
struct  VConsole {}

impl Console for VConsole {
    fn write(&self, bytes: Physical<&[u8]>) -> SbiRet {
        sbi_rt::console_write(bytes)
    }

    fn read(&self, bytes: Physical<&mut [u8]>) -> SbiRet {
        sbi_rt::console_read(bytes)
    }

    fn write_byte(&self, byte: u8) -> SbiRet {
        sbi_rt::console_write_byte(byte)
    }
}

#[derive(Default)]
struct VTimer {}

impl Timer for VTimer {
    fn set_timer(&self, stime_value: u64) {
        // info!("set_timer: {}, current_time: {}", stime_value, timer_arch_get_counter());
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
    fn send_ipi(&self, hart_mask: HartMask) -> SbiRet {
        // info!("sbi_send_ipi: {:?}", hart_mask);
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
fn get_pcpu_ids(vm: &Vm, hart_mask: HartMask) -> (Vec<usize>, bool) {
    let mut ret: Vec<usize> = Vec::new();
    // let nvcpu = vm.inner().lock().vcpu_list.len();
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

// Converts the vCPU-based hart mask to the hart mask of the pcpu corresponding to the vcpu of the current vm
// If an error occurs (for example, the vcpu core number exceeds the number of Vcpus on the vm), return an empty HartMask
#[inline(always)]
fn vcpu_hart_mask_to_pcpu_mask(hart_mask: HartMask) -> HartMask {
    let (pcpus, valid) = get_pcpu_ids(&active_vm().unwrap(), hart_mask.clone());
    if valid {
        let mut mask: usize = 0;
        for pcpu in pcpus {
            mask |= 1 << pcpu;
        }
        HartMask::from_mask_base(mask, 0)
    } else {
        // no core selected
        warn!(
            "vcpu_hart_mask_to_pcpu_mask: no core selected since invalid hart_mask: {:?}!",
            hart_mask
        );
        HartMask::from_mask_base(0,0)
    }
}

#[derive(Default)]
struct VRfnc {}


impl rustsbi::Fence for VRfnc {
    fn remote_fence_i(&self, hart_mask: HartMask) -> SbiRet {
        sbi_rt::remote_fence_i(vcpu_hart_mask_to_pcpu_mask(hart_mask))
    }

    fn remote_sfence_vma(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
    ) -> SbiRet {
        let sbi_mask = vcpu_hart_mask_to_pcpu_mask(hart_mask);
        // On harts specified by hart_mask，execute hfence.vvma(vmid, start_addr, size) （vmid is from current cpu's hgatp）
        sbi_rt::remote_hfence_vvma(sbi_mask, start_addr, size)
    }

    fn remote_sfence_vma_asid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        asid: usize,
    ) -> SbiRet {
        let sbi_mask = vcpu_hart_mask_to_pcpu_mask(hart_mask);
        sbi_rt::remote_hfence_vvma_asid(sbi_mask, start_addr, size, asid)
    }
}

#[derive(Default)]
struct VHsm {}

impl Hsm for VHsm {
    // TODO: needs to handle the cpu hart stop restart. This is not a real stop,
    // but into the sleep state, need to call PsciIpiCpuOn
    // similar to psci guest cpu on this function (in fact, it is also copied from the aarch64 function)
    fn hart_start(&self, hartid: usize, start_addr: usize, opaque: usize) -> SbiRet {
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

    fn hart_stop(&self) -> SbiRet {
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

    fn hart_get_status(&self, hartid: usize) -> SbiRet {
        let vm = active_vm().unwrap();
        let vcpu_ = vm.vcpu(hartid);
        if let Some(vcpu) = vcpu_ {
            let state = vcpu.state();
            match state {
                // Both the running state and the ready state are HART_STATE_STARTED
                VcpuState::Running => SbiRet::success(HART_START),
                VcpuState::Ready => SbiRet::success(HART_START),
                // The inactive state and the sleep state (which is no longer functioning) are both HART_STATE_STOPPED
                VcpuState::Invalid => SbiRet::success(HART_STOP),
                VcpuState::Sleep => SbiRet::success(HART_STOP),
            }
        } else {
            SbiRet::invalid_param()
        }
    }
}

#[derive(Default)]
struct VSrst {}

#[allow(unused_variables)]
impl Reset for VSrst {
    fn system_reset(&self, reset_type: u32, reset_reason: u32) -> SbiRet {
        todo!()
    }
}

#[derive(Default)]
struct VPmu {}

#[allow(unused_variables)]
impl Pmu for VPmu {
    fn num_counters(&self) -> usize {
        todo!()
    }

    fn counter_get_info(&self, counter_idx: usize) -> SbiRet {
        todo!()
    }

    fn counter_config_matching(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        config_flags: usize,
        event_idx: usize,
        event_data: u64,
    ) -> SbiRet {
        todo!()
    }

    fn counter_start(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        start_flags: usize,
        initial_value: u64,
    ) -> SbiRet {
        todo!()
    }

    fn counter_stop(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        stop_flags: usize,
    ) -> SbiRet {
        todo!()
    }

    fn counter_fw_read(&self, counter_idx: usize) -> SbiRet {
        todo!()
    }
}

#[derive(Default)]
struct VInfo {}

impl EnvInfo for VInfo {
    fn mvendorid(&self) -> usize {
        0x0
    }

    fn marchid(&self) -> usize {
        0x0
    }

    fn mimpid(&self) -> usize {
        KVM
    }
}


pub struct VmHart {
    pub env: Mutex<ShyperSBI>,
}

#[derive(RustSBI, Default)]
pub struct ShyperSBI {
    console: VConsole,
    timer: VTimer,
    ipi: VIpi,
    hsm: VHsm,
    fence: VRfnc,
    reset: VSrst,
    info: VInfo,
}


impl VmHart {
    pub fn new() -> Self {
        VmHart {
            env: Mutex::new(ShyperSBI{
                console: VConsole::default(),
                timer: VTimer::default(),
                ipi: VIpi::default(),
                fence: VRfnc::default(),
                hsm: VHsm::default(),
                reset: VSrst::default(),
                info: VInfo::default(),
            }),
        }
    }

    /// ecall dispatch function
    #[inline(always)]
    #[allow(unused_mut)]
    pub fn handle_ecall(&self, extension: usize, function: usize, param: [usize; 6]) -> SbiRet {
        use sbi_spec::legacy::{LEGACY_CONSOLE_GETCHAR, LEGACY_CONSOLE_PUTCHAR};
        match extension {
            EID_BASE => {
                match function {
                    PROBE_EXTENSION => {
                        if matches!(param[0], LEGACY_CONSOLE_GETCHAR | LEGACY_CONSOLE_PUTCHAR) {
                            SbiRet::success(1)
                        } else {
                            self.env.lock().handle_ecall(extension, function, param)
                        }
                    }
                    _ => { self.env.lock().handle_ecall(extension, function, param) }
                }
            }
            LEGACY_CONSOLE_GETCHAR => {
                let mut ch: u8 = 0;
                let byte = Physical::new(1, 0, &ch as *const u8 as usize);
                sbi_rt::console_read(byte);
                let mut sbi_ret = SbiRet::success(0);
                sbi_ret.error = ch as usize;
                sbi_ret
            }
            LEGACY_CONSOLE_PUTCHAR => {
                sbi_rt::console_write_byte(param[0] as u8)
            }
            _ => {
                self.env.lock().handle_ecall(extension,function, param)
            }

        }
    }
}