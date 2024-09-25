use crate::config::VmEmulatedDeviceConfig;
use crate::kernel::{active_vm, current_cpu, Vcpu};
use crate::device::{EmuContext, EmuDev};
use alloc::sync::Arc;
use spin::Mutex;
use crate::arch::SourceModes;
use crate::arch::riscv64::aplic::APLICTrait;

use super::{GLOBAL_APLIC, IRQ_GUEST_TIMER, IRQ_IPI};
use super::aplic::{
    APLIC_DOMAINCFG_BASE, APLIC_DOMAINCFG_TOP, APLIC_SOURCECFG_BASE, APLIC_SOURCECFG_TOP, APLIC_S_MSIADDR_BASE,
    APLIC_S_MSIADDR_TOP, APLIC_SET_PENDING_BASE, APLIC_SET_PENDING_TOP, APLIC_SET_PENDING_NUM_BASE,
    APLIC_SET_PENDING_NUM_TOP, APLIC_CLR_PENDING_BASE, APLIC_CLR_PENDING_TOP, APLIC_CLR_PENDING_NUM_BASE,
    APLIC_CLR_PENDING_NUM_TOP, APLIC_SET_ENABLE_BASE, APLIC_SET_ENABLE_TOP, APLIC_SET_ENABLE_NUM_BASE,
    APLIC_SET_ENABLE_NUM_TOP, APLIC_CLR_ENABLE_BASE, APLIC_CLR_ENABLE_TOP, APLIC_CLR_ENABLE_NUM_BASE,
    APLIC_CLR_ENABLE_NUM_TOP, APLIC_SET_IPNUM_LE_BASE, APLIC_SET_IPNUM_LE_TOP, APLIC_SET_IPNUM_BE_BASE,
    APLIC_SET_IPNUM_BE_TOP, APLIC_GENMSI_BASE, APLIC_GENMSI_TOP, APLIC_TARGET_BASE, APLIC_TARGET_TOP,
};
use riscv::register::sie;

const VPLIC_LENGTH: usize = 0x8000;
pub struct VAPlic {
    // Note：Here you need to take full advantage of the internal variability,
    // so that all functions used by VAPlic do not have mut permissions, so you need to add Mutex to inner
    inner: Arc<Mutex<VAPlicInner>>,
}

/// Note: Define the structure of the vAPLIC, which includes some information about the VM in addition to the APLIC
pub struct VAPlicInner {
    emulated_base: usize,
    emulated_size: usize,
    domaincfg: u32,
    sourcecfg: [u32; 1023],
    mmsiaddrcfg: u32,
    mmsiaddrcfgh: u32,
    smsiaddrcfg: u32,
    smsiaddrcfgh: u32,
    setip: [u32; 32],
    setipnum: u32,
    in_clrip: [u32; 32],
    clripnum: u32,
    setie: [u32; 32],
    setienum: u32,
    clrie: [u32; 32],
    clrienum: u32,
    setipnum_le: u32,
    setipnum_be: u32,
    genmsi: u32,
    target: [u32; 1023],
}

impl VAPlic {
    pub fn new(emulated_base: usize) -> VAPlic {
        let inner = VAPlicInner {
            emulated_base,
            emulated_size: VPLIC_LENGTH,
            domaincfg: 0,
            sourcecfg: [0; 1023],
            mmsiaddrcfg: 0,
            mmsiaddrcfgh: 0,
            smsiaddrcfg: 0,
            smsiaddrcfgh: 0,
            setip: [0; 32],
            setipnum: 0,
            in_clrip: [0; 32],
            clripnum: 0,
            setie: [0; 32],
            setienum: 0,
            clrie: [0; 32],
            clrienum: 0,
            setipnum_le: 0,
            setipnum_be: 0,
            genmsi: 0,
            target: [0; 1023],
        };
        VAPlic {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    pub fn get_emulated_base_addr(&self) -> usize {
        let inner = self.inner.lock();
        inner.emulated_base
    }
}

impl APLICTrait for VAPlic {
    /// # Overview
    /// Set the `domaincfg` register.
    /// ## Arguments
    /// * `bigendian` `true`: the APLIC uses big endian byte order, `false`: the APLIC uses little endian byte order.
    /// * `msimode` `true`: the APLIC will send MSIs for interrupts, `false`: the APLIC will only trigger actual wires.
    /// * `enabled` `true`: this APLIC is enabled and can receive/send interrupts, `false`: the APLIC domain is disabled.
    fn set_domaincfg(&self, bigendian: bool, msimode: bool, enabled: bool) {
        // Rust library assures that converting a bool into u32 will use
        // 1 for true and 0 for false

        // test @ CHonghao
        GLOBAL_APLIC.lock().set_domaincfg(bigendian, msimode, enabled);

        let mut inner = self.inner.lock();
        let enabled = u32::from(enabled);
        let msimode = u32::from(msimode);
        let bigendian = u32::from(bigendian);
        inner.domaincfg = (enabled << 8) | (msimode << 2) | bigendian;
    }

    fn get_domaincfg(&self) -> u32 {
        let inner = self.inner.lock();
        inner.domaincfg
    }

    fn get_msimode(&self) -> bool {
        let inner: spin::MutexGuard<VAPlicInner> = self.inner.lock();
        ((inner.domaincfg >> 2) & 0b11) != 0
    }

    /// # Overview
    /// Setup a source configuration to a particular mode.
    /// This does NOT delegate the source to a child.
    /// ## Arguments
    /// * `irq` the interrupt number to set
    /// * `mode` the source mode--how the interrupt is triggered.
    fn set_sourcecfg(&self, irq: u32, mode: SourceModes) {
        assert!(irq > 0 && irq < 1024);
        let vm = active_vm().unwrap();
        if vm.has_interrupt(irq.try_into().unwrap()) {
            GLOBAL_APLIC.lock().set_sourcecfg(irq, mode)
        }
        let mut inner = self.inner.lock();
        inner.sourcecfg[irq as usize - 1] = mode as u32;
    }

    /// # Overview
    /// Setup a source configuration to delegate an IRQ to a child.
    /// ## Arguments
    /// * `irq` the interrupt number to delegate
    /// * `child` the child to delegate this interrupt to
    fn set_sourcecfg_delegate(&self, irq: u32, child: u32) {
        assert!(irq > 0 && irq < 1024);
        let vm = active_vm().unwrap();
        if vm.has_interrupt(irq.try_into().unwrap()) {
            GLOBAL_APLIC.lock().set_sourcecfg_delegate(irq, child)
        }
        let mut inner = self.inner.lock();
        inner.sourcecfg[irq as usize - 1] = 1 << 10 | child & 0x3ff;
    }

    fn get_sourcecfg(&self, irq: u32) -> u32 {
        assert!(irq > 0 && irq < 1024);
        let inner = self.inner.lock();
        inner.sourcecfg[irq as usize - 1]
    }

    /// # Overview
    /// Set the MSI target physical address. This only accepts the lower
    /// 32-bits of an address.
    /// ## Arguments
    /// * `mode` the MSI mode (machine or supervisor)
    /// * `addr` the physical address for messages. This MUST be page aligned.
    fn set_msiaddr(&self, addr: usize) {
        let mut inner = self.inner.lock();
        // match mode {
        //     APLICMode::Machine => {
        //         GLOBAL_APLIC.lock().set_msiaddr(addr);
        //         inner.mmsiaddrcfg = (addr >> 12) as u32;
        //         inner.mmsiaddrcfgh = 0;
        //     }
        //     APLICMode::Supervisor => {
        //         GLOBAL_APLIC.lock().set_msiaddr(addr);
        //         inner.smsiaddrcfg = (addr >> 12) as u32;
        //         inner.smsiaddrcfgh = 0;
        //     }
        // }
        GLOBAL_APLIC.lock().set_msiaddr(addr);
        inner.smsiaddrcfg = (addr >> 12) as u32;
        inner.smsiaddrcfgh = 0;
    }

    fn get_pending(&self, irqidx: usize) -> u32 {
        assert!(irqidx < 32);
        let inner = self.inner.lock();
        inner.setip[irqidx]
    }

    /// # Overview
    /// Set the irq pending bit to the given state
    /// ## Arguments
    /// * `irq` the interrupt number
    /// * `pending` true: set the bit to 1, false: clear the bit to 0
    fn set_pending(&self, irqidx: usize, value: u32, pending: bool) {
        // (&mut self, irq: u32, pending: bool) {
        // assert!(irq > 0 && irq < 1024);
        // let irqidx = irq as usize / 32;
        // let irqbit = irq as usize % 32;
        assert!(irqidx < 32);
        GLOBAL_APLIC.lock().set_pending(irqidx, value, pending);
        let mut inner = self.inner.lock();
        if pending {
            // self.setipnum = irq;
            inner.setip[irqidx] = value;
        } else {
            // self.clripnum = irq;
            inner.in_clrip[irqidx] = value;
        }
    }

    fn set_pending_num(&self, value: u32) {
        GLOBAL_APLIC.lock().set_pending_num(value);
        let mut inner = self.inner.lock();
        inner.setipnum = value;
    }

    fn get_in_clrip(&self, irqidx: usize) -> u32 {
        assert!(irqidx < 32);
        let inner = self.inner.lock();
        inner.in_clrip[irqidx]
    }

    fn get_enable(&self, irqidx: usize) -> u32 {
        assert!(irqidx < 32);
        let inner = self.inner.lock();
        inner.setie[irqidx]
    }

    fn get_clr_enable(&self, irqidx: usize) -> u32 {
        assert!(irqidx < 32);
        let inner = self.inner.lock();
        inner.clrie[irqidx]
    }

    /// # Overview
    /// Set the irq enabled bit to given state
    /// ## Arguments
    /// * `irq` the interrupt number
    /// * `enabled` true: enable interrupt, false: disable interrupt
    fn set_enable(&self, irqidx: usize, value: u32, enabled: bool) {
        //  (&mut self, irq: u32, enabled: bool) {
        //  assert!(irq > 0 && irq < 1024);
        //  let irqidx = irq as usize / 32;
        //  let irqbit = irq as usize % 32;
        assert!(irqidx < 32);
        GLOBAL_APLIC.lock().set_enable(irqidx, value, enabled);
        let mut inner = self.inner.lock();
        if enabled {
            // self.setienum = irq;
            inner.setie[irqidx] = value;
        } else {
            // self.clrienum = irq;
            inner.clrie[irqidx] = value;
        }
    }

    fn set_enable_num(&self, value: u32) {
        GLOBAL_APLIC.lock().set_enable_num(value);
        let mut inner = self.inner.lock();
        inner.setienum = value;
    }

    fn clr_enable_num(&self, value: u32) {
        GLOBAL_APLIC.lock().clr_enable_num(value);
        let mut inner = self.inner.lock();
        inner.clrienum = value;
    }

    fn setipnum_le(&self, value: u32) {
        GLOBAL_APLIC.lock().setipnum_le(value);
        let mut inner = self.inner.lock();
        inner.setipnum_le = value;
    }

    /// # Overview
    /// Set the target interrupt to a given hart, guest, and identifier
    /// ## Arguments
    /// * `irq` - the interrupt to set
    /// * `hart` - the hart that will receive interrupts from this irq
    /// * `guest` - the guest identifier to send these interrupts
    /// * `eiid` - the identification number of the irq (usually the same as the irq itself)
    fn set_target_msi(&self, irq: u32, hart: u32, guest: u32, eiid: u32) {
        assert!(irq > 0 && irq < 1024);
        let vm = active_vm().unwrap();
        if vm.has_interrupt(irq.try_into().unwrap()) {
            GLOBAL_APLIC.lock().set_target_msi(irq, hart, guest, eiid);
        }
        let mut inner = self.inner.lock();
        inner.target[irq as usize - 1] = (hart << 18) | (guest << 12) | eiid;
    }

    /// # Overview
    /// Set the target interrupt to a given hart and priority
    /// ## Arguments
    /// * `irq` - the interrupt to set
    /// * `hart` - the hart that will receive interrupts from this irq
    /// * `prio` - the priority of this direct interrupt.
    fn set_target_direct(&self, irq: u32, hart: u32, prio: u32) {
        assert!(irq > 0 && irq < 1024);
        let vm = active_vm().unwrap();
        if vm.has_interrupt(irq.try_into().unwrap()) {
            GLOBAL_APLIC.lock().set_target_direct(irq, hart, prio);
        }
        let mut inner = self.inner.lock();
        inner.target[irq as usize - 1] = (hart << 18) | (prio & 0xFF);
    }
}

impl VAPlic {
    pub fn vm_read_register(&self, addr: usize) -> u32 {
        // debug!("vm_read_register:0x{:#x}", addr);
        let offset = addr - self.get_emulated_base_addr();
        // align to 4B
        let offset = offset & !0x3;
        if (APLIC_DOMAINCFG_BASE..=APLIC_DOMAINCFG_TOP).contains(&offset) {
            // domaincfg
            let value = self.get_domaincfg();
            debug!("APLIC read domaincfg addr@{:#x} value {}", addr, value);
            value
        /* } else if (APLIC_SOURCECFG_BASE..=APLIC_SOURCECFG_TOP).contains(&offset) {
            // sourcecfg
            panic!("sourcecfg Unexpected addr {:#x}", addr);
        } else if (APLIC_S_MSIADDR_BASE..=APLIC_S_MSIADDR_TOP).contains(&offset) {
            // smsiaddrcfg
            panic!("smsiaddrcfg Unexpected addr {:#x}", addr);
        } else if (APLIC_SET_PENDING_BASE..=APLIC_SET_PENDING_TOP).contains(&offset) {
            // setip
            panic!("setip Unexpected addr {:#x}", addr);
        } else if (APLIC_SET_PENDING_NUM_BASE..=APLIC_SET_PENDING_NUM_TOP).contains(&offset) {
            // setipnum
            panic!("setipnum Unexpected addr {:#x}", addr); */
        } else if (APLIC_CLR_PENDING_BASE..=APLIC_CLR_PENDING_TOP).contains(&offset) {
            // inclrip
            let irqidx = (offset - APLIC_CLR_PENDING_BASE) / 4;
            let value = self.get_in_clrip(irqidx);
            debug!("APLIC read in clrip addr@{:#x} irqidx {} value {}", addr, irqidx, value);
            value
        /* } else if (APLIC_CLR_PENDING_NUM_BASE..=APLIC_CLR_PENDING_NUM_TOP).contains(&offset) {
            // clripnum
            panic!("clripnum Unexpected addr {:#x}", addr);
        } else if (APLIC_SET_ENABLE_BASE..=APLIC_SET_ENABLE_TOP).contains(&offset) {
            // setie
            panic!("setie Unexpected addr {:#x}", addr);
        } else if (APLIC_SET_ENABLE_NUM_BASE..=APLIC_SET_ENABLE_NUM_TOP).contains(&offset) {
            // setienum
            panic!("setienum Unexpected addr {:#x}", addr);
        } else if (APLIC_CLR_ENABLE_BASE..=APLIC_CLR_ENABLE_TOP).contains(&offset) {
            // clrie
            panic!("clrie Unexpected addr {:#x}", addr);
        } else if (APLIC_CLR_ENABLE_NUM_BASE..=APLIC_CLR_ENABLE_NUM_TOP).contains(&offset) {
            // clrienum
            panic!("clrienum Unexpected addr {:#x}", addr);
        } else if (APLIC_SET_IPNUM_LE_BASE..=APLIC_SET_IPNUM_LE_TOP).contains(&offset) {
            // setipnum_le
            panic!("clrienum Unexpected addr {:#x}", addr);
        } else if (APLIC_SET_IPNUM_BE_BASE..=APLIC_SET_IPNUM_BE_TOP).contains(&offset) {
            // setipnum_be
            panic!("clrienum Unexpected addr {:#x}", addr);
        } else if (APLIC_GENMSI_BASE..=APLIC_GENMSI_TOP).contains(&offset) {
            // genmsi
            panic!("genmsi Unexpected addr {:#x}", addr);
        } else if (APLIC_TARGET_BASE..=APLIC_TARGET_TOP).contains(&offset) {
            // target
            panic!("target Unexpected addr {:#x}", addr); */
        } else {
            panic!("invalid plic register access: 0x{:x} offset: 0x{:x}", addr, offset);
        }
    }

    pub fn vm_write_register(&self, addr: usize, value: u32) {
        // debug!("vm_write_register:0x{:#x}", addr);
        let offset = addr - self.get_emulated_base_addr();
        // align to 4B
        let offset = offset & !0x3;
        if (APLIC_DOMAINCFG_BASE..=APLIC_DOMAINCFG_TOP).contains(&offset) {
            // domaincfg
            let enabled = ((value >> 8) & 0x1) != 0; // IE
            let msimode = ((value >> 2) & 0b1) != 0; // DM / MSI
            let bigendian = (value & 0b1) != 0; // Endianness
            self.set_domaincfg(bigendian, msimode, enabled);
            debug!(
                "APLIC set domaincfg write addr@{:#x} bigendian {} msimode {} enabled {}",
                addr, bigendian, msimode, enabled
            );
        } else if (APLIC_SOURCECFG_BASE..=APLIC_SOURCECFG_TOP).contains(&offset) {
            // sourcecfg
            let irq = ((offset - APLIC_SOURCECFG_BASE) / 4) + 1;
            if (value >> 10) & 0b1 == 1 {
                //delegate
                let child = value & 0x3ff;
                self.set_sourcecfg_delegate(irq as u32, child);
                debug!(
                    "APLIC set sourcecfg_delegate write addr@{:#x} irq {} child {}",
                    addr, irq, child
                );
            } else {
                let mode = match value {
                    0 => SourceModes::Inactive,
                    1 => SourceModes::Detached,
                    4 => SourceModes::RisingEdge,
                    5 => SourceModes::FallingEdge,
                    6 => SourceModes::LevelHigh,
                    7 => SourceModes::LevelLow,
                    _ => panic!("Unknown sourcecfg mode"),
                };
                self.set_sourcecfg(irq as u32, mode);
                debug!("APLIC set sourcecfg write addr@{:#x} irq {} mode {}", addr, irq, value);
            }
        } else if (APLIC_S_MSIADDR_BASE..=APLIC_S_MSIADDR_TOP).contains(&offset) {
            // smsiaddrcfg
            let address = (value as usize) << 12;
            self.set_msiaddr(address);
            debug!("APLIC set msiaddr write addr@{:#x} address {}", addr, address);
        } else if (APLIC_SET_PENDING_BASE..=APLIC_SET_PENDING_TOP).contains(&offset) {
            // setip
            panic!("setip Unexpected addr {:#x}", addr);
        } else if (APLIC_SET_PENDING_NUM_BASE..=APLIC_SET_PENDING_NUM_TOP).contains(&offset) {
            // setipnum
            panic!("setipnum Unexpected addr {:#x}", addr);
        } else if (APLIC_CLR_PENDING_BASE..=APLIC_CLR_PENDING_TOP).contains(&offset) {
            // clrip
            panic!("clrip Unexpected addr {:#x}", addr);
        } else if (APLIC_CLR_PENDING_NUM_BASE..=APLIC_CLR_PENDING_NUM_TOP).contains(&offset) {
            // clripnum
            panic!("clripnum Unexpected addr {:#x}", addr);
        } else if (APLIC_SET_ENABLE_BASE..=APLIC_SET_ENABLE_TOP).contains(&offset) {
            // setie
            panic!("setie Unexpected addr {:#x}", addr);
        } else if (APLIC_SET_ENABLE_NUM_BASE..=APLIC_SET_ENABLE_NUM_TOP).contains(&offset) {
            // setienum
            self.set_enable_num(value);
            debug!("APLIC set enablenum write addr@{:#x} value {}", addr, value);
        } else if (APLIC_CLR_ENABLE_BASE..=APLIC_CLR_ENABLE_TOP).contains(&offset) {
            // clrie
            let irqidx = (offset - APLIC_CLR_ENABLE_BASE) / 4;
            self.set_enable(irqidx, value, false);
            debug!(
                "APLIC set clr_enable write addr@{:#x} irqidx {} value {}",
                addr, irqidx, value
            );
        } else if (APLIC_CLR_ENABLE_NUM_BASE..=APLIC_CLR_ENABLE_NUM_TOP).contains(&offset) {
            // clrienum
            self.clr_enable_num(value);
            debug!("APLIC set clrienum write addr@{:#x} value{}", offset, value);
        } else if (APLIC_SET_IPNUM_LE_BASE..=APLIC_SET_IPNUM_LE_TOP).contains(&offset) {
            // setipnum_le
            self.setipnum_le(value);
            // debug!("APLIC setipnum le write addr@{:#x} value@{:#x}",addr, value);
        } else if (APLIC_SET_IPNUM_BE_BASE..=APLIC_SET_IPNUM_BE_TOP).contains(&offset) {
            // setipnum_be
            warn!("setipnum_be Unexpected addr {:#x}, {:?}", addr, value);
        } else if (APLIC_GENMSI_BASE..=APLIC_GENMSI_TOP).contains(&offset) {
            // genmsi
            panic!("genmsi Unexpected addr {:#x}", addr);
        } else if (APLIC_TARGET_BASE..=APLIC_TARGET_TOP).contains(&offset) {
            // target
            let irq = ((offset - APLIC_TARGET_BASE) / 4) as u32 + 1;
            // An error may occur when the CPU ID of a virtual machine is not 0, but it assumes its CPU ID is 0.
            let hart = (value >> 18) & 0x3F;
            // let hart = ((value >> 18) & 0x3F) + (current_cpu.first_cpu) as u32;
            if self.get_msimode() {
                let guest = ((value >> 12) & 0x3F) + 1;
                let eiid = value & 0xFFF;
                self.set_target_msi(irq, hart, guest, eiid);
                debug!(
                    "APLIC set msi target write addr@{:#x} irq {} hart {} guest {} eiid {}",
                    addr, irq, hart, guest, eiid
                );
            } else {
                let prio = value & 0xFF;
                self.set_target_direct(irq, hart, prio);
                debug!(
                    "APLIC set direct target write addr@{:#x} irq {} hart {} prio {}",
                    addr, irq, hart, prio
                );
            }
        } else {
            panic!("invalid plic register access: 0x{:x} offset: 0x{:x}", addr, offset);
        }
    }
    pub fn inject_intr(&self, irq: usize) {
        if irq == IRQ_GUEST_TIMER {
            riscv::register::hvip::trigger_timing_interrupt();
            // SAFETY: Disable timer interrupt
            unsafe { sie::clear_stimer() };
        } else if irq == IRQ_IPI {
            riscv::register::hvip::trigger_software_interrupt();
        } else {
            warn!("external_intr");
        }
    }

    // // Check and inject a APLIC interrupt to VM
    // fn inject_external_intr(&self, irq: usize) {}
}

#[allow(unused_variables)]
pub fn emu_intc_init(emu_cfg: &VmEmulatedDeviceConfig, vcpu_list: &[Vcpu]) -> Result<Arc<dyn EmuDev>, ()> {
    // Create and initialize the virtual interrupt controller:
    // Create a new VPLIC object and call vm.set_emu_devs to add the emulated device to the Vm
    let vaplic = VAPlic::new(emu_cfg.base_ipa);
    Ok(Arc::new(vaplic))
}

impl EmuDev for VAPlic {
    fn emu_type(&self) -> crate::device::EmuDeviceType {
        crate::device::EmuDeviceType::EmuDeviceTAPlic
    }

    fn address_range(&self) -> core::ops::Range<usize> {
        let base_addr = self.get_emulated_base_addr();
        base_addr..base_addr + VPLIC_LENGTH
    }

    // Emulated aplic's entry
    fn handler(&self, emu_ctx: &EmuContext) -> bool {
        let vm = active_vm().unwrap();
        let vaplic = vm.vaplic();
        let reg_idx = emu_ctx.reg;

        if emu_ctx.width != 4 {
            panic!("emu_plic_mmio_handler: invalid width {}", emu_ctx.width);
        }

        // TODO: Research whether there is a requirement to implement other bit widths (other than 32bit)
        if emu_ctx.write {
            vaplic.vm_write_register(emu_ctx.address, current_cpu().get_gpr(reg_idx) as u32);
        } else {
            let val = vaplic.vm_read_register(emu_ctx.address);
            current_cpu().set_gpr(reg_idx, val as usize);
        }

        true
    }
}
