// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use core::fmt;
use core::mem::size_of;

use alloc::collections::BTreeSet;

use spin::Mutex;
use tock_registers::*;
use tock_registers::interfaces::*;
use tock_registers::registers::*;

use crate::board::{Platform, PlatOperation};
use crate::utils::bit_extract;
use crate::kernel::current_cpu;
use crate::kernel::INTERRUPT_NUM_MAX;

pub const MPIDR_AFF_MSK: usize = 0xffff; //we are only supporting 2 affinity levels

// GICD BITS
const GICD_CTLR_ENS_BIT: usize = 0x1;
const GICD_CTLR_ENNS_BIT: usize = 0b10;
pub const GICD_CTLR_ARE_NS_BIT: usize = 0x1 << 4;
pub const GICD_IROUTER_INV: usize = !MPIDR_AFF_MSK;
pub const GICD_IROUTER_RES0_MSK: usize = (1 << 40) - 1;
pub const GICD_IROUTER_IRM_BIT: usize = 1 << 31;
pub const GICD_TYPER_IDBITS_OFF: usize = 19;
pub const GICD_TYPER_IDBITS_LEN: usize = 5;
pub const GICD_TYPER_IDBITS_MSK: usize = (((1 << ((GICD_TYPER_IDBITS_LEN) - 1)) << 1) - 1) << (GICD_TYPER_IDBITS_OFF);
const GICD_IROUTER_AFF_MSK: usize = GICD_IROUTER_RES0_MSK & !GICD_IROUTER_IRM_BIT;
pub const GICD_TYPER_LPIS: usize = 1 << 17;

//GICR BITS
pub const GICR_TYPER_PRCNUM_OFF: usize = 8;
pub const GICR_TYPER_AFFVAL_OFF: usize = 32;
pub const GICR_TYPER_LAST_OFF: usize = 4;
const GICR_WAKER_PSLEEP_BIT: usize = 0x2;
const GICR_WAKER_CASLEEP_BIT: usize = 0x4;

// GICC BITS
pub const GICC_CTLR_EN_BIT: usize = 0x1;
pub const GICC_CTLR_EOIMODENS_BIT: usize = 1 << 9;
pub const GICC_SRE_SRE_BIT: usize = 0x1;
pub const GICC_CTLR_EOIMODE_BIT: usize = 0x1 << 1;
pub const GICC_IGRPEN_EL1_ENB_BIT: usize = 0x1;
pub const GICC_SGIR_AFF1_OFFSET: usize = 16;
pub const GICC_SGIR_SGIINTID_OFF: usize = 24;
pub const GICC_SGIR_SGIINTID_LEN: usize = 4;
pub const GICC_IAR_ID_OFF: usize = 0;
pub const GICC_IAR_ID_LEN: usize = 24;
pub const GICC_SGIR_IRM_BIT: usize = 1 << 40;
pub const GICC_SRE_EL2_ENABLE: usize = 1 << 3;

// GICH BITS
pub const GICH_LR_VID_OFF: usize = 0;
pub const GICH_LR_VID_LEN: usize = 32;
pub const GICH_LR_VID_MASK: usize = (((1 << ((GICH_LR_VID_LEN) - 1)) << 1) - 1) << (GICH_LR_VID_OFF);
pub const GICH_LR_PID_OFF: usize = 32;
pub const GICH_LR_PID_LEN: usize = 10;
pub const GICH_LR_PID_MSK: usize = (((1 << ((GICH_LR_PID_LEN) - 1)) << 1) - 1) << (GICH_LR_PID_OFF);
pub const GICH_LR_PRIO_OFF: usize = 48;
pub const GICH_LR_PRIO_LEN: usize = 8;
pub const GICH_LR_STATE_LEN: usize = 2;
pub const GICH_LR_STATE_OFF: usize = 62;
pub const GICH_LR_STATE_MSK: usize = (((1 << ((GICH_LR_STATE_LEN) - 1)) << 1) - 1) << (GICH_LR_STATE_OFF);
pub const GICH_LR_STATE_ACT: usize = (2 << GICH_LR_STATE_OFF) & GICH_LR_STATE_MSK;
pub const GICH_LR_STATE_PND: usize = (1 << GICH_LR_STATE_OFF) & GICH_LR_STATE_MSK;
pub const GICH_LR_GRP_BIT: usize = 1 << 60;
pub const GICH_LR_PRIO_MSK: usize = (((1 << ((GICH_LR_PRIO_LEN) - 1)) << 1) - 1) << (GICH_LR_PRIO_OFF);
pub const GICH_LR_HW_BIT: usize = 1 << 61;
pub const GICH_LR_EOI_BIT: usize = 1 << 41;
/* End Of Interrupt.
 * This maintenance interrupt is asserted when at least one bit in GICH_EISR == 1.
 */
pub const GICH_HCR_LRENPIE_BIT: usize = 1 << 2;
pub const GICH_HCR_EN_BIT: usize = 1;
pub const GICH_HCR_UIE_BIT: usize = 1 << 1;
pub const GICH_HCR_NPIE_BIT: usize = 1 << 3;
/* Counts the number of EOIs received that do not have a corresponding entry in the List registers.
 * The virtual CPU interface increments this field automatically when a matching EOI is received
 */
pub const GICH_HCR_EOIC_OFF: usize = 27;
pub const GICH_HCR_EOIC_LEN: usize = 5;
pub const GICH_HCR_EOIC_MSK: usize = (((1 << ((GICH_HCR_EOIC_LEN) - 1)) << 1) - 1) << (GICH_HCR_EOIC_OFF);
pub const GICH_MISR_U: usize = 1 << 1;
pub const GICH_MISR_EOI: usize = 1;
/* No Pending.
 * This maintenance interrupt is asserted
 * when GICH_HCR.NPIE == 1 and no List register is in the pending state.
 */
pub const GICH_MISR_NP: usize = 1 << 3;
/* List Register Entry Not Present.
 * This maintenance interrupt is asserted
 * when GICH_HCR.LRENPIE == 1 and GICH_HCR.EOICount is nonzero.
 */
pub const GICH_MISR_LRPEN: usize = 1 << 2;
const GICH_NUM_ELRSR: usize = 1;
const GICH_VTR_MSK: usize = 0b11111;
const GICH_VTR_PRIBITS_OFF: usize = 29;
const GICH_VTR_PRIBITS_LEN: usize = 3;
const GICH_PMR_MASK: usize = 0xff;
const GICH_VMCR_VPMR_SHIFT: usize = 24;
const GICC_IGRPEN1_EN: usize = 0x1;
const GICH_VMCR_VENG1: usize = 0x1 << 1;
const GICH_VMCR_VEOIM: usize = 0x1 << 9;

pub const GIC_SGIS_NUM: usize = 16;
const GIC_PPIS_NUM: usize = 16;
pub const GIC_INTS_MAX: usize = INTERRUPT_NUM_MAX;
pub const GIC_PRIVINT_NUM: usize = GIC_SGIS_NUM + GIC_PPIS_NUM;
pub const GIC_SPI_MAX: usize = INTERRUPT_NUM_MAX - GIC_PRIVINT_NUM;
pub const GIC_PRIO_BITS: usize = 8;
pub const GIC_TARGET_BITS: usize = 8;
pub const GIC_TARGETS_MAX: usize = GIC_TARGET_BITS;
pub const GIC_CONFIG_BITS: usize = 2;

const GIC_INT_REGS_NUM: usize = GIC_INTS_MAX / 32;
const GIC_PRIO_REGS_NUM: usize = GIC_INTS_MAX * 8 / 32;
const GIC_TARGET_REGS_NUM: usize = GIC_INTS_MAX * 8 / 32;
const GIC_CONFIG_REGS_NUM: usize = GIC_INTS_MAX * 2 / 32;
const GIC_SEC_REGS_NUM: usize = GIC_INTS_MAX * 2 / 32;
pub const GIC_SGI_REGS_NUM: usize = GIC_SGIS_NUM * 8 / 32;
const GIC_INT_RT_NUM: usize = 1019 - 32 + 1;

pub const GIC_LIST_REGS_NUM: usize = 64;

pub const GICD_TYPER_CPUNUM_OFF: usize = 5;
pub const GICD_TYPER_CPUNUM_LEN: usize = 3;
pub const GICD_TYPER_CPUNUM_MSK: usize = ((1 << GICD_TYPER_CPUNUM_LEN) - 1) << (GICD_TYPER_CPUNUM_OFF);
const GICD_TYPER_ITLINESNUM_LEN: usize = 0b11111;
pub const ICC_CTLR_EOIMODE_BIT: usize = 0x1 << 1;

pub static GIC_LRS_NUM: Mutex<usize> = Mutex::new(0);

static GICD_LOCK: Mutex<()> = Mutex::new(());
static GICR_LOCK: Mutex<()> = Mutex::new(());

pub static INTERRUPT_EN_SET: Mutex<BTreeSet<usize>> = Mutex::new(BTreeSet::new());

pub fn add_en_interrupt(id: usize) {
    if id < GIC_PRIVINT_NUM {
        return;
    }
    let mut set = INTERRUPT_EN_SET.lock();
    set.insert(id);
}

pub fn show_en_interrupt() {
    let set = INTERRUPT_EN_SET.lock();
    print!("en irq set: ");
    for irq in set.iter() {
        print!("{} ", irq);
    }
    print!("\n");
}

pub fn gic_prio_reg(int_id: usize) -> usize {
    (int_id * GIC_PRIO_BITS) / 32
}

pub fn gic_prio_off(int_id: usize) -> usize {
    (int_id * GIC_PRIO_BITS) % 32
}

#[derive(Copy, Clone, Debug)]
pub enum IrqState {
    IrqSInactive,
    IrqSPend,
    IrqSActive,
    IrqSPendActive,
}

impl IrqState {
    pub fn num_to_state(num: usize) -> IrqState {
        match num {
            0 => IrqState::IrqSInactive,
            1 => IrqState::IrqSPend,
            2 => IrqState::IrqSActive,
            3 => IrqState::IrqSPendActive,
            _ => panic!("num_to_state: illegal irq state"),
        }
    }

    pub fn to_num(&self) -> usize {
        match self {
            IrqState::IrqSInactive => 0,
            IrqState::IrqSPend => 1,
            IrqState::IrqSActive => 2,
            IrqState::IrqSPendActive => 3,
        }
    }
}

pub struct GicDesc {
    pub gicd_addr: usize,
    pub gicc_addr: usize,
    pub gich_addr: usize,
    pub gicv_addr: usize,
    pub gicr_addr: usize,
    pub maintenance_int_id: usize,
}

register_structs! {
    #[allow(non_snake_case)]
    pub GicDistributorBlock {
        (0x0000 => CTLR: ReadWrite<u32>), //Distributor Control Register
        (0x0004 => TYPER: ReadOnly<u32>), //Interrupt Controller Type Register
        (0x0008 => IIDR: ReadOnly<u32>),  //Distributor Implementer Identification Register
        (0x000c => TYPER2: ReadOnly<u32>), //Interrupt controller Type Register 2
        (0x0010 => STATUSR: ReadWrite<u32>), //Error Reporting Status Register, optional
        (0x0014 => reserved0),
        (0x0040 => SETSPI_NSR: WriteOnly<u32>), //Set SPI Register
        (0x0044 => reserved1),
        (0x0048 => CLRSPI_NSR: WriteOnly<u32>), //Clear SPI Register
        (0x004c => reserved2),
        (0x0050 => SETSPI_SR: WriteOnly<u32>), //Set SPI, Secure Register
        (0x0054 => reserved3),
        (0x0058 => CLRSPI_SR: WriteOnly<u32>), //Clear SPI, Secure Register
        (0x005c => reserved4),
        (0x0080 => IGROUPR: [ReadWrite<u32>; GIC_INT_REGS_NUM]), //Interrupt Group Registers
        (0x0100 => ISENABLER: [ReadWrite<u32>; GIC_INT_REGS_NUM]), //Interrupt Set-Enable Registers
        (0x0180 => ICENABLER: [ReadWrite<u32>; GIC_INT_REGS_NUM]), //Interrupt Clear-Enable Registers
        (0x0200 => ISPENDR: [ReadWrite<u32>; GIC_INT_REGS_NUM]), //Interrupt Set-Pending Registers
        (0x0280 => ICPENDR: [ReadWrite<u32>; GIC_INT_REGS_NUM]), //Interrupt Clear-Pending Registers
        (0x0300 => ISACTIVER: [ReadWrite<u32>; GIC_INT_REGS_NUM]), //Interrupt Set-Active Registers
        (0x0380 => ICACTIVER: [ReadWrite<u32>; GIC_INT_REGS_NUM]), //Interrupt Clear-Active Registers
        (0x0400 => IPRIORITYR: [ReadWrite<u32>; GIC_PRIO_REGS_NUM]), //Interrupt Priority Registers
        (0x0800 => ITARGETSR: [ReadWrite<u32>; GIC_TARGET_REGS_NUM]), //Interrupt Processor Targets Registers
        (0x0c00 => ICFGR: [ReadWrite<u32>; GIC_CONFIG_REGS_NUM]), //Interrupt Configuration Registers
        (0x0d00 => IGRPMODR: [ReadWrite<u32>; GIC_CONFIG_REGS_NUM]), //Interrupt Group Modifier Registers
        (0x0e00 => NSACR: [ReadWrite<u32>; GIC_SEC_REGS_NUM]), //Non-secure Access Control Registers
        (0x0f00 => SGIR: WriteOnly<u32>),  //Software Generated Interrupt Register
        (0x0f04 => reserved6),
        (0x0f10 => CPENDSGIR: [ReadWrite<u32>; GIC_SGI_REGS_NUM]), //SGI Clear-Pending Registers
        (0x0f20 => SPENDSGIR: [ReadWrite<u32>; GIC_SGI_REGS_NUM]), //SGI Set-Pending Registers
        (0x0f30 => reserved7),
        (0x6000 => IROUTER: [ReadWrite<u64>; (0x8000 - 0x6000) / size_of::<u64>()]), //Interrupt Routing Registers for extended SPI range
        (0x8000 => reserved21),
        (0xffd0 => ID: [ReadOnly<u32>; (0x10000 - 0xffd0) / size_of::<u32>()]), //Reserved for ID registers
        (0x10000 => @END),
    }
}

impl core::fmt::Debug for GicDistributorBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GicDistributorBlock")
            .field("CTLR", &format_args!("{:x}", self.CTLR.get()))
            .field("TYPER", &format_args!("{:x}", self.TYPER.get()))
            .field("IIDR", &format_args!("{:x}", self.IIDR.get()))
            .field("TYPER2", &format_args!("{:x}", self.TYPER2.get()))
            .field("STATUSR", &format_args!("{:x}", self.STATUSR.get()))
            .finish()
    }
}
pub struct GicDistributor {
    base_addr: usize,
}

impl core::ops::Deref for GicDistributor {
    type Target = GicDistributorBlock;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.ptr() }
    }
}

impl GicDistributor {
    const fn new(base_addr: usize) -> GicDistributor {
        GicDistributor { base_addr }
    }

    pub fn ptr(&self) -> *const GicDistributorBlock {
        self.base_addr as *const GicDistributorBlock
    }

    pub fn is_enabler(&self, idx: usize) -> u32 {
        self.ISENABLER[idx].get()
    }

    pub fn is_activer(&self, idx: usize) -> u32 {
        self.ISACTIVER[idx].get()
    }

    pub fn is_pender(&self, idx: usize) -> u32 {
        self.ISPENDR[idx].get()
    }

    pub fn cpendsgir(&self, idx: usize) -> u32 {
        self.CPENDSGIR[idx].get()
    }

    pub fn igroup(&self, idx: usize) -> u32 {
        self.IGROUPR[idx].get()
    }

    pub fn ipriorityr(&self, idx: usize) -> u32 {
        self.IPRIORITYR[idx].get()
    }

    pub fn itargetsr(&self, idx: usize) -> u32 {
        self.ITARGETSR[idx].get()
    }

    pub fn ctlr(&self) -> u32 {
        self.CTLR.get()
    }

    pub fn id(&self, idx: usize) -> u32 {
        self.ID[idx].get()
    }

    pub fn icfgr(&self, idx: usize) -> u32 {
        self.ICFGR[idx].get()
    }

    pub fn ic_enabler(&self, idx: usize) -> u32 {
        self.ICENABLER[idx].get()
    }

    fn global_init(&self) {
        let int_num = gic_max_spi();

        for i in (GIC_PRIVINT_NUM / 32)..(int_num / 32) {
            self.IGROUPR[i].set(u32::MAX);
            self.ICENABLER[i].set(u32::MAX);
            self.ICPENDR[i].set(u32::MAX);
            self.ICACTIVER[i].set(u32::MAX);
        }

        for i in (GIC_PRIVINT_NUM * 8 / 32)..(int_num * 8 / 32) {
            self.IPRIORITYR[i].set(u32::MAX);
        }

        for i in GIC_PRIVINT_NUM..GIC_INTS_MAX {
            self.IROUTER[i].set(GICD_IROUTER_INV as u64);
        }

        let prev = self.CTLR.get();

        self.CTLR
            .set(prev | GICD_CTLR_ARE_NS_BIT as u32 | GICD_CTLR_ENNS_BIT as u32);
    }

    pub fn send_sgi(&self, cpu_target: usize, sgi_num: usize) {
        if sgi_num < GIC_SGIS_NUM {
            let mpidr = Platform::cpuid_to_cpuif(cpu_target) & MPIDR_AFF_MSK;
            /* We only support two affinity levels */
            let sgi = ((((mpidr) >> 8) & 0xff) << GICC_SGIR_AFF1_OFFSET) //aff1
                | (1 << (mpidr & 0xff))  //aff0
                | ((sgi_num) << GICC_SGIR_SGIINTID_OFF); //sgi_num
            msr!(ICC_SGI1R_EL1, sgi as u64);
        }
    }

    pub fn prio(&self, int_id: usize) -> usize {
        let idx = (int_id * GIC_PRIO_BITS) / 32;
        let off = (int_id * GIC_PRIO_BITS) % 32;
        ((self.IPRIORITYR[idx].get() >> off) & 0xff) as usize
    }

    pub fn set_prio(&self, int_id: usize, prio: u8) {
        let idx = ((int_id) * GIC_PRIO_BITS) / 32;
        let off = (int_id * GIC_PRIO_BITS) % 32;
        let mask = ((1 << (GIC_PRIO_BITS)) - 1) << off;

        let lock = GICD_LOCK.lock();

        let prev: u32 = self.IPRIORITYR[idx].get();
        let value = (prev & !(mask as u32)) | (((prio as u32) << off) & mask as u32);
        self.IPRIORITYR[idx].set(value);

        drop(lock);
    }

    pub fn trgt(&self, int_id: usize) -> usize {
        let idx = (int_id * 8) / 32;
        let off = (int_id * 8) % 32;
        ((self.ITARGETSR[idx].get() >> off) & 0xff) as usize
    }

    pub fn set_trgt(&self, int_id: usize, trgt: u8) {
        let idx = (int_id * 8) / 32;
        let off = (int_id * 8) % 32;
        let mask: u32 = 0b11111111 << off;

        let lock = GICD_LOCK.lock();
        let prev = self.ITARGETSR[idx].get();
        let value = (prev & !mask) | (((trgt as u32) << off) & mask);
        self.ITARGETSR[idx].set(value);
        drop(lock);
    }

    pub fn set_enable(&self, int_id: usize, en: bool) {
        let reg_id = int_id / 32;
        let mask = 1 << (int_id % 32);

        let lock = GICD_LOCK.lock();

        if en {
            add_en_interrupt(int_id);
            self.ISENABLER[reg_id].set(mask);
        } else {
            self.ICENABLER[reg_id].set(mask);
        }

        drop(lock);
    }

    pub fn get_pend(&self, int_id: usize) -> bool {
        let reg_id = int_id / 32;
        let mask = 1 << (int_id % 32);
        (self.ISPENDR[reg_id].get() & (mask as u32)) != 0
    }

    pub fn get_act(&self, int_id: usize) -> bool {
        let reg_id = int_id / 32;
        let mask = 1 << (int_id % 32);
        (self.ISACTIVER[reg_id].get() & (mask as u32)) != 0
    }

    pub fn set_pend(&self, int_id: usize, pend: bool) {
        let lock = GICD_LOCK.lock();

        let reg_ind = int_id / 32;
        let mask = 1 << (int_id % 32);
        if pend {
            self.ISPENDR[reg_ind].set(mask);
        } else {
            self.ICPENDR[reg_ind].set(mask);
        }

        drop(lock);
    }

    pub fn set_act(&self, int_id: usize, act: bool) {
        let reg_ind = int_id / 32;
        let mask = 1 << (int_id % 32);

        let lock = GICD_LOCK.lock();
        if act {
            self.ISACTIVER[reg_ind].set(mask);
        } else {
            self.ICACTIVER[reg_ind].set(mask);
        }
        drop(lock);
    }

    pub fn set_icfgr(&self, int_id: usize, cfg: u8) {
        let lock = GICD_LOCK.lock();
        let reg_ind = (int_id * GIC_CONFIG_BITS) / 32;
        let off = (int_id * GIC_CONFIG_BITS) % 32;
        let mask = 0b11 << off;

        let icfgr = self.ICFGR[reg_ind].get();
        self.ICFGR[reg_ind].set((icfgr & !mask) | (((cfg as u32) << off as u32) & mask));

        drop(lock);
    }

    pub fn typer(&self) -> u32 {
        self.TYPER.get()
    }

    pub fn iidr(&self) -> u32 {
        self.IIDR.get()
    }

    pub fn state(&self, int_id: usize) -> usize {
        let reg_ind = int_id / 32;
        let mask = 1 << (int_id % 32);

        let lock = GICD_LOCK.lock();
        let pend = if (self.ISPENDR[reg_ind].get() & mask) != 0 {
            1
        } else {
            0
        };
        let act = if (self.ISACTIVER[reg_ind].get() & mask) != 0 {
            2
        } else {
            0
        };
        drop(lock);
        pend | act
    }

    pub fn set_route(&self, int_id: usize, route: usize) {
        if gic_is_priv(int_id) {
            return;
        }

        let lock = GICD_LOCK.lock();

        self.IROUTER[int_id].set((route & GICD_IROUTER_AFF_MSK) as u64);

        drop(lock)
    }
}

register_structs! {
    #[allow(non_snake_case)]
    pub GicRedistributorBlock {
        (0x0000 => CTLR: ReadWrite<u32>),   // Redistributor Control Register
        (0x0004 => IIDR: ReadOnly<u32>),    // Implementer Identification Register
        (0x0008 => TYPER: ReadOnly<u64>),   // Redistributor Type Register
        (0x0010 => STATUSR: ReadWrite<u32>),  // Error Reporting Status Register, optional
        (0x0014 => WAKER: ReadWrite<u32>),     // Redistributor Wake Register
        (0x0018 => MPAMIDR: ReadOnly<u32>),   // Report maximum PARTID and PMG Register
        (0x001c => PARTIDR: ReadWrite<u32>),   // Set PARTID and PMG Register
        (0x0020 => reserved18),
        (0x0040 => SETLPIR: WriteOnly<u64>),    // Set LPI Pending Register
        (0x0048 => CLRLPIR: WriteOnly<u64>),  // Clear LPI Pending Register
        (0x0050 => reserved17),
        (0x0070 => PROPBASER: ReadWrite<u64>),  //Redistributor Properties Base Address Register
        (0x0078 => PEDNBASER: ReadWrite<u64>),    //Redistributor LPI Pending Table Base Address Register
        (0x0080 => reserved16),
        (0x00a0 => INVLPIR: WriteOnly<u64>),  // Redistributor Invalidate LPI Register
        (0x00a8 => reserved15),
        (0x00b0 => INVALLR: WriteOnly<u64>),    // Redistributor Invalidate All Register
        (0x00b8 => reserved14),
        (0x00c0 => SYNCR: ReadOnly<u64>),    // Redistributor Synchronize Register
        (0x00c8 => reserved13),
        (0xffd0 => ID: [ReadOnly<u32>; (0x10000 - 0xFFD0) / size_of::<u32>()]),
        (0x10000 => reserved12),
        (0x10080 => IGROUPR0: ReadWrite<u32>), //SGI_base frame, all below
        (0x10084 => reserved11),
        (0x10100 => ISENABLER0: ReadWrite<u32>),
        (0x10104 => reserved10),
        (0x10180 => ICENABLER0: ReadWrite<u32>),
        (0x10184 => reserved9),
        (0x10200 => ISPENDR0: ReadWrite<u32>),
        (0x10204 => reserved8),
        (0x10280 => ICPENDR0: ReadWrite<u32>),
        (0x10284 => reserved7),
        (0x10300 => ISACTIVER0: ReadWrite<u32>),
        (0x10304 => reserved6),
        (0x10380 => ICACTIVER0: ReadWrite<u32>),
        (0x10384 => reserved5),
        (0x10400 => IPRIORITYR: [ReadWrite<u32>;8]),
        (0x10420 => reserved4),
        (0x10c00 => ICFGR0: ReadWrite<u32>),
        (0x10c04 => ICFGR1: ReadWrite<u32>),
        (0x10c08 => reserved3),
        (0x10d00 => IGRPMODR0: ReadWrite<u32>),
        (0x10d04 => reserved2),
        (0x10e00 => NSACR: ReadWrite<u32>),
        (0x10e04 => reserved1),
        (0x20000 => @END),
  }
}

impl core::fmt::Debug for GicRedistributorBlock {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GicRedistributorBlock")
            .field("Current_cpu", &current_cpu().id)
            .field("CTLR", &format_args!("{:#x}", self.CTLR.get()))
            .field("IIDR", &format_args!("{:#x}", self.IIDR.get()))
            .field("TYPER", &format_args!("{:#x}", self.TYPER.get()))
            .field("STATUSR", &format_args!("{:#x}", self.STATUSR.get()))
            .field("WAKER", &format_args!("{:#x}", self.WAKER.get()))
            .field("MPAMIDR", &format_args!("{:#x}", self.MPAMIDR.get()))
            .field("PARTIDR", &format_args!("{:#x}", self.PARTIDR.get()))
            .field("PROPBASER", &format_args!("{:#x}", self.PROPBASER.get()))
            .field("PEDNBASER", &format_args!("{:#x}", self.PEDNBASER.get()))
            .field("SYNCR", &format_args!("{:#x}", self.SYNCR.get()))
            .field("IGROUPR0", &format_args!("{:#x}", self.IGROUPR0.get()))
            .field("ISENABLER0", &format_args!("{:#x}", self.ISENABLER0.get()))
            .field("ICENABLER0", &format_args!("{:#x}", self.ICENABLER0.get()))
            .field("ISPENDR0", &format_args!("{:#x}", self.ISPENDR0.get()))
            .field("ICPENDR0", &format_args!("{:#x}", self.ICPENDR0.get()))
            .field("ISACTIVER0", &format_args!("{:#x}", self.ISACTIVER0.get()))
            .field("ICACTIVER0", &format_args!("{:#x}", self.ICACTIVER0.get()))
            .field("ICFGR0", &format_args!("{:#x}", self.ICFGR0.get()))
            .field("ICFGR1", &format_args!("{:#x}", self.ICFGR1.get()))
            .field("IGRPMODR0", &self.IGRPMODR0.get())
            .field("NSACR", &format_args!("{:#x}", self.NSACR.get()))
            .finish()
    }
}

pub struct GicRedistributor {
    base_addr: usize,
}

impl core::ops::Deref for GicRedistributor {
    type Target = GicRedistributorBlock;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.ptr() }
    }
}

impl core::ops::Index<usize> for GicRedistributor {
    type Output = GicRedistributorBlock;
    fn index(&self, index: usize) -> &Self::Output {
        unsafe { &*self.ptr().offset(index as isize) }
    }
}

impl GicRedistributor {
    pub const fn new(base_addr: usize) -> GicRedistributor {
        GicRedistributor { base_addr }
    }

    pub fn ptr(&self) -> *const GicRedistributorBlock {
        self.base_addr as *const GicRedistributorBlock
    }

    fn init(&self) {
        let waker = self[current_cpu().id].WAKER.get();
        self[current_cpu().id].WAKER.set(waker & !GICR_WAKER_PSLEEP_BIT as u32);
        while (self[current_cpu().id].WAKER.get() & GICR_WAKER_CASLEEP_BIT as u32) != 0 {}

        self[current_cpu().id].IGROUPR0.set(u32::MAX);
        self[current_cpu().id].ICENABLER0.set(u32::MAX);
        self[current_cpu().id].ICPENDR0.set(u32::MAX);
        self[current_cpu().id].ICACTIVER0.set(u32::MAX);

        for i in 0..gic_prio_reg(GIC_PRIVINT_NUM) {
            self[current_cpu().id].IPRIORITYR[i].set(u32::MAX);
        }
    }

    pub fn set_prio(&self, int_id: usize, prio: u8, gicr_id: u32) {
        let reg_id = gic_prio_reg(int_id);
        let off = gic_prio_off(int_id);
        let mask = (((1 << ((GIC_PRIO_BITS) - 1)) << 1) - 1) << (off);
        let lock = GICR_LOCK.lock(); //lock is for per core

        self[gicr_id as usize].IPRIORITYR[reg_id].set(
            (self[gicr_id as usize].IPRIORITYR[reg_id].get() & !mask as u32) | (((prio as usize) << off) & mask) as u32,
        );

        drop(lock);
    }

    pub fn get_prio(&self, int_id: usize, gicr_id: u32) -> usize {
        let reg_id = gic_prio_reg(int_id);
        let off = gic_prio_off(int_id);
        let mask = (((1 << ((GIC_PRIO_BITS) - 1)) << 1) - 1) << (off);
        let lock = GICR_LOCK.lock();

        let prio = (self[gicr_id as usize].IPRIORITYR[reg_id].get() as usize) >> off & mask;

        drop(lock);
        prio
    }

    pub fn set_icfgr(&self, int_id: usize, cfg: u8, gicr_id: u32) {
        let reg_id = (int_id * GIC_CONFIG_BITS) / u32::BITS as usize;
        let off = (int_id * GIC_CONFIG_BITS) % u32::BITS as usize;
        let mask = ((1 << (GIC_CONFIG_BITS)) - 1) << (off);

        let lock = GICR_LOCK.lock();

        match reg_id {
            0 => {
                self[gicr_id as usize].ICFGR0.set(
                    ((self[gicr_id as usize].ICFGR0.get() as usize & !mask) | (((cfg as usize) << off) & mask)) as u32,
                );
            }
            _ => {
                self[gicr_id as usize].ICFGR1.set(
                    ((self[gicr_id as usize].ICFGR1.get() as usize & !mask) | (((cfg as usize) << off) & mask)) as u32,
                );
            }
        }

        drop(lock);
    }

    pub fn set_pend(&self, int_id: usize, pend: bool, gicr_id: u32) {
        let lock = GICR_LOCK.lock();

        if pend {
            self[gicr_id as usize].ISPENDR0.set((1 << (int_id % 32)) as u32);
        } else {
            self[gicr_id as usize].ICPENDR0.set((1 << (int_id % 32)) as u32);
        }

        drop(lock);
    }

    pub fn get_pend(&self, int_id: usize, gicr_id: u32) -> bool {
        let mask = 1 << (int_id % 32);
        if gic_is_priv(int_id) {
            (self[gicr_id as usize].ISPENDR0.get() as usize & mask) != 0
        } else {
            false
        }
    }

    pub fn set_act(&self, int_id: usize, act: bool, gicr_id: u32) {
        let mask = 1 << (int_id % 32);

        let lock = GICR_LOCK.lock();

        if act {
            self[gicr_id as usize].ISACTIVER0.set(mask as u32);
        } else {
            self[gicr_id as usize].ICACTIVER0.set(mask as u32);
        }

        drop(lock);
    }

    pub fn get_act(&self, int_id: usize, gicr_id: u32) -> bool {
        let mask = 1 << (int_id % 32);
        if gic_is_priv(int_id) {
            (self[gicr_id as usize].ISACTIVER0.get() as usize & mask) != 0
        } else {
            false
        }
    }

    pub fn set_enable(&self, int_id: usize, en: bool, gicr_id: u32) {
        let mask = 1 << (int_id % 32);

        let lock = GICR_LOCK.lock();

        if en {
            add_en_interrupt(int_id);
            self[gicr_id as usize].ISENABLER0.set(mask);
        } else {
            self[gicr_id as usize].ICENABLER0.set(mask);
        }

        drop(lock);
    }

    pub fn get_enable(&self, _int_id: usize, gicr_id: u32) -> u32 {
        self[gicr_id as usize].ISENABLER0.get()
    }

    pub fn get_typer(&self, gicr_id: usize) -> u64 {
        self[gicr_id].TYPER.get()
    }

    pub fn get_propbaser(&self, gicr_id: usize) -> u64 {
        self[gicr_id].PROPBASER.get()
    }

    pub fn set_propbaser(&self, gicr_id: usize, val: usize) {
        self[gicr_id].PROPBASER.set(val as u64);
    }

    pub fn get_pendbaser(&self, gicr_id: usize) -> u64 {
        self[gicr_id].PEDNBASER.get()
    }

    pub fn set_pendbaser(&self, gicr_id: usize, val: usize) {
        self[gicr_id].PEDNBASER.set(val as u64);
    }

    pub fn set_ctrlr(&self, gicr_id: usize, val: usize) {
        self[gicr_id].CTLR.set(val as u32);
    }

    pub fn get_iidr(&self, gicr_id: usize) -> u32 {
        self[gicr_id].IIDR.get()
    }

    pub fn get_id(&self, gicr_id: u32, index: usize) -> u32 {
        self[gicr_id as usize].ID[index].get()
    }

    pub fn get_ctrl(&self, gicr_id: u32) -> u32 {
        self[gicr_id as usize].CTLR.get()
    }

    pub fn is_enabler(&self, gicr_id: u32) -> u32 {
        self[gicr_id as usize].ISENABLER0.get()
    }

    pub fn get_igroup(&self, gicr_id: u32) -> u32 {
        self[gicr_id as usize].IGROUPR0.get()
    }

    pub fn priority(&self, gicr_id: usize, index: usize) -> u32 {
        self[gicr_id].IPRIORITYR[index].get()
    }
}
pub struct GicCpuInterface;

impl core::fmt::Display for GicCpuInterface {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "ICC_SRE_EL2:{:016x}", mrs!(ICC_SRE_EL2, "x"))?;
        writeln!(f, "ICC_PMR_EL1:{:016x}", mrs!(ICC_PMR_EL1, "x"))?;
        writeln!(f, "ICC_BPR1_EL1:{:016x}", mrs!(ICC_BPR1_EL1, "x"))?;
        writeln!(f, "ICC_CTLR_EL1:{:016x}", mrs!(ICC_CTLR_EL1, "x"))?;
        writeln!(f, "ICH_HCR_EL2:{:016x}", mrs!(ICH_HCR_EL2, "x"))?;
        writeln!(f, "ICC_IGRPEN1_EL1:{:016x}", mrs!(ICC_IGRPEN1_EL1, "x"))?;
        Ok(())
    }
}

impl GicCpuInterface {
    fn init(&self) {
        msr!(ICC_SRE_EL2, 0b1, "x");

        unsafe {
            core::arch::asm!("isb");
        }

        for i in 0..gich_lrs_num() {
            GICH.set_lr(i, 0);
        }

        let pmr = mrs!(ICC_PMR_EL1, "x");
        msr!(ICC_PMR_EL1, 0xff, "x");
        msr!(ICC_BPR1_EL1, 0x0, "x");
        msr!(ICC_CTLR_EL1, ICC_CTLR_EOIMODE_BIT, "x");
        let hcr = mrs!(ICH_HCR_EL2) as usize;
        msr!(ICH_HCR_EL2, hcr | GICH_HCR_LRENPIE_BIT);
        msr!(ICC_IGRPEN1_EL1, GICC_IGRPEN_EL1_ENB_BIT, "x");

        //set ICH_VMCR_EL2:Interrupt Controller Virtual Machine Control Register Enables the hypervisor to save and restore the virtual machine view of the GIC state.
        let mut ich_vmcr = (pmr & GICH_PMR_MASK as u32) << GICH_VMCR_VPMR_SHIFT as u32;
        ich_vmcr |= GICH_VMCR_VENG1 as u32 | GICH_VMCR_VEOIM as u32;
        msr!(ICH_VMCR_EL2, ich_vmcr, "x");
    }

    pub fn iar(&self) -> u32 {
        let mut iarc: u32;
        mrs!(iarc, ICC_IAR1_EL1, "x");
        iarc
    }

    pub fn set_eoir(&self, eoir: u32) {
        msr!(ICC_EOIR1_EL1, eoir, "x");
    }

    pub fn set_dir(&self, dir: u32) {
        msr!(ICC_DIR_EL1, dir, "x");
    }
}

pub struct GicHypervisorInterface;

impl GicHypervisorInterface {
    pub fn hcr(&self) -> usize {
        let hcrc: usize;
        mrs!(hcrc, ICH_HCR_EL2);
        hcrc
    }

    pub fn set_hcr(&self, hcr: usize) {
        msr!(ICH_HCR_EL2, hcr)
    }

    // These registers can be used to locate a usable List register when the hypervisor is delivering an interrupt to a Guest OS.
    pub fn elrsr(&self) -> usize {
        let elrsrc: usize;
        mrs!(elrsrc, ICH_ELRSR_EL2);
        elrsrc
    }

    pub fn eisr(&self) -> u32 {
        let eisrc: u32;
        mrs!(eisrc, ICH_EISR_EL2, "x");
        eisrc
    }

    pub fn lr(&self, lr_idx: usize) -> usize {
        let lrc: usize;
        match lr_idx {
            0 => mrs!(lrc, ICH_LR0_EL2),
            1 => mrs!(lrc, ICH_LR1_EL2),
            2 => mrs!(lrc, ICH_LR2_EL2),
            3 => mrs!(lrc, ICH_LR3_EL2),
            4 => mrs!(lrc, ICH_LR4_EL2),
            5 => mrs!(lrc, ICH_LR5_EL2),
            6 => mrs!(lrc, ICH_LR6_EL2),
            7 => mrs!(lrc, ICH_LR7_EL2),
            8 => mrs!(lrc, ICH_LR8_EL2),
            9 => mrs!(lrc, ICH_LR9_EL2),
            10 => mrs!(lrc, ICH_LR10_EL2),
            11 => mrs!(lrc, ICH_LR11_EL2),
            12 => mrs!(lrc, ICH_LR12_EL2),
            13 => mrs!(lrc, ICH_LR13_EL2),
            14 => mrs!(lrc, ICH_LR14_EL2),
            15 => mrs!(lrc, ICH_LR15_EL2),
            _ => lrc = 0,
        };
        lrc
    }

    // Indicates which maintenance interrupts are asserted.
    pub fn misr(&self) -> u32 {
        let misrc: u32;
        mrs!(misrc, ICH_MISR_EL2, "x");
        misrc
    }

    pub fn set_lr(&self, lr_idx: usize, val: usize) {
        match lr_idx {
            0 => msr!(ICH_LR0_EL2, val),
            1 => msr!(ICH_LR1_EL2, val),
            2 => msr!(ICH_LR2_EL2, val),
            3 => msr!(ICH_LR3_EL2, val),
            4 => msr!(ICH_LR4_EL2, val),
            5 => msr!(ICH_LR5_EL2, val),
            6 => msr!(ICH_LR6_EL2, val),
            7 => msr!(ICH_LR7_EL2, val),
            8 => msr!(ICH_LR8_EL2, val),
            9 => msr!(ICH_LR9_EL2, val),
            10 => msr!(ICH_LR10_EL2, val),
            11 => msr!(ICH_LR11_EL2, val),
            12 => msr!(ICH_LR12_EL2, val),
            13 => msr!(ICH_LR13_EL2, val),
            14 => msr!(ICH_LR14_EL2, val),
            15 => msr!(ICH_LR15_EL2, val),
            _ => panic!("gic: trying to write inexistent list register"),
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct GicState {
    pub ctlr: u32,
    pub pmr: u32,
    pub bpr: u32,
    pub eoir: u32,
    pub rpr: u32,
    pub hppir: u32,
    pub priv_isenabler: u32,
    pub priv_ipriorityr: [u32; GIC_PRIVINT_NUM / 4],
    pub hcr: usize,
    pub lr: [usize; GIC_LIST_REGS_NUM],
    pub apr0: [u32; 4],
    pub apr1: [u32; 4],
    igrpen1: usize,
    vmcr: u32,
    nr_prio: u32, //Priority bits. The number of virtual priority bits implemented, minus one.
    sre_el1: u32,
}

impl Default for GicState {
    fn default() -> Self {
        let nr_prio = (((mrs!(ICH_VTR_EL2) >> GICH_VTR_PRIBITS_OFF) & ((1 << GICH_VTR_PRIBITS_LEN) - 1)) + 1) as u32;
        let r: GicState = GicState {
            ctlr: GICC_CTLR_EOIMODE_BIT as u32,
            igrpen1: GICC_IGRPEN_EL1_ENB_BIT,
            pmr: 0xff,
            bpr: 0,
            eoir: 0,
            rpr: 0,
            hppir: 0,
            priv_isenabler: GICR[current_cpu().id].ISENABLER0.get(),
            priv_ipriorityr: [u32::MAX; GIC_PRIVINT_NUM / 4],
            hcr: 0b101,
            lr: [0; GIC_LIST_REGS_NUM],
            vmcr: 0,
            nr_prio,
            apr0: [0; 4],
            apr1: [0; 4],
            sre_el1: 0,
        };
        r
    }
}

impl crate::arch::InterruptContextTriat for GicState {
    fn save_state(&mut self) {
        mrs!(self.hcr, ICH_HCR_EL2, "x");
        // save VMCR_EL2: save and restore the virtual machine view of the GIC state.
        mrs!(self.vmcr, ICH_VMCR_EL2, "x");
        // save ICH_AP1Rn_EL2: Provides information about Group 1 virtual active priorities for EL2.
        // if some bit set 1:There is a Group 1 interrupt active with this priority level which has not undergone priority drop.
        self.save_aprn_regs();
        // save lr
        for i in 0..gich_lrs_num() {
            self.lr[i] = GICH.lr(i);
        }
        // save ICC_SRE_EL1: EL1`s systregister use
        mrs!(self.sre_el1, ICC_SRE_EL1, "x");
        let icc_sre_el2_enable = mrs!(ICC_SRE_EL2, "x");
        msr!(ICC_SRE_EL2, icc_sre_el2_enable & !GICC_SRE_EL2_ENABLE as u32, "x");
    }

    fn restore_state(&self) {
        // make EL2 can use sysrem register
        msr!(ICC_SRE_EL2, 0b1001, "x");
        // restore ICC_SRE_EL1 for EL1
        msr!(ICC_SRE_EL1, 0x1, "x");
        unsafe {
            core::arch::asm!("isb");
        }
        // restore HCR
        msr!(ICH_HCR_EL2, self.hcr, "x");
        // restore ICH_VMCR_EL2
        msr!(ICH_VMCR_EL2, self.vmcr, "x");
        // restore aprn
        self.restore_aprn_regs();
        // restore lr
        for i in 0..gich_lrs_num() {
            GICH.set_lr(i, self.lr[i]);
        }
    }
}

impl GicState {
    fn save_apr2(&mut self) {
        mrs!(self.apr0[2], ICH_AP0R2_EL2, "x");
        mrs!(self.apr1[2], ICH_AP1R2_EL2, "x");
    }

    fn save_apr1(&mut self) {
        mrs!(self.apr0[1], ICH_AP0R1_EL2, "x");
        mrs!(self.apr1[1], ICH_AP1R1_EL2, "x");
    }

    fn save_apr0(&mut self) {
        mrs!(self.apr0[0], ICH_AP0R0_EL2, "x");
        mrs!(self.apr1[0], ICH_AP1R0_EL2, "x");
    }

    fn save_aprn_regs(&mut self) {
        match self.nr_prio {
            7 => {
                self.save_apr2();
                self.save_apr1();
                self.save_apr0();
            }
            6 => {
                self.save_apr1();
                self.save_apr0();
            }
            5 => {
                self.save_apr0();
            }
            _ => panic!("priority not surpport"),
        }
    }

    fn restore_aprn_regs(&self) {
        let restore_apr2 = || {
            msr!(ICH_AP0R2_EL2, self.apr0[2], "x");
            msr!(ICH_AP1R2_EL2, self.apr1[2], "x");
        };
        let restore_apr1 = || {
            msr!(ICH_AP0R1_EL2, self.apr0[1], "x");
            msr!(ICH_AP1R1_EL2, self.apr1[1], "x");
        };
        let restore_apr0 = || {
            msr!(ICH_AP0R0_EL2, self.apr0[0], "x");
            msr!(ICH_AP1R0_EL2, self.apr1[0], "x");
        };
        match self.nr_prio {
            7 => {
                restore_apr2();
                restore_apr1();
                restore_apr0();
            }
            6 => {
                restore_apr1();
                restore_apr0();
            }
            5 => {
                restore_apr0();
            }
            _ => panic!("priority not surpport"),
        }
    }
}

pub static GICD: GicDistributor = GicDistributor::new(Platform::GICD_BASE);
pub static GICC: GicCpuInterface = GicCpuInterface;
pub static GICH: GicHypervisorInterface = GicHypervisorInterface;
pub static GICR: GicRedistributor = GicRedistributor::new(Platform::GICR_BASE);

#[inline(always)]
pub fn gich_lrs_num() -> usize {
    let mut vtr: u32;
    mrs!(vtr, ICH_VTR_EL2, "x");
    ((vtr & GICH_VTR_MSK as u32) + 1) as usize
}

#[inline(always)]
pub fn gic_max_spi() -> usize {
    let typer = GICD.TYPER.get();
    let value = typer & GICD_TYPER_ITLINESNUM_LEN as u32;
    (32 * (value + 1)) as usize
}

pub fn gic_glb_init() {
    set_gic_lrs(gich_lrs_num());
    GICD.global_init();
}

pub fn gic_cpu_init() {
    GICR.init();
    GICC.init();
}

pub fn gic_cpu_reset() {
    GICC.init();
}

#[inline(always)]
pub fn gic_is_priv(int_id: usize) -> bool {
    int_id < GIC_PRIVINT_NUM
}

#[inline(always)]
pub fn gic_is_sgi(int_id: usize) -> bool {
    int_id < GIC_SGIS_NUM
}

pub fn gicc_clear_current_irq(for_hypervisor: bool) {
    let irq = current_cpu().current_irq as u32;
    if irq == 0 {
        return;
    }
    GICC.set_eoir(irq);
    if for_hypervisor {
        GICC.set_dir(irq);
    }
    current_cpu().current_irq = 0;
}

pub fn gicc_get_current_irq() -> (usize, usize) {
    let iar = GICC.iar();
    let irq = iar as usize;
    current_cpu().current_irq = irq;
    let id = bit_extract(iar as usize, GICC_IAR_ID_OFF, GICC_IAR_ID_LEN);
    let src = bit_extract(iar as usize, 10, 3);
    (id, src)
}

pub fn gic_lrs() -> usize {
    *GIC_LRS_NUM.lock()
}

pub fn set_gic_lrs(lrs: usize) {
    let mut gic_lrs = GIC_LRS_NUM.lock();
    *gic_lrs = lrs;
}

pub fn gic_set_icfgr(int_id: usize, cfg: u8) {
    if !gic_is_priv(int_id) {
        GICD.set_icfgr(int_id, cfg);
    } else {
        GICR.set_icfgr(int_id, cfg, current_cpu().id as u32);
    }
}

pub fn gic_set_state(int_id: usize, state: usize, gicr_id: u32) {
    gic_set_act(int_id, (state & IrqState::IrqSActive as usize) != 0, gicr_id);
    gic_set_pend(int_id, (state & IrqState::IrqSPend as usize) != 0, gicr_id);
}

pub fn gic_set_act(int_id: usize, act: bool, gicr_id: u32) {
    if !gic_is_priv(int_id) {
        GICD.set_act(int_id, act);
    } else {
        GICR.set_act(int_id, act, gicr_id);
    }
}

pub fn gic_set_pend(int_id: usize, pend: bool, gicr_id: u32) {
    if !gic_is_priv(int_id) {
        GICD.set_pend(int_id, pend);
    } else {
        GICR.set_pend(int_id, pend, gicr_id);
    }
}

pub fn gic_get_pend(int_id: usize) -> bool {
    if !gic_is_priv(int_id) {
        GICD.get_pend(int_id)
    } else {
        GICR.get_pend(int_id, current_cpu().id as u32)
    }
}

pub fn gic_get_act(int_id: usize) -> bool {
    if !gic_is_priv(int_id) {
        GICD.get_act(int_id)
    } else {
        GICR.get_act(int_id, current_cpu().id as u32)
    }
}

pub fn gic_set_enable(int_id: usize, en: bool) {
    if !gic_is_priv(int_id) {
        GICD.set_enable(int_id, en);
    } else {
        GICR.set_enable(int_id, en, current_cpu().id as u32);
    }
}

pub fn gic_get_prio(int_id: usize) {
    if !gic_is_priv(int_id) {
        GICD.prio(int_id);
    } else {
        GICR.get_prio(int_id, current_cpu().id as u32);
    }
}

pub fn gic_set_prio(int_id: usize, prio: u8) {
    if !gic_is_priv(int_id) {
        GICD.set_prio(int_id, prio);
    } else {
        GICR.set_prio(int_id, prio, current_cpu().id as u32);
    }
}
