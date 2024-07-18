// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.
use core::ptr::NonNull;
use core::cell::{Cell, RefCell};
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use core::ops::Range;

use spin::Mutex;

use crate::{arch::GICH, kernel::IpiInitcMessage};
use crate::config::VmEmulatedDeviceConfig;
use crate::board::{Platform, PlatOperation};
use crate::device::EmuContext;
use crate::device::{EmuDev, EmuDeviceType};
use crate::kernel::{active_vcpu_id, current_cpu, restore_vcpu_gic, save_vcpu_gic};
use crate::kernel::{active_vm, active_vm_id, active_vm_ncpu};
use crate::kernel::{ipi_intra_broadcast_msg, ipi_send_msg, IpiInnerMsg, IpiMessage, IpiType};
use crate::kernel::{InitcEvent, Vcpu, Vm};
use crate::utils::{bit_extract, bit_get, bit_set, bitmap_find_nth};

use super::gic::*;

// GICv2 interrupt structure
struct VgicInt {
    inner_const: VgicIntInnerConst,
    inner: Mutex<VgicIntInnerMut>,
    pub lock: Mutex<()>,
}

struct VgicIntInnerConst {
    id: u16,
    hw: Cell<bool>,
}

// SAFETY: VgicIntInnerConst hw is only set when initializing
unsafe impl Sync for VgicIntInnerConst {}

impl VgicInt {
    fn new(id: usize) -> Self {
        Self {
            inner_const: VgicIntInnerConst {
                id: (id + GIC_PRIVINT_NUM) as u16,
                hw: Cell::new(false),
            },
            inner: Mutex::new(VgicIntInnerMut::new()),
            lock: Mutex::new(()),
        }
    }

    fn priv_new(id: usize, owner: Vcpu, targets: usize, enabled: bool) -> Self {
        Self {
            inner_const: VgicIntInnerConst {
                id: id as u16,
                hw: Cell::new(false),
            },
            inner: Mutex::new(VgicIntInnerMut::priv_new(owner, targets, enabled)),
            lock: Mutex::new(()),
        }
    }

    fn set_in_pend_state(&self, is_pend: bool) {
        let mut vgic_int = self.inner.lock();
        vgic_int.in_pend = is_pend;
    }

    fn set_in_act_state(&self, is_act: bool) {
        let mut vgic_int = self.inner.lock();
        vgic_int.in_act = is_act;
    }

    pub fn in_pend(&self) -> bool {
        let vgic_int = self.inner.lock();
        vgic_int.in_pend
    }

    pub fn in_act(&self) -> bool {
        let vgic_int = self.inner.lock();
        vgic_int.in_act
    }

    fn set_enabled(&self, enabled: bool) {
        let mut vgic_int = self.inner.lock();
        vgic_int.enabled = enabled;
    }

    fn set_lr(&self, lr: u16) {
        let mut vgic_int = self.inner.lock();
        vgic_int.lr = lr;
    }

    fn set_targets(&self, targets: u8) {
        let mut vgic_int = self.inner.lock();
        vgic_int.targets = targets;
    }

    fn set_prio(&self, prio: u8) {
        let mut vgic_int = self.inner.lock();
        vgic_int.prio = prio;
    }

    fn set_in_lr(&self, in_lr: bool) {
        let mut vgic_int = self.inner.lock();
        vgic_int.in_lr = in_lr;
    }

    fn set_state(&self, state: IrqState) {
        let mut vgic_int = self.inner.lock();
        vgic_int.state = state;
    }

    fn set_owner(&self, owner: Vcpu) {
        let mut vgic_int = self.inner.lock();
        vgic_int.owner = Some(owner);
    }

    fn clear_owner(&self) {
        let mut vgic_int = self.inner.lock();
        // println!("clear owner get lock");
        vgic_int.owner = None;
    }

    fn set_hw(&self, hw: bool) {
        self.inner_const.hw.set(hw);
    }

    fn set_cfg(&self, cfg: u8) {
        let mut vgic_int = self.inner.lock();
        vgic_int.cfg = cfg;
    }

    fn lr(&self) -> u16 {
        let vgic_int = self.inner.lock();
        vgic_int.lr
    }

    fn in_lr(&self) -> bool {
        let vgic_int = self.inner.lock();
        vgic_int.in_lr
    }

    #[inline]
    fn id(&self) -> u16 {
        self.inner_const.id
    }

    fn enabled(&self) -> bool {
        let vgic_int = self.inner.lock();
        vgic_int.enabled
    }

    fn prio(&self) -> u8 {
        let vgic_int = self.inner.lock();
        vgic_int.prio
    }

    fn targets(&self) -> u8 {
        let vgic_int = self.inner.lock();
        vgic_int.targets
    }

    #[inline]
    fn hw(&self) -> bool {
        self.inner_const.hw.get()
    }

    pub fn state(&self) -> IrqState {
        let vgic_int = self.inner.lock();
        vgic_int.state
    }

    fn cfg(&self) -> u8 {
        let vgic_int = self.inner.lock();
        vgic_int.cfg
    }

    fn owner(&self) -> Option<Vcpu> {
        let vgic_int = self.inner.lock();
        vgic_int.owner.as_ref().cloned()
    }

    fn owner_phys_id(&self) -> Option<usize> {
        let vgic_int = self.inner.lock();
        vgic_int.owner.as_ref().map(|owner| owner.phys_id())
    }

    fn owner_id(&self) -> Option<usize> {
        let vgic_int = self.inner.lock();
        match &vgic_int.owner {
            Some(owner) => Some(owner.id()),
            None => {
                warn!("owner_id is None");
                None
            }
        }
    }

    fn owner_vm_id(&self) -> Option<usize> {
        let vgic_int = self.inner.lock();
        vgic_int.owner.as_ref().map(|owner| owner.vm_id())
    }

    fn owner_vm(&self) -> Arc<Vm> {
        let vgic_int = self.inner.lock();
        vgic_int.owner_vm()
    }

    fn locked_helper<F>(&self, f: F)
    where
        F: FnOnce(&mut VgicIntInnerMut),
    {
        f(&mut self.inner.lock());
    }
}

struct VgicIntInnerMut {
    owner: Option<Vcpu>,
    in_lr: bool,
    lr: u16,
    enabled: bool,
    state: IrqState,
    prio: u8,
    targets: u8,
    cfg: u8,

    in_pend: bool,
    in_act: bool,
}

impl VgicIntInnerMut {
    fn new() -> Self {
        Self {
            owner: None,
            in_lr: false,
            lr: 0,
            enabled: false,
            state: IrqState::IrqSInactive,
            prio: 0xff,
            targets: 0,
            cfg: 0,
            in_pend: false,
            in_act: false,
        }
    }

    fn priv_new(owner: Vcpu, targets: usize, enabled: bool) -> Self {
        Self {
            owner: Some(owner),
            in_lr: false,
            lr: 0,
            enabled,
            state: IrqState::IrqSInactive,
            prio: 0xff,
            targets: targets as u8,
            cfg: 0,
            in_pend: false,
            in_act: false,
        }
    }

    fn owner_vm(&self) -> Arc<Vm> {
        let owner = self.owner.as_ref().unwrap();
        owner.vm().unwrap()
    }
}

struct Vgicd {
    // ctlr will be written among different cores, so we use AtomicU32 to guarantee thread safety
    ctlr: AtomicU32,
    // Others will be read only and only be written when initializings
    typer: u32,
    iidr: u32,
    interrupts: Vec<VgicInt>,
}

impl Vgicd {
    fn new(cpu_num: usize) -> Vgicd {
        Vgicd {
            ctlr: AtomicU32::new(0b10),
            typer: (GICD.typer() & GICD_TYPER_CPUNUM_MSK as u32)
                | (((cpu_num - 1) << GICD_TYPER_CPUNUM_OFF) & GICD_TYPER_CPUNUM_MSK) as u32,
            iidr: GICD.iidr(),
            interrupts: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct Sgis {
    pub pend: u8,
    pub act: u8,
}

struct VgicCpuPriv {
    interrupts: Vec<VgicInt>,
    inner_mut: RefCell<VgicCpuPrivMut>,
}

struct VgicCpuPrivMut {
    curr_lrs: [u16; GIC_LIST_REGS_NUM],
    sgis: [Sgis; GIC_SGIS_NUM],

    pend_list: VecDeque<NonNull<VgicInt>>,
    act_list: VecDeque<NonNull<VgicInt>>,
}

impl VgicCpuPrivMut {
    fn queue_remove(list: &mut VecDeque<NonNull<VgicInt>>, interrupt: &VgicInt) {
        /// SAFETY: All VgicInt are allocated when initializing, so it's safe to convert them to NonNull
        list.iter()
            .position(|x| unsafe { x.as_ref().id() } == interrupt.id())
            .map(|i| list.remove(i));
    }

    fn pend_list_push(&mut self, interrupt: &VgicInt) {
        /// SAFETY: All VgicInt are allocated when initializing, so it's safe to convert them to NonNull
        self.pend_list
            .push_back(unsafe { NonNull::new_unchecked(interrupt as *const _ as *mut _) });
    }

    fn pend_list_remove(&mut self, interrupt: &VgicInt) {
        Self::queue_remove(&mut self.pend_list, interrupt);
    }

    fn act_list_push(&mut self, interrupt: &VgicInt) {
        /// SAFETY: All VgicInt are allocated when initializing, so it's safe to convert them to NonNull
        self.act_list
            .push_back(unsafe { NonNull::new_unchecked(interrupt as *const _ as *mut _) });
    }

    fn act_list_remove(&mut self, interrupt: &VgicInt) {
        Self::queue_remove(&mut self.act_list, interrupt);
    }
}

// SAFETY: VgicCpuPriv is only accessed on one core
unsafe impl Send for VgicCpuPriv {}
unsafe impl Sync for VgicCpuPriv {}

impl VgicCpuPriv {
    fn default() -> VgicCpuPriv {
        VgicCpuPriv {
            interrupts: Vec::new(),
            inner_mut: RefCell::new(VgicCpuPrivMut {
                curr_lrs: [0; GIC_LIST_REGS_NUM],
                sgis: [Sgis::default(); GIC_SGIS_NUM],
                pend_list: VecDeque::new(),
                act_list: VecDeque::new(),
            }),
        }
    }
}

pub struct Vgic {
    address_range: Range<usize>,
    vgicd: Vgicd,
    cpu_priv: Vec<VgicCpuPriv>,
}

impl Vgic {
    pub fn new(base: usize, length: usize, cpu_num: usize) -> Vgic {
        Vgic {
            address_range: base..base + length,
            vgicd: Vgicd::new(cpu_num),
            cpu_priv: Vec::new(),
        }
    }

    fn update_int_list(&self, vcpu: &Vcpu, interrupt: &VgicInt) {
        // Every vcpu has its own cpu_priv, so we can use vcpu.id() to index cpu_priv
        let mut cpu_priv = self.cpu_priv[vcpu.id()].inner_mut.borrow_mut();

        interrupt.locked_helper(|int| {
            let state = int.state.to_num();

            if state & IrqState::IrqSPend.to_num() != 0 && !int.in_pend {
                cpu_priv.pend_list_push(interrupt);
                int.in_pend = true;
            } else if state & IrqState::IrqSPend.to_num() == 0 {
                cpu_priv.pend_list_remove(interrupt);
                int.in_pend = false;
            }

            if state & IrqState::IrqSActive.to_num() != 0 && !int.in_act {
                cpu_priv.act_list_push(interrupt);
                int.in_act = true;
            } else if state & IrqState::IrqSActive.to_num() == 0 {
                cpu_priv.act_list_remove(interrupt);
                int.in_act = false;
            }
        });
    }

    fn int_list_head(&self, vcpu: &Vcpu, is_pend: bool) -> Option<&VgicInt> {
        let vcpu_id = vcpu.id();
        let cpu_priv = self.cpu_priv[vcpu_id].inner_mut.borrow();
        if is_pend {
            /// SAFETY: All VgicInt are allocated when initializing, so it's safe to convert them to NonNull
            cpu_priv.pend_list.front().cloned().map(|x| unsafe { x.as_ref() })
        } else {
            /// SAFETY: All VgicInt are allocated when initializing, so it's safe to convert them to NonNull
            cpu_priv.act_list.front().cloned().map(|x| unsafe { x.as_ref() })
        }
    }

    fn set_vgicd_ctlr(&self, ctlr: u32) {
        self.vgicd.ctlr.store(ctlr, Ordering::Relaxed);
    }

    pub fn vgicd_ctlr(&self) -> u32 {
        self.vgicd.ctlr.load(Ordering::Relaxed)
    }

    pub fn vgicd_typer(&self) -> u32 {
        self.vgicd.typer
    }

    pub fn vgicd_iidr(&self) -> u32 {
        self.vgicd.iidr
    }

    fn cpu_priv_interrupt(&self, cpu_id: usize, idx: usize) -> Option<&VgicInt> {
        self.cpu_priv[cpu_id].interrupts.get(idx)
    }

    fn cpu_priv_curr_lrs(&self, cpu_id: usize, idx: usize) -> u16 {
        let cpu_priv = self.cpu_priv[cpu_id].inner_mut.borrow();
        cpu_priv.curr_lrs[idx]
    }

    fn cpu_priv_sgis_pend(&self, cpu_id: usize, idx: usize) -> u8 {
        let cpu_priv = self.cpu_priv[cpu_id].inner_mut.borrow();
        cpu_priv.sgis[idx].pend
    }

    fn cpu_priv_sgis_act(&self, cpu_id: usize, idx: usize) -> u8 {
        let cpu_priv = self.cpu_priv[cpu_id].inner_mut.borrow();
        cpu_priv.sgis[idx].act
    }

    fn set_cpu_priv_curr_lrs(&self, cpu_id: usize, idx: usize, val: u16) {
        let mut cpu_priv = self.cpu_priv[cpu_id].inner_mut.borrow_mut();
        cpu_priv.curr_lrs[idx] = val;
    }

    fn set_cpu_priv_sgis_pend(&self, cpu_id: usize, idx: usize, pend: u8) {
        let mut cpu_priv = self.cpu_priv[cpu_id].inner_mut.borrow_mut();
        cpu_priv.sgis[idx].pend = pend;
    }

    fn set_cpu_priv_sgis_act(&self, cpu_id: usize, idx: usize, act: u8) {
        let mut cpu_priv = self.cpu_priv[cpu_id].inner_mut.borrow_mut();
        cpu_priv.sgis[idx].act = act;
    }

    fn vgicd_interrupt(&self, idx: usize) -> Option<&VgicInt> {
        self.vgicd.interrupts.get(idx)
    }

    fn get_int(&self, vcpu: &Vcpu, int_id: usize) -> Option<&VgicInt> {
        if int_id < GIC_PRIVINT_NUM {
            let vcpu_id = vcpu.id();
            self.cpu_priv_interrupt(vcpu_id, int_id)
        } else if (GIC_PRIVINT_NUM..GIC_INTS_MAX).contains(&int_id) {
            self.vgicd_interrupt(int_id - GIC_PRIVINT_NUM)
        } else {
            None
        }
    }

    fn remove_lr(&self, vcpu: &Vcpu, interrupt: &VgicInt) -> bool {
        if !vgic_owns(vcpu, interrupt) {
            return false;
        }
        let int_lr = interrupt.lr();
        let int_id = interrupt.id() as usize;
        let vcpu_id = vcpu.id();

        if !interrupt.in_lr() {
            return false;
        }

        let mut lr_val = 0;
        if let Some(lr) = gich_get_lr(interrupt) {
            GICH.set_lr(int_lr as usize, 0);
            lr_val = lr;
        }

        interrupt.set_in_lr(false);

        let lr_state = (lr_val >> 28) & 0b11;
        if lr_state != 0 {
            interrupt.set_state(IrqState::num_to_state(lr_state as usize));
            if int_id < GIC_SGIS_NUM {
                if interrupt.state().to_num() & 2 != 0 {
                    self.set_cpu_priv_sgis_act(vcpu_id, int_id, ((lr_val >> 10) & 0b111) as u8);
                } else if interrupt.state().to_num() & 1 != 0 {
                    let pend = self.cpu_priv_sgis_pend(vcpu_id, int_id);
                    self.set_cpu_priv_sgis_pend(vcpu_id, int_id, pend | (1 << ((lr_val >> 10) & 0b111) as u8));
                }
            }

            self.update_int_list(vcpu, interrupt);

            if (interrupt.state().to_num() & 1 != 0) && interrupt.enabled() {
                let hcr = GICH.hcr();
                /*
                NPIE, bit [3]
                No Pending Interrupt Enable. Enables the signaling of a maintenance interrupt while no pending interrupts are present in the List registers:

                NPIE	Meaning
                0
                Maintenance interrupt disabled.

                1
                Maintenance interrupt signaled while the List registers contain no interrupts in the pending state.

                When this register has an architecturally-defined reset value, this field resets to 0.
                                 */
                GICH.set_hcr(hcr | (1 << 3));
            }
            return true;
        }
        false
    }

    fn add_lr(&self, vcpu: &Vcpu, interrupt: &VgicInt) -> bool {
        if !interrupt.enabled() || interrupt.in_lr() {
            return false;
        }

        let gic_lrs = gic_lrs();
        let mut lr_ind = None;

        for i in 0..gic_lrs {
            if (GICH.elrsr(i / 32) & (1 << (i % 32))) != 0 {
                lr_ind = Some(i);
                break;
            }
        }

        if lr_ind.is_none() {
            let mut pend_found = 0;
            let mut act_found = 0;
            let mut min_prio_act = 0;
            let mut min_prio_pend = 0;
            let mut act_ind = None;
            let mut pend_ind = None;

            for i in 0..gic_lrs {
                let lr = GICH.lr(i);
                let lr_prio = (lr >> 23) & 0b11111;
                let lr_state = (lr >> 28) & 0b11;

                if lr_state & 2 != 0 {
                    if lr_prio > min_prio_act {
                        min_prio_act = lr_prio;
                        act_ind = Some(i);
                    }
                    act_found += 1;
                } else if lr_state & 1 != 0 {
                    if lr_prio > min_prio_pend {
                        min_prio_pend = lr_prio;
                        pend_ind = Some(i);
                    }
                    pend_found += 1;
                }
            }

            if pend_found > 1 {
                lr_ind = pend_ind;
            } else if act_found > 1 {
                lr_ind = act_ind;
            }

            if let Some(idx) = lr_ind {
                let spilled_int = self.get_int(vcpu, GICH.lr(idx) as usize & 0b1111111111).unwrap();
                if spilled_int.id() != interrupt.id() {
                    let spilled_int_lock = spilled_int.lock.lock();
                    self.remove_lr(vcpu, spilled_int);
                    vgic_int_yield_owner(vcpu, spilled_int);
                    drop(spilled_int_lock);
                } else {
                    self.remove_lr(vcpu, spilled_int);
                    vgic_int_yield_owner(vcpu, spilled_int);
                }
            }
        }

        match lr_ind {
            Some(idx) => {
                self.write_lr(vcpu, interrupt, idx);
                return true;
            }
            None => {
                // turn on maintenance interrupts
                if vgic_get_state(interrupt) & 1 != 0 {
                    let hcr = GICH.hcr();
                    GICH.set_hcr(hcr | (1 << 3));
                }
            }
        }

        false
    }

    fn write_lr(&self, vcpu: &Vcpu, interrupt: &VgicInt, lr_ind: usize) {
        let vcpu_id = vcpu.id();
        let int_id = interrupt.id() as usize;
        let int_prio = interrupt.prio();

        let prev_int_id = self.cpu_priv_curr_lrs(vcpu_id, lr_ind) as usize;
        if prev_int_id != int_id {
            if let Some(prev_interrupt) = self.get_int(vcpu, prev_int_id) {
                let prev_interrupt_lock = prev_interrupt.lock.lock();
                if vgic_owns(vcpu, prev_interrupt) && prev_interrupt.in_lr() && prev_interrupt.lr() == lr_ind as u16 {
                    prev_interrupt.set_in_lr(false);
                    let prev_id = prev_interrupt.id() as usize;
                    if !gic_is_priv(prev_id) {
                        vgic_int_yield_owner(vcpu, prev_interrupt);
                    }
                }
                drop(prev_interrupt_lock);
            }
        }

        let state = vgic_get_state(interrupt);
        let mut lr = (int_id & 0b1111111111) | (((int_prio as usize >> 3) & 0b11111) << 23);

        if vgic_int_is_hw(interrupt) {
            lr |= 1 << 31;
            lr |= (0b1111111111 & int_id) << 10;
            if state == 3 {
                lr |= (2 & 0b11) << 28;
            } else {
                lr |= (state & 0b11) << 28;
            }
            if GICD.state(int_id) != 2 {
                GICD.set_state(int_id, 2);
            }
        } else if int_id < GIC_SGIS_NUM {
            if (state & 2) != 0 {
                lr |= ((self.cpu_priv_sgis_act(vcpu_id, int_id) as usize) << 10) & (0b111 << 10);
                lr |= (2 & 0b11) << 28;
            } else {
                let mut idx = GIC_TARGETS_MAX - 1;
                while idx as isize >= 0 {
                    if (self.cpu_priv_sgis_pend(vcpu_id, int_id) & (1 << idx)) != 0 {
                        lr |= (idx & 0b111) << 10;
                        let pend = self.cpu_priv_sgis_pend(vcpu_id, int_id);
                        self.set_cpu_priv_sgis_pend(vcpu_id, int_id, pend & !(1 << idx));

                        lr |= (1 & 0b11) << 28;
                        break;
                    }
                    idx -= 1;
                }
            }

            if self.cpu_priv_sgis_pend(vcpu_id, int_id) != 0 {
                lr |= 1 << 19;
            }
        } else {
            if !gic_is_priv(int_id) && !vgic_int_is_hw(interrupt) {
                lr |= 1 << 19;
            }

            lr |= (state & 0b11) << 28;
        }

        interrupt.locked_helper(|int| {
            int.state = IrqState::IrqSInactive;
            int.in_lr = true;
            int.lr = lr_ind as u16;
        });
        self.set_cpu_priv_curr_lrs(vcpu_id, lr_ind, int_id as u16);
        GICH.set_lr(lr_ind, lr as u32);

        self.update_int_list(vcpu, interrupt);
    }

    fn route(&self, vcpu: &Vcpu, interrupt: &VgicInt) {
        let cpu_id = current_cpu().id;
        if let IrqState::IrqSInactive = interrupt.state() {
            return;
        }

        if !interrupt.enabled() {
            return;
        }

        let int_targets = interrupt.targets();
        if (int_targets & (1 << cpu_id)) != 0 {
            self.add_lr(vcpu, interrupt);
        }

        if !interrupt.in_lr() && (int_targets & !(1 << cpu_id)) != 0 {
            let vcpu_vm_id = vcpu.vm_id();

            let ipi_msg = IpiInitcMessage {
                event: InitcEvent::VgicdRoute,
                vm_id: vcpu_vm_id,
                int_id: interrupt.id(),
                val: 0,
            };
            vgic_int_yield_owner(vcpu, interrupt);
            ipi_intra_broadcast_msg(&active_vm().unwrap(), IpiType::IpiTIntc, IpiInnerMsg::Initc(ipi_msg));
        }
    }

    fn set_enable(&self, vcpu: &Vcpu, int_id: usize, en: bool) {
        if int_id < GIC_SGIS_NUM {
            return;
        }
        match self.get_int(vcpu, int_id) {
            Some(interrupt) => {
                let interrupt_lock = interrupt.lock.lock();
                if vgic_int_get_owner(vcpu, interrupt) {
                    if interrupt.enabled() ^ en {
                        interrupt.set_enabled(en);
                        if !interrupt.enabled() {
                            self.remove_lr(vcpu, interrupt);
                        } else {
                            self.route(vcpu, interrupt);
                        }
                        if interrupt.hw() {
                            GICD.set_enable(interrupt.id() as usize, en);
                        }
                    }
                    vgic_int_yield_owner(vcpu, interrupt);
                } else {
                    let int_phys_id = interrupt.owner_phys_id().unwrap();
                    let vcpu_vm_id = vcpu.vm_id();
                    let ipi_msg = IpiInitcMessage {
                        event: InitcEvent::VgicdSetEn,
                        vm_id: vcpu_vm_id,
                        int_id: interrupt.id(),
                        val: en as u8,
                    };
                    if !ipi_send_msg(int_phys_id, IpiType::IpiTIntc, IpiInnerMsg::Initc(ipi_msg)) {
                        error!(
                            "vgicd_set_enable: Failed to send ipi message, target {} type {}",
                            int_phys_id, 0
                        );
                    }
                }
                drop(interrupt_lock);
            }
            None => {
                error!("vgicd_set_enable: interrupt {} is illegal", int_id);
            }
        }
    }

    fn get_enable(&self, vcpu: &Vcpu, int_id: usize) -> bool {
        self.get_int(vcpu, int_id).unwrap().enabled()
    }

    fn set_pend(&self, vcpu: &Vcpu, int_id: usize, pend: bool) {
        // TODO: sgi_get_pend ?
        if bit_extract(int_id, 0, 10) < GIC_SGIS_NUM {
            self.sgi_set_pend(vcpu, int_id, pend);
            return;
        }

        if let Some(interrupt) = self.get_int(vcpu, bit_extract(int_id, 0, 10)) {
            let interrupt_lock = interrupt.lock.lock();
            if vgic_int_get_owner(vcpu, interrupt) {
                self.remove_lr(vcpu, interrupt);

                let state = interrupt.state().to_num();
                if pend && ((state & 1) == 0) {
                    interrupt.set_state(IrqState::num_to_state(state | 1));
                } else if !pend && (state & 1) != 0 {
                    interrupt.set_state(IrqState::num_to_state(state & !1));
                }
                self.update_int_list(vcpu, interrupt);

                let state = interrupt.state().to_num();
                if interrupt.hw() {
                    let vgic_int_id = interrupt.id() as usize;
                    GICD.set_state(vgic_int_id, if state == 1 { 2 } else { state })
                }
                self.route(vcpu, interrupt);
                vgic_int_yield_owner(vcpu, interrupt);
                drop(interrupt_lock);
            } else {
                let vm_id = vcpu.vm_id();

                let m = IpiInitcMessage {
                    event: InitcEvent::VgicdSetPend,
                    vm_id,
                    int_id: interrupt.id(),
                    val: pend as u8,
                };
                match interrupt.owner() {
                    Some(owner) => {
                        let phys_id = owner.phys_id();

                        drop(interrupt_lock);
                        if !ipi_send_msg(phys_id, IpiType::IpiTIntc, IpiInnerMsg::Initc(m)) {
                            error!(
                                "vgicd_set_pend: Failed to send ipi message, target {} type {}",
                                phys_id, 0
                            );
                        }
                    }
                    None => {
                        panic!(
                            "set_pend: Core {} int {} has no owner",
                            current_cpu().id,
                            interrupt.id()
                        );
                    }
                }
            }
        }
    }

    fn set_active(&self, vcpu: &Vcpu, int_id: usize, act: bool) {
        if let Some(interrupt) = self.get_int(vcpu, bit_extract(int_id, 0, 10)) {
            let interrupt_lock = interrupt.lock.lock();
            if vgic_int_get_owner(vcpu, interrupt) {
                self.remove_lr(vcpu, interrupt);
                let state = interrupt.state().to_num();
                if act && ((state & 2) == 0) {
                    interrupt.set_state(IrqState::num_to_state(state | 2));
                } else if !act && (state & 2) != 0 {
                    interrupt.set_state(IrqState::num_to_state(state & !2));
                }
                self.update_int_list(vcpu, interrupt);

                let state = interrupt.state().to_num();
                if interrupt.hw() {
                    let vgic_int_id = interrupt.id() as usize;
                    GICD.set_state(vgic_int_id, if state == 1 { 2 } else { state })
                }
                self.route(vcpu, interrupt);
                vgic_int_yield_owner(vcpu, interrupt);
            } else {
                let vm_id = vcpu.vm_id();

                let m = IpiInitcMessage {
                    event: InitcEvent::VgicdSetPend,
                    vm_id,
                    int_id: interrupt.id(),
                    val: act as u8,
                };
                let phys_id = interrupt.owner_phys_id().unwrap();
                if !ipi_send_msg(phys_id, IpiType::IpiTIntc, IpiInnerMsg::Initc(m)) {
                    error!(
                        "vgicd_set_active: Failed to send ipi message, target {} type {}",
                        phys_id, 0
                    );
                }
            }
            drop(interrupt_lock);
        }
    }

    fn set_icfgr(&self, vcpu: &Vcpu, int_id: usize, cfg: u8) {
        if let Some(interrupt) = self.get_int(vcpu, int_id) {
            let interrupt_lock = interrupt.lock.lock();
            if vgic_int_get_owner(vcpu, interrupt) {
                interrupt.set_cfg(cfg);
                if interrupt.hw() {
                    GICD.set_icfgr(interrupt.id() as usize, cfg);
                }
                vgic_int_yield_owner(vcpu, interrupt);
            } else {
                let m = IpiInitcMessage {
                    event: InitcEvent::VgicdSetCfg,
                    vm_id: vcpu.vm_id(),
                    int_id: interrupt.id(),
                    val: cfg,
                };
                if !ipi_send_msg(
                    interrupt.owner_phys_id().unwrap(),
                    IpiType::IpiTIntc,
                    IpiInnerMsg::Initc(m),
                ) {
                    error!(
                        "set_icfgr: Failed to send ipi message, target {} type {}",
                        interrupt.owner_phys_id().unwrap(),
                        0
                    );
                }
            }
            drop(interrupt_lock);
        } else {
            unimplemented!();
        }
    }

    fn get_icfgr(&self, vcpu: &Vcpu, int_id: usize) -> u8 {
        if let Some(interrupt) = self.get_int(vcpu, int_id) {
            interrupt.cfg()
        } else {
            unimplemented!();
        }
    }

    fn sgi_set_pend(&self, vcpu: &Vcpu, int_id: usize, pend: bool) {
        if bit_extract(int_id, 0, 10) > GIC_SGIS_NUM {
            return;
        }

        let source = bit_extract(int_id, 10, 5);

        if let Some(interrupt) = self.get_int(vcpu, bit_extract(int_id, 0, 10)) {
            let interrupt_lock = interrupt.lock.lock();
            self.remove_lr(vcpu, interrupt);
            let vcpu_id = vcpu.id();

            let vgic_int_id = interrupt.id() as usize;
            let pendstate = self.cpu_priv_sgis_pend(vcpu_id, vgic_int_id);
            let new_pendstate = if pend {
                pendstate | (1 << source) as u8
            } else {
                pendstate & !(1 << source) as u8
            };
            // state changed ,the two state isn`t equal
            if (pendstate ^ new_pendstate) != 0 {
                self.set_cpu_priv_sgis_pend(vcpu_id, vgic_int_id, new_pendstate);
                let state = interrupt.state().to_num();
                if new_pendstate != 0 {
                    interrupt.set_state(IrqState::num_to_state(state | 1));
                } else {
                    interrupt.set_state(IrqState::num_to_state(state & !1));
                }

                self.update_int_list(vcpu, interrupt);

                match interrupt.state() {
                    IrqState::IrqSInactive => {
                        debug!("inactive");
                    }
                    _ => {
                        self.add_lr(vcpu, interrupt);
                    }
                }
            }
            drop(interrupt_lock);
        } else {
            error!("sgi_set_pend: interrupt {} is None", bit_extract(int_id, 0, 10));
        }
    }

    fn set_prio(&self, vcpu: &Vcpu, int_id: usize, mut prio: u8) {
        if let Some(interrupt) = self.get_int(vcpu, int_id) {
            prio &= 0xf0; // gic-400 only allows 4 priority bits in non-secure state

            let interrupt_lock = interrupt.lock.lock();
            if vgic_int_get_owner(vcpu, interrupt) {
                if interrupt.prio() != prio {
                    self.remove_lr(vcpu, interrupt);
                    let prev_prio = interrupt.prio();
                    interrupt.set_prio(prio);
                    if prio <= prev_prio {
                        self.route(vcpu, interrupt);
                    }
                    if interrupt.hw() {
                        GICD.set_prio(interrupt.id() as usize, prio);
                    }
                }
                vgic_int_yield_owner(vcpu, interrupt);
            } else {
                let vm_id = vcpu.vm_id();

                let m = IpiInitcMessage {
                    event: InitcEvent::VgicdSetPrio,
                    vm_id,
                    int_id: interrupt.id(),
                    val: prio,
                };
                if !ipi_send_msg(
                    interrupt.owner_phys_id().unwrap(),
                    IpiType::IpiTIntc,
                    IpiInnerMsg::Initc(m),
                ) {
                    error!(
                        "set_prio: Failed to send ipi message, target {} type {}",
                        interrupt.owner_phys_id().unwrap(),
                        0
                    );
                }
            }
            drop(interrupt_lock);
        }
    }

    fn get_prio(&self, vcpu: &Vcpu, int_id: usize) -> u8 {
        self.get_int(vcpu, int_id).unwrap().prio()
    }

    fn set_trgt(&self, vcpu: &Vcpu, int_id: usize, trgt: u8) {
        if let Some(interrupt) = self.get_int(vcpu, int_id) {
            let interrupt_lock = interrupt.lock.lock();
            if vgic_int_get_owner(vcpu, interrupt) {
                if interrupt.targets() != trgt {
                    interrupt.set_targets(trgt);
                    let mut ptrgt = 0;
                    for cpuid in 0..8 {
                        if bit_get(trgt as usize, cpuid) != 0 {
                            ptrgt = bit_set(ptrgt, Platform::cpuid_to_cpuif(cpuid))
                        }
                    }
                    if interrupt.hw() {
                        GICD.set_trgt(interrupt.id() as usize, ptrgt as u8);
                    }
                    if vgic_get_state(interrupt) != 0 {
                        self.route(vcpu, interrupt);
                    }
                }
                vgic_int_yield_owner(vcpu, interrupt);
            } else {
                let vm_id = vcpu.vm_id();
                let m = IpiInitcMessage {
                    event: InitcEvent::VgicdSetTrgt,
                    vm_id,
                    int_id: interrupt.id(),
                    val: trgt,
                };
                if !ipi_send_msg(
                    interrupt.owner_phys_id().unwrap(),
                    IpiType::IpiTIntc,
                    IpiInnerMsg::Initc(m),
                ) {
                    error!(
                        "set_trgt: Failed to send ipi message, target {} type {}",
                        interrupt.owner_phys_id().unwrap(),
                        0
                    );
                }
            }
            drop(interrupt_lock);
        }
    }

    fn get_trgt(&self, vcpu: &Vcpu, int_id: usize) -> u8 {
        self.get_int(vcpu, int_id).unwrap().targets()
    }

    pub fn inject(&self, vcpu: &Vcpu, int_id: usize) {
        if let Some(interrupt) = self.get_int(vcpu, bit_extract(int_id, 0, 10)) {
            if interrupt.hw() {
                let interrupt_lock = interrupt.lock.lock();
                interrupt.locked_helper(|interrupt| {
                    interrupt.owner = Some(vcpu.clone());
                    interrupt.state = IrqState::IrqSPend;
                    interrupt.in_lr = false;
                });
                self.update_int_list(vcpu, interrupt);
                self.route(vcpu, interrupt);
                drop(interrupt_lock);
            } else {
                self.set_pend(vcpu, int_id, true);
            }
        }
    }

    fn emu_ctrl_access(&self, emu_ctx: &EmuContext) {
        if emu_ctx.write {
            let prev_ctlr = self.vgicd_ctlr();
            let idx = emu_ctx.reg;
            self.set_vgicd_ctlr(current_cpu().get_gpr(idx) as u32 & 0x1);
            if prev_ctlr ^ self.vgicd_ctlr() != 0 {
                let enable = self.vgicd_ctlr() != 0;
                let hcr = GICH.hcr();
                if enable {
                    GICH.set_hcr(hcr | 1);
                } else {
                    GICH.set_hcr(hcr & !1);
                }

                let m = IpiInitcMessage {
                    event: InitcEvent::VgicdGichEn,
                    vm_id: active_vm_id(),
                    int_id: 0,
                    val: enable as u8,
                };
                ipi_intra_broadcast_msg(&active_vm().unwrap(), IpiType::IpiTIntc, IpiInnerMsg::Initc(m));
            }
        } else {
            let idx = emu_ctx.reg;
            let val = self.vgicd_ctlr() as usize;
            current_cpu().set_gpr(idx, val);
        }
    }

    fn emu_typer_access(&self, emu_ctx: &EmuContext) {
        if !emu_ctx.write {
            let idx = emu_ctx.reg;
            let val = self.vgicd_typer() as usize;
            current_cpu().set_gpr(idx, val);
        } else {
            warn!("emu_typer_access: can't write to RO reg");
        }
    }

    fn emu_iidr_access(&self, emu_ctx: &EmuContext) {
        if !emu_ctx.write {
            let idx = emu_ctx.reg;
            let val = self.vgicd_iidr() as usize;
            current_cpu().set_gpr(idx, val);
        } else {
            warn!("emu_iidr_access: can't write to RO reg");
        }
    }

    fn emu_isenabler_access(&self, emu_ctx: &EmuContext) {
        // println!("DEBUG: in emu_isenabler_access");
        let reg_idx = (emu_ctx.address & 0b1111111) / 4;
        let idx = emu_ctx.reg;
        let mut val = if emu_ctx.write { current_cpu().get_gpr(idx) } else { 0 };
        let first_int = reg_idx * 32;
        let vm_id = active_vm_id();
        let vm = match active_vm() {
            Some(vm) => vm,
            None => {
                panic!("emu_isenabler_access: current vcpu.vm is none");
            }
        };
        let mut vm_has_interrupt_flag = false;

        for i in 0..32 {
            if vm.has_interrupt(first_int + i) || vm.emu_has_interrupt(first_int + i) {
                vm_has_interrupt_flag = true;
                break;
            }
        }
        if first_int >= 16 && !vm_has_interrupt_flag {
            error!(
                "emu_isenabler_access: vm[{}] does not have interrupt {}",
                vm_id, first_int
            );
            return;
        }

        if emu_ctx.write {
            for i in 0..32 {
                if bit_get(val, i) != 0 {
                    self.set_enable(current_cpu().active_vcpu.as_ref().unwrap(), first_int + i, true);
                }
            }
        } else {
            for i in 0..32 {
                if self.get_enable(current_cpu().active_vcpu.as_ref().unwrap(), first_int + i) {
                    val |= 1 << i;
                }
            }
            let idx = emu_ctx.reg;
            current_cpu().set_gpr(idx, val);
        }
    }

    fn emu_pendr_access(&self, emu_ctx: &EmuContext, set: bool) {
        trace!("emu_pendr_access");
        let reg_idx = (emu_ctx.address & 0b1111111) / 4;
        let idx = emu_ctx.reg;
        let mut val = if emu_ctx.write { current_cpu().get_gpr(idx) } else { 0 };
        let first_int = reg_idx * 32;
        let vm_id = active_vm_id();
        let vm = match active_vm() {
            Some(vm) => vm,
            None => {
                panic!("emu_pendr_access: current vcpu.vm is none");
            }
        };
        let mut vm_has_interrupt_flag = false;

        for i in 0..emu_ctx.width {
            if vm.has_interrupt(first_int + i) || vm.emu_has_interrupt(first_int + i) {
                vm_has_interrupt_flag = true;
                break;
            }
        }
        if first_int >= 16 && !vm_has_interrupt_flag {
            error!("emu_pendr_access: vm[{}] does not have interrupt {}", vm_id, first_int);
            return;
        }

        if emu_ctx.write {
            for i in 0..32 {
                if bit_get(val, i) != 0 {
                    self.set_pend(current_cpu().active_vcpu.as_ref().unwrap(), first_int + i, set);
                }
            }
        } else {
            for i in 0..32 {
                match self.get_int(current_cpu().active_vcpu.as_ref().unwrap(), first_int + i) {
                    Some(interrupt) => {
                        if vgic_get_state(interrupt) & 1 != 0 {
                            val |= 1 << i;
                        }
                    }
                    None => {
                        unimplemented!();
                    }
                }
            }
            let idx = emu_ctx.reg;
            current_cpu().set_gpr(idx, val);
        }
    }

    fn emu_ispendr_access(&self, emu_ctx: &EmuContext) {
        self.emu_pendr_access(emu_ctx, true);
    }

    fn emu_activer_access(&self, emu_ctx: &EmuContext, set: bool) {
        // println!("DEBUG: in emu_activer_access");
        let reg_idx = (emu_ctx.address & 0b1111111) / 4;
        let idx = emu_ctx.reg;
        let mut val = if emu_ctx.write { current_cpu().get_gpr(idx) } else { 0 };
        let first_int = reg_idx * 32;
        let vm_id = active_vm_id();
        let vm = match active_vm() {
            Some(vm) => vm,
            None => {
                panic!("emu_activer_access: current vcpu.vm is none");
            }
        };
        let mut vm_has_interrupt_flag = false;

        for i in 0..32 {
            if vm.has_interrupt(first_int + i) || vm.emu_has_interrupt(first_int + i) {
                vm_has_interrupt_flag = true;
                break;
            }
        }
        if first_int >= 16 && !vm_has_interrupt_flag {
            warn!(
                "emu_activer_access: vm[{}] does not have interrupt {}",
                vm_id, first_int
            );
            return;
        }

        if emu_ctx.write {
            for i in 0..32 {
                if bit_get(val, i) != 0 {
                    self.set_active(current_cpu().active_vcpu.as_ref().unwrap(), first_int + i, set);
                }
            }
        } else {
            for i in 0..32 {
                match self.get_int(current_cpu().active_vcpu.as_ref().unwrap(), first_int + i) {
                    Some(interrupt) => {
                        if vgic_get_state(interrupt) & 2 != 0 {
                            val |= 1 << i;
                        }
                    }
                    None => {
                        unimplemented!();
                    }
                }
            }
            let idx = emu_ctx.reg;
            current_cpu().set_gpr(idx, val);
        }
    }

    fn emu_isactiver_access(&self, emu_ctx: &EmuContext) {
        self.emu_activer_access(emu_ctx, true);
    }

    fn emu_icenabler_access(&self, emu_ctx: &EmuContext) {
        let reg_idx = (emu_ctx.address & 0b1111111) / 4;
        let idx = emu_ctx.reg;
        let mut val = if emu_ctx.write { current_cpu().get_gpr(idx) } else { 0 };
        let first_int = reg_idx * 32;
        let vm_id = active_vm_id();
        let vm = match active_vm() {
            Some(vm) => vm,
            None => {
                panic!("emu_activer_access: current vcpu.vm is none");
            }
        };
        let mut vm_has_interrupt_flag = false;

        if emu_ctx.write {
            for i in 0..32 {
                if vm.has_interrupt(first_int + i) || vm.emu_has_interrupt(first_int + i) {
                    vm_has_interrupt_flag = true;
                    break;
                }
            }
            if first_int >= 16 && !vm_has_interrupt_flag {
                warn!(
                    "emu_icenabler_access: vm[{}] does not have interrupt {}",
                    vm_id, first_int
                );
                return;
            }
        }

        if emu_ctx.write {
            for i in 0..32 {
                if bit_get(val, i) != 0 {
                    self.set_enable(current_cpu().active_vcpu.as_ref().unwrap(), first_int + i, false);
                }
            }
        } else {
            for i in 0..32 {
                if self.get_enable(current_cpu().active_vcpu.as_ref().unwrap(), first_int + i) {
                    val |= 1 << i;
                }
            }
            let idx = emu_ctx.reg;
            current_cpu().set_gpr(idx, val);
        }
    }

    fn emu_icpendr_access(&self, emu_ctx: &EmuContext) {
        self.emu_pendr_access(emu_ctx, false);
    }

    fn emu_icativer_access(&self, emu_ctx: &EmuContext) {
        self.emu_activer_access(emu_ctx, false);
    }

    fn emu_icfgr_access(&self, emu_ctx: &EmuContext) {
        let first_int = (32 / GIC_CONFIG_BITS) * bit_extract(emu_ctx.address, 0, 9) / 4;
        let vm_id = active_vm_id();
        let vm = match active_vm() {
            Some(vm) => vm,
            None => {
                panic!("emu_icfgr_access: current vcpu.vm is none");
            }
        };
        let mut vm_has_interrupt_flag = false;

        if emu_ctx.write {
            for i in 0..emu_ctx.width * 8 {
                if vm.has_interrupt(first_int + i) || vm.emu_has_interrupt(first_int + i) {
                    vm_has_interrupt_flag = true;
                    break;
                }
            }
            if first_int >= 16 && !vm_has_interrupt_flag {
                warn!("emu_icfgr_access: vm[{}] does not have interrupt {}", vm_id, first_int);
                return;
            }
        }

        if emu_ctx.write {
            let idx = emu_ctx.reg;
            let cfg = current_cpu().get_gpr(idx);
            let mut irq = first_int;
            let mut bit = 0;
            while bit < emu_ctx.width * 8 {
                self.set_icfgr(
                    current_cpu().active_vcpu.as_ref().unwrap(),
                    irq,
                    bit_extract(cfg as usize, bit, 2) as u8,
                );
                bit += 2;
                irq += 1;
            }
        } else {
            let mut cfg = 0;
            let mut irq = first_int;
            let mut bit = 0;
            while bit < emu_ctx.width * 8 {
                cfg |= (self.get_icfgr(current_cpu().active_vcpu.as_ref().unwrap(), irq) as usize) << bit;
                bit += 2;
                irq += 1;
            }
            let idx = emu_ctx.reg;
            let val = cfg;
            current_cpu().set_gpr(idx, val);
        }
    }

    fn emu_sgiregs_access(&self, emu_ctx: &EmuContext) {
        let idx = emu_ctx.reg;
        let val = if emu_ctx.write { current_cpu().get_gpr(idx) } else { 0 };
        let vm = match active_vm() {
            Some(vm) => vm,
            None => {
                panic!("emu_sgiregs_access: current vcpu.vm is none");
            }
        };

        if bit_extract(emu_ctx.address, 0, 12) == bit_extract(Platform::GICD_BASE + 0x0f00, 0, 12) {
            if emu_ctx.write {
                let sgir_trglstflt = bit_extract(val, 24, 2);
                let mut trgtlist = 0;
                // println!("addr {:x}, sgir trglst flt {}, vtrgt {}", emu_ctx.address, sgir_trglstflt, bit_extract(val, 16, 8));
                match sgir_trglstflt {
                    0 => {
                        trgtlist = vgic_target_translate(&vm, bit_extract(val, 16, 8) as u32, true) as usize;
                    }
                    1 => {
                        trgtlist = active_vm_ncpu() & !(1 << current_cpu().id);
                    }
                    2 => {
                        trgtlist = 1 << current_cpu().id;
                    }
                    3 => {
                        return;
                    }
                    _ => {}
                }

                for i in 0..8 {
                    if trgtlist & (1 << i) != 0 {
                        let m = IpiInitcMessage {
                            event: InitcEvent::VgicdSetPend,
                            vm_id: active_vm_id(),
                            int_id: (bit_extract(val, 0, 8) | (active_vcpu_id() << 10)) as u16,
                            val: true as u8,
                        };
                        if !ipi_send_msg(i, IpiType::IpiTIntc, IpiInnerMsg::Initc(m)) {
                            error!(
                                "emu_sgiregs_access: Failed to send ipi message, target {} type {}",
                                i, 0
                            );
                        }
                    }
                }
            }
        } else {
            // TODO: CPENDSGIR and SPENDSGIR access
            warn!("unimplemented: CPENDSGIR and SPENDSGIR access");
        }
    }

    fn emu_ipriorityr_access(&self, emu_ctx: &EmuContext) {
        let idx = emu_ctx.reg;
        let mut val = if emu_ctx.write { current_cpu().get_gpr(idx) } else { 0 };
        let first_int = (8 / GIC_PRIO_BITS) * bit_extract(emu_ctx.address, 0, 9);
        let vm_id = active_vm_id();
        let vm = match active_vm() {
            Some(vm) => vm,
            None => {
                panic!("emu_ipriorityr_access: current vcpu.vm is none");
            }
        };
        let mut vm_has_interrupt_flag = false;

        if emu_ctx.write {
            for i in 0..emu_ctx.width {
                if vm.has_interrupt(first_int + i) || vm.emu_has_interrupt(first_int + i) {
                    vm_has_interrupt_flag = true;
                    break;
                }
            }
            if first_int >= 16 && !vm_has_interrupt_flag {
                warn!(
                    "emu_ipriorityr_access: vm[{}] does not have interrupt {}",
                    vm_id, first_int
                );
                return;
            }
        }

        if emu_ctx.write {
            for i in 0..emu_ctx.width {
                self.set_prio(
                    current_cpu().active_vcpu.as_ref().unwrap(),
                    first_int + i,
                    bit_extract(val, GIC_PRIO_BITS * i, GIC_PRIO_BITS) as u8,
                );
            }
        } else {
            for i in 0..emu_ctx.width {
                val |= (self.get_prio(current_cpu().active_vcpu.as_ref().unwrap(), first_int + i) as usize)
                    << (GIC_PRIO_BITS * i);
            }
            let idx = emu_ctx.reg;
            current_cpu().set_gpr(idx, val);
        }
    }

    fn emu_itargetr_access(&self, emu_ctx: &EmuContext) {
        let idx = emu_ctx.reg;
        let mut val = if emu_ctx.write { current_cpu().get_gpr(idx) } else { 0 };
        let first_int = (8 / GIC_TARGET_BITS) * bit_extract(emu_ctx.address, 0, 9);

        if emu_ctx.write {
            val = vgic_target_translate(&active_vm().unwrap(), val as u32, true) as usize;
            for i in 0..emu_ctx.width {
                self.set_trgt(
                    current_cpu().active_vcpu.as_ref().unwrap(),
                    first_int + i,
                    bit_extract(val, GIC_TARGET_BITS * i, GIC_TARGET_BITS) as u8,
                );
            }
        } else {
            for i in 0..emu_ctx.width {
                val |= (self.get_trgt(current_cpu().active_vcpu.as_ref().unwrap(), first_int + i) as usize)
                    << (GIC_TARGET_BITS * i);
            }
            val = vgic_target_translate(&active_vm().unwrap(), val as u32, false) as usize;
            let idx = emu_ctx.reg;
            current_cpu().set_gpr(idx, val);
        }
    }

    fn handle_trapped_eoir(&self, vcpu: &Vcpu) {
        let gic_lrs = gic_lrs();
        let mut lr_idx_opt = bitmap_find_nth(
            GICH.eisr(0) as usize | ((GICH.eisr(1) as usize) << 32),
            0,
            gic_lrs,
            1,
            true,
        );

        while lr_idx_opt.is_some() {
            let lr_idx = lr_idx_opt.unwrap();
            let lr_val = GICH.lr(lr_idx) as usize;
            GICH.set_lr(lr_idx, 0);

            match self.get_int(vcpu, bit_extract(lr_val, 0, 10)) {
                Some(interrupt) => {
                    let interrupt_lock = interrupt.lock.lock();
                    interrupt.set_in_lr(false);
                    if (interrupt.id() as usize) < GIC_SGIS_NUM {
                        self.add_lr(vcpu, interrupt);
                    } else {
                        vgic_int_yield_owner(vcpu, interrupt);
                    }
                    drop(interrupt_lock);
                }
                None => {
                    unimplemented!();
                }
            }
            lr_idx_opt = bitmap_find_nth(
                GICH.eisr(0) as usize | ((GICH.eisr(1) as usize) << 32),
                0,
                gic_lrs,
                1,
                true,
            );
        }
    }

    fn refill_lrs(&self, vcpu: &Vcpu) {
        let gic_lrs = gic_lrs();
        let mut has_pending = false;

        for i in 0..gic_lrs {
            let lr = GICH.lr(i) as usize;
            if bit_extract(lr, 28, 2) & 1 != 0 {
                has_pending = true;
            }
        }

        let mut lr_idx_opt = bitmap_find_nth(
            GICH.elrsr(0) as usize | ((GICH.elrsr(1) as usize) << 32),
            0,
            gic_lrs,
            1,
            true,
        );

        while lr_idx_opt.is_some() {
            let mut interrupt_opt: Option<&VgicInt> = None;
            let mut prev_pend = false;
            let act_head = self.int_list_head(vcpu, false);
            let pend_head = self.int_list_head(vcpu, true);
            if has_pending {
                if let Some(act_int) = act_head {
                    if !act_int.in_lr() {
                        interrupt_opt = Some(act_int);
                    }
                }
            }
            if interrupt_opt.is_none() {
                if let Some(pend_int) = pend_head {
                    if !pend_int.in_lr() {
                        interrupt_opt = Some(pend_int);
                        prev_pend = true;
                    }
                }
            }

            match interrupt_opt {
                Some(interrupt) => {
                    // println!("refill int {}", interrupt.id());
                    vgic_int_get_owner(vcpu, interrupt);
                    self.write_lr(vcpu, interrupt, lr_idx_opt.unwrap());
                    has_pending = has_pending || prev_pend;
                }
                None => {
                    // println!("no int to refill");
                    let hcr = GICH.hcr();
                    GICH.set_hcr(hcr & !(1 << 3));
                    break;
                }
            }

            lr_idx_opt = bitmap_find_nth(
                GICH.elrsr(0) as usize | ((GICH.elrsr(1) as usize) << 32),
                0,
                gic_lrs,
                1,
                true,
            );
        }
    }

    fn eoir_highest_spilled_active(&self, vcpu: &Vcpu) {
        if let Some(int) = self.int_list_head(vcpu, false) {
            int.lock.lock();
            vgic_int_get_owner(vcpu, int);

            let state = int.state().to_num();
            int.set_state(IrqState::num_to_state(state & !2));
            self.update_int_list(vcpu, int);

            if vgic_int_is_hw(int) {
                GICD.set_act(int.id() as usize, false);
            } else if int.state().to_num() & 1 != 0 {
                self.add_lr(vcpu, int);
            }
        }
    }
}

fn vgic_target_translate(vm: &Vm, trgt: u32, v2p: bool) -> u32 {
    let from = trgt.to_le_bytes();

    let mut result = 0;
    for (idx, val) in from
        .map(|x| {
            if v2p {
                vm.vcpu_to_pcpu_mask(x as usize, 8) as u32
            } else {
                vm.pcpu_to_vcpu_mask(x as usize, 8) as u32
            }
        })
        .iter()
        .enumerate()
    {
        result |= *val << (8 * idx);
        if idx >= 4 {
            panic!("illegal idx, from len {}", from.len());
        }
    }
    result
}

fn vgic_owns(vcpu: &Vcpu, interrupt: &VgicInt) -> bool {
    if gic_is_priv(interrupt.id() as usize) {
        return true;
    }

    let vcpu_id = vcpu.id();
    let pcpu_id = vcpu.phys_id();
    match interrupt.owner() {
        Some(owner) => {
            let owner_vcpu_id = owner.id();
            let owner_pcpu_id = owner.phys_id();
            owner_vcpu_id == vcpu_id && owner_pcpu_id == pcpu_id
        }
        None => false,
    }
}

fn vgic_get_state(interrupt: &VgicInt) -> usize {
    let mut state = interrupt.state().to_num();

    if interrupt.in_lr() && interrupt.owner_phys_id().unwrap() == current_cpu().id {
        let lr_option = gich_get_lr(interrupt);
        if let Some(lr_val) = lr_option {
            state = lr_val as usize;
        }
    }

    if interrupt.id() as usize >= GIC_SGIS_NUM {
        return state;
    }
    if interrupt.owner().is_none() {
        return state;
    }

    let vm = interrupt.owner_vm();
    let vgic = vm.vgic();
    let vcpu_id = interrupt.owner_id().unwrap();

    if vgic.cpu_priv_sgis_pend(vcpu_id, interrupt.id() as usize) != 0 {
        state |= 1;
    }

    state
}

fn vgic_int_yield_owner(vcpu: &Vcpu, interrupt: &VgicInt) {
    if !vgic_owns(vcpu, interrupt) || interrupt.in_lr() || gic_is_priv(interrupt.id() as usize) {
        return;
    }

    if vgic_get_state(interrupt) & 2 == 0 {
        interrupt.clear_owner();
    }
}

fn vgic_int_is_hw(interrupt: &VgicInt) -> bool {
    interrupt.id() as usize >= GIC_SGIS_NUM && interrupt.hw()
}

fn gich_get_lr(interrupt: &VgicInt) -> Option<u32> {
    let cpu_id = current_cpu().id;
    let phys_id = interrupt.owner_phys_id().unwrap();

    if !interrupt.in_lr() || phys_id != cpu_id {
        return None;
    }

    let lr_val = GICH.lr(interrupt.lr() as usize);
    if (lr_val & 0b1111111111 == interrupt.id() as u32) && (lr_val >> 28 & 0b11 != 0) {
        return Some(lr_val);
    }
    None
}

fn vgic_int_get_owner(vcpu: &Vcpu, interrupt: &VgicInt) -> bool {
    let vcpu_id = vcpu.id();
    let vcpu_vm_id = vcpu.vm_id();

    match interrupt.owner() {
        Some(owner) => {
            let owner_vcpu_id = owner.id();
            let owner_vm_id = owner.vm_id();

            owner_vm_id == vcpu_vm_id && owner_vcpu_id == vcpu_id
        }
        None => {
            interrupt.set_owner(vcpu.clone());
            true
        }
    }
}

pub fn gic_maintenance_handler() {
    let misr = GICH.misr();
    let vm = match active_vm() {
        Some(vm) => vm,
        None => {
            panic!("gic_maintenance_handler: current vcpu.vm is None");
        }
    };
    let vgic = vm.vgic();

    if misr & 1 != 0 {
        vgic.handle_trapped_eoir(current_cpu().active_vcpu.as_ref().unwrap());
    }

    if misr & (1 << 3) != 0 {
        vgic.refill_lrs(current_cpu().active_vcpu.as_ref().unwrap());
    }

    if misr & (1 << 2) != 0 {
        let mut hcr = GICH.hcr();
        while hcr & (0b11111 << 27) != 0 {
            vgic.eoir_highest_spilled_active(current_cpu().active_vcpu.as_ref().unwrap());
            hcr -= 1 << 27;
            GICH.set_hcr(hcr);
            hcr = GICH.hcr();
        }
    }
}

const VGICD_REG_OFFSET_PREFIX_CTLR: usize = 0x0;
// same as TYPER & IIDR
const VGICD_REG_OFFSET_PREFIX_ISENABLER: usize = 0x2;
const VGICD_REG_OFFSET_PREFIX_ICENABLER: usize = 0x3;
const VGICD_REG_OFFSET_PREFIX_ISPENDR: usize = 0x4;
const VGICD_REG_OFFSET_PREFIX_ICPENDR: usize = 0x5;
const VGICD_REG_OFFSET_PREFIX_ISACTIVER: usize = 0x6;
const VGICD_REG_OFFSET_PREFIX_ICACTIVER: usize = 0x7;
const VGICD_REG_OFFSET_PREFIX_ICFGR: usize = 0x18;
const VGICD_REG_OFFSET_PREFIX_SGIR: usize = 0x1e;

pub fn vgicd_emu_access_is_vaild(emu_ctx: &EmuContext) -> bool {
    let offset = emu_ctx.address & 0xfff;
    let offset_prefix = (offset & 0xf80) >> 7;
    match offset_prefix {
        VGICD_REG_OFFSET_PREFIX_CTLR
        | VGICD_REG_OFFSET_PREFIX_ISENABLER
        | VGICD_REG_OFFSET_PREFIX_ISPENDR
        | VGICD_REG_OFFSET_PREFIX_ISACTIVER
        | VGICD_REG_OFFSET_PREFIX_ICENABLER
        | VGICD_REG_OFFSET_PREFIX_ICPENDR
        | VGICD_REG_OFFSET_PREFIX_ICACTIVER
        | VGICD_REG_OFFSET_PREFIX_ICFGR => {
            if emu_ctx.width != 4 || emu_ctx.address & 0x3 != 0 {
                return false;
            }
        }
        VGICD_REG_OFFSET_PREFIX_SGIR => {
            if (emu_ctx.width == 4 && emu_ctx.address & 0x3 != 0) || (emu_ctx.width == 2 && emu_ctx.address & 0x1 != 0)
            {
                return false;
            }
        }
        _ => {
            // TODO: hard code to rebuild (gicd IPRIORITYR and ITARGETSR)
            if (0x400..0xc00).contains(&offset)
                && ((emu_ctx.width == 4 && emu_ctx.address & 0x3 != 0)
                    || (emu_ctx.width == 2 && emu_ctx.address & 0x1 != 0))
            {
                return false;
            }
        }
    }
    true
}

pub fn vgic_ipi_handler(msg: IpiMessage) {
    if let IpiInnerMsg::Initc(intc) = msg.ipi_message {
        let vm_id = intc.vm_id;
        let int_id = intc.int_id;
        let val = intc.val;
        let trgt_vcpu = match current_cpu().vcpu_array.pop_vcpu_through_vmid(vm_id) {
            None => {
                error!("Core {} received vgic msg from unknown VM {}", current_cpu().id, vm_id);
                return;
            }
            Some(vcpu) => vcpu,
        };
        restore_vcpu_gic(current_cpu().active_vcpu.clone(), trgt_vcpu.clone());

        let vm = match trgt_vcpu.vm() {
            None => {
                panic!("vgic_ipi_handler: vm is None");
            }
            Some(x) => x,
        };
        let vgic = vm.vgic();

        if vm_id != vm.id() {
            error!("VM {} received vgic msg from another vm {}", vm.id(), vm_id);
            return;
        }
        match intc.event {
            InitcEvent::VgicdGichEn => {
                let hcr = GICH.hcr();
                if val != 0 {
                    GICH.set_hcr(hcr | 0b1);
                } else {
                    GICH.set_hcr(hcr & !0b1);
                }
            }
            InitcEvent::VgicdSetEn => {
                vgic.set_enable(trgt_vcpu, int_id as usize, val != 0);
            }
            InitcEvent::VgicdSetPend => {
                vgic.set_pend(trgt_vcpu, int_id as usize, val != 0);
            }
            InitcEvent::VgicdSetPrio => {
                vgic.set_prio(trgt_vcpu, int_id as usize, val);
            }
            InitcEvent::VgicdSetTrgt => {
                vgic.set_trgt(trgt_vcpu, int_id as usize, val);
            }
            InitcEvent::VgicdRoute => {
                if let Some(interrupt) = vgic.get_int(trgt_vcpu, bit_extract(int_id as usize, 0, 10)) {
                    let interrupt_lock = interrupt.lock.lock();
                    if vgic_int_get_owner(trgt_vcpu, interrupt) {
                        if (interrupt.targets() & (1 << current_cpu().id)) != 0 {
                            vgic.add_lr(trgt_vcpu, interrupt);
                        }
                        vgic_int_yield_owner(trgt_vcpu, interrupt);
                    }
                    drop(interrupt_lock);
                }
            }
            _ => {
                error!("vgic_ipi_handler: core {} received unknown event", current_cpu().id)
            }
        }
        save_vcpu_gic(current_cpu().active_vcpu.clone(), trgt_vcpu);
    } else {
        error!("vgic_ipi_handler: illegal ipi");
    }
}

pub fn emu_intc_init(emu_cfg: &VmEmulatedDeviceConfig, vcpu_list: &[Vcpu]) -> Result<Arc<dyn EmuDev>, ()> {
    if emu_cfg.emu_type != EmuDeviceType::EmuDeviceTGicd {
        error!("emu_intc_init: emu_type is not EmuDeviceTGicd");
        return Err(());
    }

    let vcpu_num = vcpu_list.len();
    let mut vgic = Vgic::new(emu_cfg.base_ipa, emu_cfg.length, vcpu_num);

    for i in 0..GIC_SPI_MAX {
        vgic.vgicd.interrupts.push(VgicInt::new(i));
    }

    for vcpu in vcpu_list {
        let mut cpu_priv = VgicCpuPriv::default();
        for int_idx in 0..GIC_PRIVINT_NUM {
            let phys_id = vcpu.phys_id();

            cpu_priv.interrupts.push(VgicInt::priv_new(
                int_idx,
                vcpu.clone(),
                1 << phys_id,
                int_idx < GIC_SGIS_NUM,
            ));
        }

        vgic.cpu_priv.push(cpu_priv);
    }

    Ok(Arc::new(vgic))
}

pub fn vgic_set_hw_int(vm: &Vm, int_id: usize) {
    if int_id < GIC_SGIS_NUM {
        return;
    }

    if !vm.has_vgic() {
        return;
    }
    let vgic = vm.vgic();

    if int_id < GIC_PRIVINT_NUM {
        for i in 0..vm.cpu_num() {
            if let Some(interrupt) = vgic.get_int(vm.vcpu(i).unwrap(), int_id) {
                let interrupt_lock = interrupt.lock.lock();
                interrupt.set_hw(true);
                drop(interrupt_lock);
            }
        }
    } else if let Some(interrupt) = vgic.get_int(vm.vcpu(0).unwrap(), int_id) {
        let interrupt_lock = interrupt.lock.lock();
        interrupt.set_hw(true);
        drop(interrupt_lock);
    }
}

impl EmuDev for Vgic {
    fn emu_type(&self) -> EmuDeviceType {
        EmuDeviceType::EmuDeviceTGicd
    }

    fn address_range(&self) -> Range<usize> {
        self.address_range.clone()
    }

    fn handler(&self, emu_ctx: &EmuContext) -> bool {
        let offset = emu_ctx.address & 0xfff;

        let vgicd_offset_prefix = offset >> 7;
        if !vgicd_emu_access_is_vaild(emu_ctx) {
            return false;
        }

        trace!(
            "current_cpu:{} emu_intc_handler offset:{:#x} is write:{},val:{:#x}",
            current_cpu().id,
            emu_ctx.address,
            emu_ctx.write,
            current_cpu().get_gpr(emu_ctx.reg)
        );
        match vgicd_offset_prefix {
            VGICD_REG_OFFSET_PREFIX_ISENABLER => {
                self.emu_isenabler_access(emu_ctx);
            }
            VGICD_REG_OFFSET_PREFIX_ISPENDR => {
                self.emu_ispendr_access(emu_ctx);
            }
            VGICD_REG_OFFSET_PREFIX_ISACTIVER => {
                self.emu_isactiver_access(emu_ctx);
            }
            VGICD_REG_OFFSET_PREFIX_ICENABLER => {
                self.emu_icenabler_access(emu_ctx);
            }
            VGICD_REG_OFFSET_PREFIX_ICPENDR => {
                self.emu_icpendr_access(emu_ctx);
            }
            VGICD_REG_OFFSET_PREFIX_ICACTIVER => {
                self.emu_icativer_access(emu_ctx);
            }
            VGICD_REG_OFFSET_PREFIX_ICFGR => {
                self.emu_icfgr_access(emu_ctx);
            }
            VGICD_REG_OFFSET_PREFIX_SGIR => {
                self.emu_sgiregs_access(emu_ctx);
            }
            _ => {
                match offset {
                    // VGICD_REG_OFFSET(CTLR)
                    0 => {
                        self.emu_ctrl_access(emu_ctx);
                    }
                    // VGICD_REG_OFFSET(TYPER)
                    0x004 => {
                        self.emu_typer_access(emu_ctx);
                    }
                    // VGICD_REG_OFFSET(IIDR)
                    0x008 => {
                        self.emu_iidr_access(emu_ctx);
                    }
                    _ => {
                        if !emu_ctx.write {
                            current_cpu().set_gpr(emu_ctx.reg, 0);
                        }
                    }
                }
                if (0x400..0x800).contains(&offset) {
                    self.emu_ipriorityr_access(emu_ctx);
                } else if (0x800..0xc00).contains(&offset) {
                    self.emu_itargetr_access(emu_ctx);
                }
            }
        }
        true
    }
}

pub struct PartialPassthroughIntc {
    address_range: Range<usize>,
}

impl EmuDev for PartialPassthroughIntc {
    fn emu_type(&self) -> EmuDeviceType {
        EmuDeviceType::EmuDeviceTGPPT
    }

    fn address_range(&self) -> Range<usize> {
        self.address_range.clone()
    }

    fn handler(&self, emu_ctx: &EmuContext) -> bool {
        if !vgicd_emu_access_is_vaild(emu_ctx) {
            return false;
        }

        if emu_ctx.write {
            // SAFETY: Emu_ctx.address is writeable in EL2
            unsafe {
                emu_ctx.write(current_cpu().get_gpr(emu_ctx.reg));
            }
        } else {
            // SAFETY: Emu_ctx.address is readable in EL2
            current_cpu().set_gpr(emu_ctx.reg, unsafe { emu_ctx.read() });
        }

        true
    }
}

pub fn partial_passthrough_intc_init(emu_cfg: &VmEmulatedDeviceConfig) -> Result<Arc<dyn EmuDev>, ()> {
    if emu_cfg.emu_type == EmuDeviceType::EmuDeviceTGPPT {
        let intc = PartialPassthroughIntc {
            address_range: emu_cfg.base_ipa..emu_cfg.base_ipa + emu_cfg.length,
        };
        Ok(Arc::new(intc))
    } else {
        Err(())
    }
}
