use core::arch::asm;

use crate::config::VmEmulatedDeviceConfig;
use crate::kernel::{active_vm, current_cpu, Vcpu};
use crate::device::{EmuContext, EmuDev};
use super::{GLOBAL_PLIC, IRQ_GUEST_TIMER, IRQ_IPI};
use super::plic::PLICMode;
use alloc::sync::Arc;
use riscv::register::sie;
use spin::Mutex;
use super::plic::{
    PLIC_PRIO_BEGIN, PLIC_PRIO_END, PLIC_PENDING_BEGIN, PLIC_PENDING_END, PLIC_ENABLE_BEGIN, PLIC_ENABLE_END,
    PLIC_THRESHOLD_CLAIM_BEGIN, PLIC_THRESHOLD_CLAIM_END, MAX_HARTS, PLIC_MAX_IRQ,
};
use crate::arch::riscv64::plic::PLICTrait;

pub struct VPlic {
    // Note：Here you need to take full advantage of the internal variability,
    // so that all functions used by VPlic do not have mut permissions, so you need to add Mutex to inner
    inner: Arc<Mutex<VPlicInner>>,
}

/// Note: Define the structure of the vPLIC, which includes some information about the VM in addition to the PLIC
pub struct VPlicInner {
    emulated_base_addr: usize,
    priority: [u32; PLIC_MAX_IRQ + 1],
    pending_cnt: [usize; PLIC_MAX_IRQ + 1],
    m_enables: [[bool; PLIC_MAX_IRQ + 1]; MAX_HARTS],
    s_enables: [[bool; PLIC_MAX_IRQ + 1]; MAX_HARTS], // no hart 0
    m_thresholds: [u32; MAX_HARTS],
    s_thresholds: [u32; MAX_HARTS], // no hart 0
    m_claim: [u32; MAX_HARTS],
    s_claim: [u32; MAX_HARTS], // no hart 0
}

impl VPlic {
    pub fn new(emulated_base_addr: usize) -> VPlic {
        let inner = VPlicInner {
            emulated_base_addr,
            priority: [0; PLIC_MAX_IRQ + 1],
            pending_cnt: [0; PLIC_MAX_IRQ + 1],
            m_enables: [[false; PLIC_MAX_IRQ + 1]; MAX_HARTS],
            s_enables: [[false; PLIC_MAX_IRQ + 1]; MAX_HARTS],
            m_thresholds: [0; MAX_HARTS],
            s_thresholds: [0; MAX_HARTS],
            m_claim: [0; MAX_HARTS],
            s_claim: [0; MAX_HARTS],
        };
        VPlic {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    pub fn get_emulated_base_addr(&self) -> usize {
        let inner = self.inner.lock();
        inner.emulated_base_addr
    }

    // Function: Gets the currently pending interrupts
    // If there are no pending interrupts, return 0
    fn get_pending_irq(&self, mode: PLICMode, hart: usize) -> usize {
        // 1. get prio threshold
        let thres = self.get_threshold(mode, hart);

        // 2. get pending list
        for i in 1..=PLIC_MAX_IRQ {
            if self.get_pending(i) && self.get_enable(i, mode, hart) && self.get_priority(i) >= thres {
                return i;
            }
        }

        0
    }
}

impl PLICTrait for VPlic {
    #[inline(always)]
    fn get_priority(&self, irq: usize) -> usize {
        if !(irq <= PLIC_MAX_IRQ && irq > 0) {
            return 0;
        }

        let inner = self.inner.lock();
        inner.priority[irq] as usize
    }

    #[inline(always)]
    fn set_priority(&self, irq: usize, priority: usize) {
        if !(irq <= PLIC_MAX_IRQ && irq > 0) {
            return;
        }

        // Regardless of the current vplic set priority, the PLIC set priority is 1
        let vm = active_vm().unwrap();
        if vm.has_interrupt(irq) {
            GLOBAL_PLIC.lock().set_priority(irq, 1);
        }

        let mut inner = self.inner.lock();
        inner.priority[irq] = priority as u32;
    }

    #[inline(always)]
    fn get_pending(&self, irq: usize) -> bool {
        if !(irq <= PLIC_MAX_IRQ && irq > 0) {
            return false;
        }

        let inner = self.inner.lock();
        inner.pending_cnt[irq] > 0
    }

    #[inline(always)]
    fn get_enable(&self, irq: usize, mode: PLICMode, hart: usize) -> bool {
        if !(irq <= PLIC_MAX_IRQ && irq > 0) || hart >= MAX_HARTS || (mode == PLICMode::Supervisor && hart == 0) {
            return false;
        }

        let inner = self.inner.lock();
        match mode {
            PLICMode::Machine => inner.m_enables[hart][irq],
            PLICMode::Supervisor => inner.s_enables[hart][irq],
        }
    }

    #[inline(always)]
    fn set_enable(&self, irq: usize, mode: PLICMode, hart: usize) {
        if !(irq <= PLIC_MAX_IRQ && irq > 0) || hart >= MAX_HARTS || (mode == PLICMode::Supervisor && hart == 0) {
            return;
        }

        let mut inner = self.inner.lock();
        match mode {
            PLICMode::Machine => inner.m_enables[hart][irq] = true,
            PLICMode::Supervisor => inner.s_enables[hart][irq] = true,
        }

        // If this interrupt irq is a real pass-through interrupt number,
        // then convey the enable operation to the real plic
        let vm = active_vm().unwrap();
        if let Some(pcpu) = vm.vcpu(hart) {
            let pcpu = pcpu.phys_id();
            if vm.has_interrupt(irq) {
                GLOBAL_PLIC.lock().set_enable(irq, mode, pcpu);
            }
        }
    }

    #[inline(always)]
    fn clear_enable(&self, irq: usize, mode: PLICMode, hart: usize) {
        if !(irq <= PLIC_MAX_IRQ && irq > 0) || hart >= MAX_HARTS || (mode == PLICMode::Supervisor && hart == 0) {
            return;
        }

        let mut inner = self.inner.lock();
        match mode {
            PLICMode::Machine => inner.m_enables[hart][irq] = false,
            PLICMode::Supervisor => inner.s_enables[hart][irq] = false,
        }

        let vm = active_vm().unwrap();
        if let Some(pcpu) = vm.vcpu(hart) {
            let pcpu = pcpu.phys_id();
            if vm.has_interrupt(irq) {
                GLOBAL_PLIC.lock().clear_enable(irq, mode, pcpu);
            }
        }
    }

    #[inline(always)]
    fn get_threshold(&self, mode: PLICMode, hart: usize) -> usize {
        if hart >= MAX_HARTS || (mode == PLICMode::Supervisor && hart == 0) {
            return 0;
        }
        let inner = self.inner.lock();
        match mode {
            PLICMode::Machine => inner.m_thresholds[hart] as usize,
            PLICMode::Supervisor => inner.s_thresholds[hart] as usize,
        }
    }

    #[inline(always)]
    fn set_threshold(&self, mode: PLICMode, hart: usize, threshold: usize) {
        if hart >= MAX_HARTS || (mode == PLICMode::Supervisor && hart == 0) {
            return;
        }

        let mut inner = self.inner.lock();
        match mode {
            PLICMode::Machine => inner.m_thresholds[hart] = threshold as u32,
            PLICMode::Supervisor => inner.s_thresholds[hart] = threshold as u32,
        }
    }

    fn get_claim(&self, mode: PLICMode, hart: usize) -> usize {
        let irq = self.get_pending_irq(mode, hart);
        if irq != 0 {
            // disable pending
            let mut inner = self.inner.lock();
            inner.pending_cnt[irq] -= 1;
        }

        // SAFETY: Clear the hvip external interrupt
        unsafe {
            asm!("csrc hvip, {}", in(reg) (1 << 10));
        }

        irq
    }

    #[allow(unused_variables)]
    fn set_complete(&self, mode: PLICMode, hart: usize, irq: usize) {
        // no action
    }
}

/// Some features of the impl VPlicInner include
/// (Since the VPlicInner is an analog interrupt controller, So we can add a read/write interface to this controller)
/// * Read the value of a register (VM interface)
/// * Write the value of a register (VM interface)
/// * Inject an interrupt: The behavior is to set in Pending, then check Priority and Enable, Threshold, and then trigger Claim and the corresponding interrupt
/// inner function
/// use the related functions of the PLIC structure to perform read and write operations on the related attributes
impl VPlic {
    fn get_pending_reg(&self, index: usize) -> u32 {
        let mut res: u32 = 0;
        let inner = self.inner.lock();
        for i in 0..32 {
            if inner.pending_cnt[index * 32 + i] > 0 {
                res |= (1 << i) as u32;
            }
        }
        res
    }

    fn get_enable_reg(&self, index: usize, mode: PLICMode, hart: usize) -> u32 {
        let mut res: u32 = 0;
        for i in 0..32 {
            if self.get_enable(index * 32 + i, mode, hart) {
                res |= (1 << i) as u32;
            }
        }
        res
    }

    fn edit_enable_reg(&self, index: usize, mode: PLICMode, hart: usize, val: u32) {
        for i in 0..32 {
            if (val & (1 << i)) != 0 {
                self.set_enable(index * 32 + i, mode, hart);
            } else {
                self.clear_enable(index * 32 + i, mode, hart);
            }
        }
    }

    // VM Called
    // Handle register boundaries, beyond the boundary always return 0
    // 4B aligned read, read size u32
    pub fn vm_read_register(&self, addr: usize) -> u32 {
        let offset = addr - self.get_emulated_base_addr();
        // align to 4B
        let offset = offset & !0x3;

        if (PLIC_PRIO_BEGIN..=PLIC_PRIO_END).contains(&offset) {
            self.get_priority((offset - PLIC_PRIO_BEGIN) / 0x4) as u32
        } else if (PLIC_PENDING_BEGIN..=PLIC_PENDING_END).contains(&offset) {
            self.get_pending_reg((offset - PLIC_PENDING_BEGIN) / 0x4)
        } else if (PLIC_ENABLE_BEGIN..=PLIC_ENABLE_END).contains(&offset) {
            let index = (offset & (0x80 - 1)) / 0x4;

            #[cfg(feature = "board_u740")]
            {
                let mode = if ((offset & 0x80) != 0) && ((offset & !0x7F) == PLIC_ENABLE_BEGIN) {
                    PLICMode::Machine
                } else {
                    PLICMode::Supervisor
                };
                let hart = (offset - 0x1f80) / 0x100;
                self.get_enable_reg(index, mode, hart)
            }
            #[cfg(not(feature = "board_u740"))]
            {
                let mode = if (offset & 0x80) != 0 {
                    PLICMode::Machine
                } else {
                    PLICMode::Supervisor
                };
                let hart = (offset - 0x1f80) / 0x100 - 1;
                self.get_enable_reg(index, mode, hart)
            }
        } else if (PLIC_THRESHOLD_CLAIM_BEGIN..=PLIC_THRESHOLD_CLAIM_END).contains(&offset) {
            let reg = offset & 0xfff;
            let mode: PLICMode;
            let hart: usize;

            #[cfg(feature = "board_u740")]
            {
                mode = if ((offset & 0x2000) != 0) && ((offset & !0xfff) != PLIC_THRESHOLD_CLAIM_BEGIN) {
                    PLICMode::Supervisor
                } else {
                    PLICMode::Machine
                };
                hart = (offset - (PLIC_THRESHOLD_CLAIM_BEGIN - 0x1000)) / 0x2000;
            }
            #[cfg(not(feature = "board_u740"))]
            {
                mode = if (offset & 0x2000) != 0 {
                    PLICMode::Supervisor
                } else {
                    PLICMode::Machine
                };
                hart = (offset - PLIC_THRESHOLD_CLAIM_BEGIN) / 0x2000;
            }

            if reg == 0x0 {
                self.get_threshold(mode, hart) as u32
            } else if reg == 0x4 {
                self.get_claim(mode, hart) as u32
            } else {
                0
            }
        } else {
            panic!("invalid plic register access: 0x{:x} offset: 0x{:x}", addr, offset);
        }
    }

    pub fn vm_write_register(&self, addr: usize, val: u32) {
        let offset = addr - self.get_emulated_base_addr();
        // align to 4B
        let offset = offset & !0x3;

        // Pending regs is Read Only
        if (PLIC_PRIO_BEGIN..=PLIC_PRIO_END).contains(&offset) {
            self.set_priority((offset - PLIC_PRIO_BEGIN) / 0x4, val as usize);
        } else if (PLIC_ENABLE_BEGIN..=PLIC_ENABLE_END).contains(&offset) {
            let index = (offset & (0x80 - 1)) / 0x4;

            #[cfg(feature = "board_u740")]
            {
                let mode = if ((offset & 0x80) != 0) && ((offset & !0x7F) == PLIC_ENABLE_BEGIN) {
                    PLICMode::Machine
                } else {
                    PLICMode::Supervisor
                };
                let hart = (offset - 0x1f80) / 0x100;
                self.edit_enable_reg(index, mode, hart, val);
            }
            #[cfg(not(feature = "board_u740"))]
            {
                let mode = if (offset & 0x80) != 0 {
                    PLICMode::Machine
                } else {
                    PLICMode::Supervisor
                };
                let hart = (offset - 0x1f80) / 0x100 - 1;
                self.edit_enable_reg(index, mode, hart, val);
            }
        } else if (PLIC_THRESHOLD_CLAIM_BEGIN..=PLIC_THRESHOLD_CLAIM_END).contains(&offset) {
            let reg = offset & 0xfff;
            let mode: PLICMode;
            let hart: usize;

            #[cfg(feature = "board_u740")]
            {
                mode = if ((offset & 0x2000) != 0) && ((offset & !0xfff) != PLIC_THRESHOLD_CLAIM_BEGIN) {
                    PLICMode::Supervisor
                } else {
                    PLICMode::Machine
                };
                hart = (offset - (PLIC_THRESHOLD_CLAIM_BEGIN - 0x1000)) / 0x2000;
            }
            #[cfg(not(feature = "board_u740"))]
            {
                mode = if (offset & 0x2000) != 0 {
                    PLICMode::Supervisor
                } else {
                    PLICMode::Machine
                };
                hart = (offset - PLIC_THRESHOLD_CLAIM_BEGIN) / 0x2000;
            }

            if reg == 0x0 {
                self.set_threshold(mode, hart, val as usize);
            } else if reg == 0x4 {
                self.set_complete(mode, hart, val as usize);
            }
        } else {
            panic!("invalid plic register access: 0x{:x}, offset: 0x{:x}", addr, offset);
        }
        // Does not conform to the specification of the visit, do nothing
    }

    pub fn inject_intr(&self, irq: usize) {
        if irq == IRQ_GUEST_TIMER {
            riscv::register::hvip::trigger_timing_interrupt();
            // SAFETY: Disable timer interrupt
            unsafe { sie::clear_stimer() };
        } else if irq == IRQ_IPI {
            riscv::register::hvip::trigger_software_interrupt();
        } else {
            self.inject_external_intr(irq)
        }
    }

    // Check and inject a PLIC interrupt to VM
    fn inject_external_intr(&self, irq: usize) {
        let mut inner = self.inner.lock();
        inner.pending_cnt[irq] += 1;
        drop(inner);

        // In general, the Hypervisor should enable Machine-level interrupts to obtain Machine-level pending
        let mode = PLICMode::Machine;
        let vcpu = current_cpu().active_vcpu.as_ref().unwrap().id();
        let fetched_irq = self.get_pending_irq(mode, vcpu as usize);
        if fetched_irq != 0 {
            // Note: mode and hart are dynamicly fetched
            // inject external intr
            riscv::register::hvip::trigger_external_interrupt();
        }
    }
}

#[allow(unused_variables)]
pub fn emu_intc_init(emu_cfg: &VmEmulatedDeviceConfig, vcpu_list: &[Vcpu]) -> Result<Arc<dyn EmuDev>, ()> {
    // Create and initialize the virtual interrupt controller:
    // Create a new VPLIC object and call vm.set_emu_devs to add the emulated device to the Vm
    let vplic = VPlic::new(emu_cfg.base_ipa);
    Ok(Arc::new(vplic))
}

const VPLIC_LENGTH: usize = 0x600000;

impl EmuDev for VPlic {
    fn emu_type(&self) -> crate::device::EmuDeviceType {
        crate::device::EmuDeviceType::EmuDeviceTPlic
    }

    fn address_range(&self) -> core::ops::Range<usize> {
        let base_addr = self.get_emulated_base_addr();
        base_addr..base_addr + VPLIC_LENGTH
    }

    // Emulated plic's entry
    fn handler(&self, emu_ctx: &EmuContext) -> bool {
        let vm = active_vm().unwrap();
        let vplic = vm.vplic();
        let reg_idx = emu_ctx.reg;

        if emu_ctx.width != 4 {
            panic!("emu_plic_mmio_handler: invalid width {}", emu_ctx.width);
        }

        // TODO: Research whether there is a requirement to implement other bit widths (other than 32bit)
        if emu_ctx.write {
            vplic.vm_write_register(emu_ctx.address, current_cpu().get_gpr(reg_idx) as u32);
        } else {
            let val = vplic.vm_read_register(emu_ctx.address);
            current_cpu().set_gpr(reg_idx, val as usize);
        }

        true
    }
}
