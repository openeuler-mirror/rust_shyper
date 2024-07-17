use sbi::HartMask;
use crate::arch::psci_vcpu_on;
use crate::arch::InterruptController;
use crate::arch::PLIC;
use crate::arch::PLICTrait;
use crate::kernel::vm;
use crate::kernel::IpiInnerMsg;
use crate::kernel::IpiMessage;
use crate::kernel::current_cpu;
use crate::kernel::PowerEvent;
use crate::kernel::VcpuState;
use spin::Mutex;

// reference: fu740-c000-manual
const INTERRUPT_NUM_MAX: usize = 70;
const PRIORITY_NUM_MAX: usize = 7;

pub const CLINT_IRQ_BASE: usize = 60;
pub const IRQ_IPI: usize = 60; // for clint, not plic
pub const IRQ_HYPERVISOR_TIMER: usize = 61; // for clint, not plic
pub const IRQ_GUEST_TIMER: usize = 62; // not valid

pub const PLIC_BASE_ADDR: u64 = 0x0C00_0000;

const UART0_IRQ: usize = 10;
const VIRTIO_IRQ: usize = 1;

pub struct IntCtrl;

pub static GLOBAL_PLIC: Mutex<PLIC> = Mutex::new(PLIC::new(PLIC_BASE_ADDR as usize));

// True PLIC
impl InterruptController for IntCtrl {
    const NUM_MAX: usize = INTERRUPT_NUM_MAX;

    const PRI_NUN_MAX: usize = PRIORITY_NUM_MAX;

    const IRQ_IPI: usize = IRQ_IPI;

    const IRQ_HYPERVISOR_TIMER: usize = IRQ_HYPERVISOR_TIMER;

    const IRQ_GUEST_TIMER: usize = IRQ_GUEST_TIMER;

    fn init() {
        #[cfg(not(feature = "secondary_start"))]
        crate::utils::barrier();

        // Set interrupt threshold for current cpu
        let locked = GLOBAL_PLIC.lock();
        locked.set_threshold(crate::arch::PLICMode::Machine, current_cpu().id, 0);

        // SAFETY: Enable external interrupt
        unsafe { riscv::register::sie::set_sext() };
    }

    fn enable(int_id: usize, en: bool) {
        if en {
            // Note: for timer intr, ipi intr and other reserved intr
            // give it a specified fake intr id
            if int_id >= CLINT_IRQ_BASE {
                if int_id == IRQ_HYPERVISOR_TIMER {
                    // SAFETY: Enable timer interrupt
                    unsafe { riscv::register::sie::set_stimer() }
                } else if int_id == IRQ_IPI {
                    // SAFETY: Enable software interrupt(IPI)
                    unsafe { riscv::register::sie::set_ssoft() }
                } else {
                    panic!("enable intr {} not supported", int_id);
                }
            } else {
                GLOBAL_PLIC
                    .lock()
                    .set_enable(int_id, crate::arch::PLICMode::Machine, current_cpu().id);
            }
        } else if int_id >= CLINT_IRQ_BASE {
            if int_id == IRQ_HYPERVISOR_TIMER {
                // SAFETY: Disable timer interrupt
                unsafe { riscv::register::sie::clear_stimer() }
            } else if int_id == IRQ_IPI {
                // SAFETY: Disable software interrupt(IPI)
                unsafe { riscv::register::sie::clear_ssoft() }
            } else {
                panic!("enable intr {} not supported", int_id);
            }
        } else {
            GLOBAL_PLIC
                .lock()
                .clear_enable(int_id, crate::arch::PLICMode::Machine, current_cpu().id);
        }
    }

    fn fetch() -> Option<usize> {
        // Invalid function
        todo!()
    }

    fn clear() {
        // loop until no pending intr
        loop {
            let irq = GLOBAL_PLIC.lock().get_claim(super::PLICMode::Machine, current_cpu().id);
            if irq == 0 {
                // TODO: not clearing sip，maybe no need?
                break;
            } else {
                GLOBAL_PLIC
                    .lock()
                    .set_complete(super::PLICMode::Machine, current_cpu().id, irq);
            }
        }
    }

    fn finish(int_id: usize) {
        GLOBAL_PLIC
            .lock()
            .set_complete(super::PLICMode::Machine, current_cpu().id, int_id);
    }

    #[allow(unused_variables)]
    fn ipi_send(cpu_id: usize, ipi_id: usize) {
        // TODO: can't specify ipi_id
        let _ = sbi::ipi::send_ipi(HartMask::from(cpu_id));
    }

    fn vm_inject(vm: &crate::kernel::Vm, vcpu: &crate::kernel::Vcpu, int_id: usize) {
        // Inject interrupt through virtual plic
        let vplic = vm.vplic();
        if let Some(cur_vcpu) = current_cpu().active_vcpu.clone() {
            if cur_vcpu.vm_id() == vcpu.vm_id() {
                // Note: trigger a timer intr, external intr, or soft intr
                // if external intr, inject to vplic
                vplic.inject_intr(int_id);
                return;
            }
        }

        vcpu.push_int(int_id);
    }

    #[allow(unused_variables)]
    fn vm_register(vm: &crate::kernel::Vm, int_id: usize) {
        // register interrupts with the virtual plic, that is, bind the real interrupts to the virtual plic.
        // The PLIC operation should be written here, but the int id is added to the vm bitmap by interrupt vm register.
        // Therefore, PLIC does not need to do any operation at this time
    }

    #[allow(unused_variables)]
    fn clear_current_irq(for_hypervisor: bool) {
        // Don't do anything for a while
        // TODO: Maybe you need to take an interrupt and deal with it
        current_cpu().current_irq = 0;
    }
}

const CAUSE_INTR_SOFT: usize = 1;
const CAUSE_INTR_TIMER: usize = 5;
const CAUSE_INTR_EXTERNAL: usize = 9;

const SIP_SSIP: usize = 1 << 1;

pub fn deactivate_soft_intr() {
    // sip register mapping error for riscv crate is sie (master changed, but not released)

    // SAFETY:
    // Clearing the soft interrupt bit of reg sip means deactivate software interrupt,
    //  which is called just in the IPI handler, so it is safe.
    unsafe {
        use core::arch::asm;
        // Remove the soft interrupt bit of reg sip
        // This operation must be before ipi handler，since some handler may return to VM
        asm!("csrc sip, {}", in(reg) SIP_SSIP);
    }
}

pub fn riscv_get_pending_irqs(int_cause: usize) -> Option<usize> {
    // Get interrupt information from CLINT first, if ext hangs in sip, then access PLIC
    // If you read the claim register directly, you may read an unknown value
    match int_cause {
        CAUSE_INTR_SOFT => Some(IRQ_IPI),
        CAUSE_INTR_TIMER => Some(IRQ_HYPERVISOR_TIMER),
        CAUSE_INTR_EXTERNAL => {
            // Get intr from PLIC
            let irq = GLOBAL_PLIC.lock().get_claim(super::PLICMode::Machine, current_cpu().id);
            if irq == 0 {
                None
            } else {
                GLOBAL_PLIC
                    .lock()
                    .set_complete(super::PLICMode::Machine, current_cpu().id, irq);
                Some(irq)
            }
        }
        _ => {
            panic!("unhandled interrupt cause: {}", int_cause);
        }
    }
}

pub fn psci_ipi_handler(msg: IpiMessage) {
    info!("psci_ipi_handler: cpu{} receive psci ipi", current_cpu().id);
    match msg.ipi_message {
        IpiInnerMsg::Power(power_msg) => {
            // True power event

            // AssignAndOn events，indicates that the vcpu is allocated to a cpu and needs to run
            if let PowerEvent::PsciIpiVcpuAssignAndCpuOn = power_msg.event {
                trace!("receive PsciIpiVcpuAssignAndCpuOn msg");
                let vm = vm(power_msg.src).unwrap();
                let vcpu = vm.vcpuid_to_vcpu(power_msg.vcpuid).unwrap();
                current_cpu().vcpu_array.append_vcpu(vcpu);
            }

            let target_vcpu = match current_cpu().vcpu_array.pop_vcpu_through_vmid(power_msg.src) {
                Some(vcpu) => vcpu,
                None => {
                    warn!(
                        "Core {} failed to find target vcpu, source vmid {}",
                        current_cpu().id,
                        power_msg.src
                    );
                    return;
                }
            };

            match power_msg.event {
                PowerEvent::PsciIpiVcpuAssignAndCpuOn => {}
                PowerEvent::PsciIpiCpuOn => {
                    if target_vcpu.state() as usize != VcpuState::Invalid as usize {
                        warn!(
                            "psci_ipi_handler: target VCPU {} in VM {} is already running",
                            target_vcpu.id(),
                            target_vcpu.vm().unwrap().id()
                        );
                        return;
                    }
                    info!(
                        "Core {} (vm {}, vcpu {}) is woke up",
                        current_cpu().id,
                        target_vcpu.vm().unwrap().id(),
                        target_vcpu.id()
                    );
                    psci_vcpu_on(target_vcpu, power_msg.entry, power_msg.context);
                }
                PowerEvent::PsciIpiCpuOff => {
                    unimplemented!("PsciIpiCpuOff");
                }
                _ => {
                    panic!(
                        "unimplemented power event: {} in psci_ipi_handler",
                        power_msg.event as usize
                    );
                }
            }
        }
        _ => {
            panic!(
                "psci_ipi_handler: cpu{} receive illegal psci ipi type {}",
                current_cpu().id,
                msg.ipi_type as usize
            );
        }
    }
}

#[allow(unused_variables)]
pub fn vgic_ipi_handler(msg: IpiMessage) {
    todo!()
}
