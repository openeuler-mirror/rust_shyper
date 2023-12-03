// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use core::mem::size_of;

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec::Vec;

use spin::Mutex;

use crate::{arch::GICH, kernel::IpiInitcMessage};
use crate::board::{Platform, PlatOperation, PLAT_DESC};
use crate::device::EmuContext;
use crate::device::EmuDevs;
use crate::kernel::{current_cpu, restore_vcpu_gic, save_vcpu_gic, cpuid2mpidr};
use crate::kernel::{active_vm, active_vm_id};
use crate::kernel::{ipi_intra_broadcast_msg, ipi_send_msg, IpiInnerMsg, IpiMessage, IpiType};
use crate::kernel::{InitcEvent, Vcpu, Vm};
use crate::utils::{bit_extract, bit_get, bitmap_find_nth, ptr_read_write};

use super::gicv3::*;

#[derive(Clone)]
struct VgicInt {
    inner: Arc<Mutex<VgicIntInner>>,
    pub lock: Arc<Mutex<()>>,
}

impl VgicInt {
    fn new(id: usize) -> VgicInt {
        VgicInt {
            inner: Arc::new(Mutex::new(VgicIntInner::new(id))),
            lock: Arc::new(Mutex::new(())),
        }
    }

    fn priv_new(id: usize, owner: Vcpu, targets: usize, enabled: bool, redist: usize, cfg: usize) -> VgicInt {
        VgicInt {
            inner: Arc::new(Mutex::new(VgicIntInner::priv_new(
                id, owner, targets, enabled, redist, cfg,
            ))),
            lock: Arc::new(Mutex::new(())),
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
        vgic_int.owner = None;
    }

    fn set_hw(&self, hw: bool) {
        let mut vgic_int = self.inner.lock();
        vgic_int.hw = hw;
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

    fn route(&self) -> u64 {
        let vgic_int = self.inner.lock();
        vgic_int.route
    }

    fn phys_redist(&self) -> u64 {
        let vgic_int = self.inner.lock();
        match vgic_int.phys {
            VgicIntPhys::Redist(redist) => redist,
            _ => {
                panic!("must get redist!");
            }
        }
    }

    fn phys_route(&self) -> u64 {
        let vgic_int = self.inner.lock();
        match vgic_int.phys {
            VgicIntPhys::Route(route) => route,
            _ => {
                panic!("must get route!")
            }
        }
    }

    fn set_phys_route(&self, route: usize) {
        let mut vgic_int = self.inner.lock();
        vgic_int.phys = VgicIntPhys::Route(route as u64);
    }

    fn set_phys_redist(&self, redist: usize) {
        let mut vgic_int = self.inner.lock();
        vgic_int.phys = VgicIntPhys::Redist(redist as u64);
    }

    fn set_route(&self, route: usize) {
        let mut vgic_int = self.inner.lock();
        vgic_int.route = route as u64;
    }

    fn id(&self) -> u16 {
        let vgic_int = self.inner.lock();
        vgic_int.id
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

    fn hw(&self) -> bool {
        let vgic_int = self.inner.lock();
        vgic_int.hw
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
                error!("owner_id is None");
                None
            }
        }
    }

    fn owner_vm_id(&self) -> Option<usize> {
        let vgic_int = self.inner.lock();
        vgic_int.owner.as_ref().map(|owner| owner.vm_id())
    }

    fn owner_vm(&self) -> Vm {
        let vgic_int = self.inner.lock();
        vgic_int.owner_vm()
    }
}

#[derive(Clone)]
enum VgicIntPhys {
    Redist(u64),
    Route(u64),
}

struct VgicIntInner {
    owner: Option<Vcpu>,
    route: u64,
    phys: VgicIntPhys,
    id: u16,
    hw: bool,
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

impl VgicIntInner {
    fn new(id: usize) -> VgicIntInner {
        VgicIntInner {
            owner: None,
            route: GICD_IROUTER_INV as u64,
            phys: VgicIntPhys::Route(GICD_IROUTER_INV as u64),
            id: (id + GIC_PRIVINT_NUM) as u16,
            hw: false,
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

    fn priv_new(id: usize, owner: Vcpu, targets: usize, enabled: bool, redist: usize, cfg: usize) -> VgicIntInner {
        VgicIntInner {
            owner: Some(owner),
            route: GICD_IROUTER_INV as u64,
            phys: VgicIntPhys::Redist(redist as u64),
            id: id as u16,
            hw: false,
            in_lr: false,
            lr: 0,
            enabled,
            state: IrqState::IrqSInactive,
            prio: 0xff,
            targets: targets as u8,
            cfg: cfg as u8,
            in_pend: false,
            in_act: false,
        }
    }

    fn owner_vm(&self) -> Vm {
        let owner = self.owner.as_ref().unwrap();
        owner.vm().unwrap()
    }
}

struct Vgicd {
    ctlr: u32,
    typer: u32,
    iidr: u32,
    interrupts: Vec<VgicInt>,
}

impl Vgicd {
    fn default() -> Vgicd {
        Vgicd {
            ctlr: 0,
            typer: 0,
            iidr: 0,
            interrupts: Vec::new(),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Sgis {
    pub pend: u8,
    pub act: u8,
}

impl Sgis {
    fn default() -> Sgis {
        Sgis { pend: 0, act: 0 }
    }
}

struct Vgicr {
    inner: Arc<Mutex<VgicrInner>>,
    pub lock: Arc<Mutex<()>>,
}

impl Vgicr {
    fn default() -> Vgicr {
        Vgicr {
            inner: Arc::new(Mutex::new(VgicrInner::default())),
            lock: Arc::new(Mutex::new(())),
        }
    }
    fn new(typer: usize, cltr: usize, iidr: usize) -> Vgicr {
        Vgicr {
            inner: Arc::new(Mutex::new(VgicrInner::new(typer, cltr, iidr))),
            lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn get_typer(&self) -> u64 {
        let vgicr = self.inner.lock();
        vgicr.typer
    }

    pub fn set_typer(&self, typer: usize) {
        let mut vgicr = self.inner.lock();
        vgicr.typer = typer as u64;
    }
}

struct VgicrInner {
    typer: u64,
    cltr: u32,
    iidr: u32,
}

impl VgicrInner {
    fn default() -> VgicrInner {
        VgicrInner {
            typer: 0,
            cltr: 0,
            iidr: 0,
        }
    }

    fn new(typer: usize, cltr: usize, iidr: usize) -> VgicrInner {
        VgicrInner {
            typer: typer as u64,
            cltr: cltr as u32,
            iidr: iidr as u32,
        }
    }
}

struct VgicCpuPriv {
    vigcr: Vgicr,
    // gich: GicHypervisorInterfaceBlock,
    curr_lrs: [u16; GIC_LIST_REGS_NUM],
    sgis: [Sgis; GIC_SGIS_NUM],
    interrupts: Vec<VgicInt>,

    pend_list: VecDeque<VgicInt>,
    act_list: VecDeque<VgicInt>,
}

impl VgicCpuPriv {
    fn default() -> VgicCpuPriv {
        VgicCpuPriv {
            vigcr: Vgicr::default(),
            curr_lrs: [0; GIC_LIST_REGS_NUM],
            sgis: [Sgis::default(); GIC_SGIS_NUM],
            interrupts: Vec::new(),
            pend_list: VecDeque::new(),
            act_list: VecDeque::new(),
        }
    }

    fn new(typer: usize, cltr: usize, iidr: usize) -> VgicCpuPriv {
        VgicCpuPriv {
            vigcr: Vgicr::new(typer, cltr, iidr),
            curr_lrs: [0; GIC_LIST_REGS_NUM],
            sgis: [Sgis::default(); GIC_SGIS_NUM],
            interrupts: Vec::new(),
            pend_list: VecDeque::new(),
            act_list: VecDeque::new(),
        }
    }
}

pub struct Vgic {
    vgicd: Mutex<Vgicd>,
    cpu_priv: Mutex<Vec<VgicCpuPriv>>,
}

impl Vgic {
    pub fn default() -> Vgic {
        Vgic {
            vgicd: Mutex::new(Vgicd::default()),
            cpu_priv: Mutex::new(Vec::new()),
        }
    }

    fn remove_int_list(&self, vcpu: Vcpu, interrupt: VgicInt, is_pend: bool) {
        let mut cpu_priv = self.cpu_priv.lock();
        let vcpu_id = vcpu.id();
        let int_id = interrupt.id();
        if interrupt.in_lr() {
            if is_pend {
                if !interrupt.in_pend() {
                    return;
                }
                for i in 0..cpu_priv[vcpu_id].pend_list.len() {
                    if cpu_priv[vcpu_id].pend_list[i].id() == int_id {
                        cpu_priv[vcpu_id].pend_list.remove(i);
                        break;
                    }
                }
                interrupt.set_in_pend_state(false);
            } else {
                if !interrupt.in_act() {
                    return;
                }
                for i in 0..cpu_priv[vcpu_id].act_list.len() {
                    if cpu_priv[vcpu_id].act_list[i].id() == int_id {
                        cpu_priv[vcpu_id].act_list.remove(i);
                        break;
                    }
                }
                interrupt.set_in_act_state(false);
            };
        }
    }

    fn add_int_list(&self, vcpu: Vcpu, interrupt: VgicInt, is_pend: bool) {
        let mut cpu_priv = self.cpu_priv.lock();
        let vcpu_id = vcpu.id();
        if !interrupt.in_lr() {
            if is_pend {
                interrupt.set_in_pend_state(true);
                cpu_priv[vcpu_id].pend_list.push_back(interrupt);
            } else {
                interrupt.set_in_act_state(true);
                cpu_priv[vcpu_id].act_list.push_back(interrupt);
            }
        }
    }

    fn update_int_list(&self, vcpu: Vcpu, interrupt: VgicInt) {
        let state = interrupt.state().to_num();

        if state & IrqState::IrqSPend.to_num() != 0 && !interrupt.in_pend() {
            self.add_int_list(vcpu.clone(), interrupt.clone(), true);
        } else if state & IrqState::IrqSPend.to_num() == 0 {
            self.remove_int_list(vcpu.clone(), interrupt.clone(), true);
        }

        if state & IrqState::IrqSActive.to_num() != 0 && !interrupt.in_act() {
            self.add_int_list(vcpu.clone(), interrupt.clone(), false);
        } else if state & IrqState::IrqSActive.to_num() == 0 {
            self.remove_int_list(vcpu.clone(), interrupt.clone(), false);
        }
    }

    fn int_list_head(&self, vcpu: Vcpu, is_pend: bool) -> Option<VgicInt> {
        let cpu_priv = self.cpu_priv.lock();
        let vcpu_id = vcpu.id();
        if is_pend {
            if cpu_priv[vcpu_id].pend_list.is_empty() {
                None
            } else {
                Some(cpu_priv[vcpu_id].pend_list[0].clone())
            }
        } else if cpu_priv[vcpu_id].act_list.is_empty() {
            None
        } else {
            Some(cpu_priv[vcpu_id].act_list[0].clone())
        }
    }

    fn set_vgicd_ctlr(&self, ctlr: u32) {
        let mut vgicd = self.vgicd.lock();
        vgicd.ctlr = ctlr;
    }

    pub fn vgicd_ctlr(&self) -> u32 {
        let vgicd = self.vgicd.lock();
        vgicd.ctlr
    }

    pub fn vgicd_typer(&self) -> u32 {
        let vgicd = self.vgicd.lock();
        vgicd.typer
    }

    pub fn vgicd_iidr(&self) -> u32 {
        let vgicd = self.vgicd.lock();
        vgicd.iidr
    }

    fn vgicr_emul_typer_access(&self, emu_ctx: &EmuContext, vgicr_id: usize) {
        let cpu_priv = self.cpu_priv.lock();
        if !emu_ctx.write {
            current_cpu().set_gpr(emu_ctx.reg, cpu_priv[vgicr_id].vigcr.get_typer() as usize);
        }
    }

    fn vgicd_set_irouter(&self, vcpu: Vcpu, int_id: usize, val: usize) {
        if let Some(interrupt) = self.get_int(vcpu.clone(), int_id) {
            let interrupt_lock = interrupt.lock.lock();

            if vgic_int_get_owner(vcpu.clone(), interrupt.clone()) {
                self.remove_lr(vcpu.clone(), interrupt.clone());

                let phys_route = if (val & GICD_IROUTER_IRM_BIT) != 0 {
                    cpuid2mpidr(vcpu.phys_id())
                } else {
                    match vcpu.vm().unwrap().get_vcpu_by_mpidr(val & MPIDR_AFF_MSK) {
                        Some(vcpu) => cpuid2mpidr(vcpu.phys_id()) & MPIDR_AFF_MSK,
                        _ => GICD_IROUTER_INV,
                    }
                };
                interrupt.set_phys_route(phys_route);
                interrupt.set_route(val & GICD_IROUTER_RES0_MSK);
                if interrupt.hw() {
                    GICD.set_route(int_id, phys_route);
                }
                self.route(vcpu.clone(), interrupt.clone());
                vgic_int_yield_owner(vcpu.clone(), interrupt.clone());
            } else {
                let m = IpiInitcMessage {
                    event: InitcEvent::VgicdRoute,
                    vm_id: vcpu.vm().unwrap().id(),
                    int_id: interrupt.id(),
                    val: val as u8,
                };
                if !ipi_send_msg(
                    interrupt.owner().unwrap().phys_id(),
                    IpiType::IpiTIntc,
                    IpiInnerMsg::Initc(m),
                ) {
                    print!(
                        "vgicd_set_irouter: Failed to send ipi message, target {} type {}",
                        interrupt.owner().unwrap().phys_id(),
                        0
                    );
                }
            }
            drop(interrupt_lock);
        }
    }

    fn cpu_priv_interrupt(&self, cpu_id: usize, idx: usize) -> VgicInt {
        let cpu_priv = self.cpu_priv.lock();
        cpu_priv[cpu_id].interrupts[idx].clone()
    }

    fn cpu_priv_curr_lrs(&self, cpu_id: usize, idx: usize) -> u16 {
        let cpu_priv = self.cpu_priv.lock();
        cpu_priv[cpu_id].curr_lrs[idx]
    }

    fn cpu_priv_sgis_pend(&self, cpu_id: usize, idx: usize) -> u8 {
        let cpu_priv = self.cpu_priv.lock();
        cpu_priv[cpu_id].sgis[idx].pend
    }

    fn cpu_priv_sgis_act(&self, cpu_id: usize, idx: usize) -> u8 {
        let cpu_priv = self.cpu_priv.lock();
        cpu_priv[cpu_id].sgis[idx].act
    }

    fn set_cpu_priv_curr_lrs(&self, cpu_id: usize, idx: usize, val: u16) {
        let mut cpu_priv = self.cpu_priv.lock();
        cpu_priv[cpu_id].curr_lrs[idx] = val;
    }

    fn set_cpu_priv_sgis_pend(&self, cpu_id: usize, idx: usize, pend: u8) {
        let mut cpu_priv = self.cpu_priv.lock();
        cpu_priv[cpu_id].sgis[idx].pend = pend;
    }

    fn set_cpu_priv_sgis_act(&self, cpu_id: usize, idx: usize, act: u8) {
        let mut cpu_priv = self.cpu_priv.lock();
        cpu_priv[cpu_id].sgis[idx].act = act;
    }

    fn vgicd_interrupt(&self, idx: usize) -> VgicInt {
        let vgicd = self.vgicd.lock();
        vgicd.interrupts[idx].clone()
    }

    fn get_int(&self, vcpu: Vcpu, int_id: usize) -> Option<VgicInt> {
        if int_id < GIC_PRIVINT_NUM {
            let vcpu_id = vcpu.id();
            return Some(self.cpu_priv_interrupt(vcpu_id, int_id));
        } else if (GIC_PRIVINT_NUM..GIC_INTS_MAX).contains(&int_id) {
            return Some(self.vgicd_interrupt(int_id - GIC_PRIVINT_NUM));
        }
        None
    }

    fn remove_lr(&self, vcpu: Vcpu, interrupt: VgicInt) -> bool {
        if !vgic_owns(vcpu.clone(), interrupt.clone()) {
            return false;
        }
        let int_lr = interrupt.lr();

        if !interrupt.in_lr() {
            return false;
        }

        let mut lr_val = 0;
        if let Some(lr) = gich_get_lr(interrupt.clone()) {
            GICH.set_lr(int_lr as usize, 0);
            lr_val = lr;
        }

        interrupt.set_in_lr(false);

        let lr_state = bit_extract(lr_val, GICH_LR_STATE_OFF, GICH_LR_STATE_LEN);
        if lr_state != IrqState::IrqSInactive.to_num() {
            interrupt.set_state(IrqState::num_to_state(lr_state));

            self.update_int_list(vcpu.clone(), interrupt.clone());

            if (interrupt.state().to_num() & IrqState::IrqSPend.to_num() != 0) && interrupt.enabled() {
                let hcr = GICH.hcr();
                GICH.set_hcr(hcr | GICH_HCR_NPIE_BIT);
            }
            return true;
        }
        false
    }

    fn add_lr(&self, vcpu: Vcpu, interrupt: VgicInt) -> bool {
        if !interrupt.enabled() || interrupt.in_lr() {
            return false;
        }

        let gic_lrs = gic_lrs();
        let mut lr_ind = None;

        let elrsr = GICH.elrsr();
        //look for empty lr for using whit ICH_ELRSR_EL2
        for i in 0..gic_lrs {
            if bit_get(elrsr, i % 32) != 0 {
                lr_ind = Some(i);
                break;
            }
        }

        // if there is no empty then, replace one
        if lr_ind.is_none() {
            let mut pend_found = 0;
            // let mut act_found = 0;
            let mut min_prio_act = interrupt.prio() as usize;
            let mut min_prio_pend = interrupt.prio() as usize;
            let mut min_id_act = interrupt.id() as usize;
            let mut min_id_pend = interrupt.id() as usize;
            let mut act_ind = None;
            let mut pend_ind = None;

            for i in 0..gic_lrs {
                let lr = GICH.lr(i);
                let lr_prio = (lr & GICH_LR_PRIO_MSK) >> GICH_LR_PRIO_OFF;
                let lr_state = lr & GICH_LR_STATE_MSK;
                let lr_id = (lr & GICH_LR_VID_MASK) >> GICH_LR_VID_OFF;

                // look for min_prio act/pend lr (the value bigger then prio smaller)
                if lr_state & GICH_LR_STATE_ACT != 0 {
                    if lr_prio > min_prio_act || (lr_prio == min_prio_act && lr_id > min_id_act) {
                        min_id_act = lr_id;
                        min_prio_act = lr_prio;
                        act_ind = Some(i);
                    }
                    // act_found += 1;
                } else if lr_state & GICH_LR_STATE_PND != 0 {
                    if lr_prio > min_prio_pend || (lr_prio == min_prio_pend && lr_id > min_id_pend) {
                        min_id_pend = lr_id;
                        min_prio_pend = lr_prio;
                        pend_ind = Some(i);
                    }
                    pend_found += 1;
                }
            }

            // replace pend first
            if pend_found > 1 {
                lr_ind = pend_ind;
            } else {
                lr_ind = act_ind;
            }

            if let Some(idx) = lr_ind {
                if let Some(spilled_int) = &self.get_int(
                    vcpu.clone(),
                    bit_extract(GICH.lr(idx), GICH_LR_VID_OFF, GICH_LR_VID_LEN),
                ) {
                    if spilled_int.id() != interrupt.id() {
                        let spilled_int_lock = spilled_int.lock.lock();
                        self.remove_lr(vcpu.clone(), spilled_int.clone());
                        vgic_int_yield_owner(vcpu.clone(), spilled_int.clone());
                        drop(spilled_int_lock);
                    } else {
                        self.remove_lr(vcpu.clone(), spilled_int.clone());
                        vgic_int_yield_owner(vcpu.clone(), spilled_int.clone());
                    }
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
                if vgic_get_state(interrupt) & IrqState::IrqSPend.to_num() != 0 {
                    let hcr = GICH.hcr();
                    //No Pending Interrupt Enable. Enables the signaling of a maintenance interrupt when there are no List registers with the State field set to 0b01
                    // then a maintance interrupt will come
                    GICH.set_hcr(hcr | GICH_HCR_NPIE_BIT);
                }
            }
        }

        false
    }

    fn write_lr(&self, vcpu: Vcpu, interrupt: VgicInt, lr_ind: usize) {
        let vcpu_id = vcpu.id();
        let int_id = interrupt.id() as usize;
        let int_prio = interrupt.prio();

        let prev_int_id = self.cpu_priv_curr_lrs(vcpu_id, lr_ind) as usize;
        if prev_int_id != int_id && !gic_is_priv(prev_int_id) {
            if let Some(prev_interrupt) = self.get_int(vcpu.clone(), prev_int_id) {
                let prev_interrupt_lock = prev_interrupt.lock.lock();
                if vgic_owns(vcpu.clone(), prev_interrupt.clone())
                    && prev_interrupt.in_lr()
                    && (prev_interrupt.lr() == lr_ind as u16)
                {
                    prev_interrupt.set_in_lr(false);
                    vgic_int_yield_owner(vcpu.clone(), prev_interrupt.clone());
                }
                drop(prev_interrupt_lock);
            }
        }

        let state = vgic_get_state(interrupt.clone());

        let mut lr = (int_id << GICH_LR_VID_OFF) & GICH_LR_VID_MASK;
        lr |= ((int_prio as usize) << GICH_LR_PRIO_OFF) & GICH_LR_PRIO_MSK;

        if vgic_int_is_hw(interrupt.clone()) {
            lr |= GICH_LR_HW_BIT;
            lr |= (int_id << GICH_LR_PID_OFF) & GICH_LR_PID_MSK;
            if state == IrqState::IrqSPendActive.to_num() {
                lr |= GICH_LR_STATE_ACT;
            } else {
                lr |= (state << GICH_LR_STATE_OFF) & GICH_LR_STATE_MSK;
            }
        } else {
            if !gic_is_priv(int_id) && !vgic_int_is_hw(interrupt.clone()) {
                lr |= GICH_LR_EOI_BIT;
            }

            lr |= (state << GICH_LR_STATE_OFF) & GICH_LR_STATE_MSK;
        }

        /*
         * When the guest is using vGICv3, all the IRQs are Group 1. Group 0
         * would result in a FIQ, which will not be expected by the guest OS.
         */
        lr |= GICH_LR_GRP_BIT;

        interrupt.set_state(IrqState::IrqSInactive);
        interrupt.set_in_lr(true);
        interrupt.set_lr(lr_ind as u16);
        self.set_cpu_priv_curr_lrs(vcpu_id, lr_ind, int_id as u16);

        GICH.set_lr(lr_ind, lr);
        self.update_int_list(vcpu, interrupt);
    }

    fn route(&self, vcpu: Vcpu, interrupt: VgicInt) {
        if let IrqState::IrqSInactive = interrupt.state() {
            return;
        }

        if !interrupt.enabled() {
            return;
        }

        if vgic_int_vcpu_is_target(&vcpu, &interrupt) {
            self.add_lr(vcpu.clone(), interrupt.clone());
        }

        if !interrupt.in_lr() && vgic_int_has_other_target(interrupt.clone()) {
            let vcpu_vm_id = vcpu.vm_id();
            let ipi_msg = IpiInitcMessage {
                event: InitcEvent::VgicdRoute,
                vm_id: vcpu_vm_id,
                int_id: interrupt.id(),
                val: 0,
            };
            vgic_int_yield_owner(vcpu.clone(), interrupt.clone());
            let trglist = vgic_int_ptarget_mask(interrupt) & !(1 << vcpu.phys_id());
            for i in 0..PLAT_DESC.cpu_desc.num {
                if trglist & (1 << i) != 0 {
                    ipi_send_msg(i, IpiType::IpiTIntc, IpiInnerMsg::Initc(ipi_msg));
                }
            }
        }
    }

    fn set_enable(&self, vcpu: Vcpu, int_id: usize, en: bool) {
        match self.get_int(vcpu.clone(), int_id) {
            Some(interrupt) => {
                let interrupt_lock = interrupt.lock.lock();
                if vgic_int_get_owner(vcpu.clone(), interrupt.clone()) {
                    if interrupt.enabled() ^ en {
                        interrupt.set_enabled(en);
                        self.remove_lr(vcpu.clone(), interrupt.clone());
                        if interrupt.hw() {
                            if gic_is_priv(interrupt.id() as usize) {
                                GICR.set_enable(interrupt.id() as usize, en, interrupt.phys_redist() as u32);
                            } else {
                                GICD.set_enable(interrupt.id() as usize, en);
                            }
                        }
                    }
                    self.route(vcpu.clone(), interrupt.clone());
                    vgic_int_yield_owner(vcpu, interrupt.clone());
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

    fn get_enable(&self, vcpu: Vcpu, int_id: usize) -> bool {
        self.get_int(vcpu, int_id).unwrap().enabled()
    }

    fn set_pend(&self, vcpu: Vcpu, int_id: usize, pend: bool) {
        if let Some(interrupt) = self.get_int(vcpu.clone(), int_id) {
            let interrupt_lock = interrupt.lock.lock();
            if vgic_int_get_owner(vcpu.clone(), interrupt.clone()) {
                self.remove_lr(vcpu.clone(), interrupt.clone());

                let state = interrupt.state().to_num();
                if pend && ((state & 1) == 0) {
                    interrupt.set_state(IrqState::num_to_state(state | 1));
                } else if !pend && (state & 1) != 0 {
                    interrupt.set_state(IrqState::num_to_state(state & !1));
                }

                let state = interrupt.state().to_num();
                if interrupt.hw() {
                    if gic_is_priv(int_id) {
                        gic_set_state(interrupt.id() as usize, state, interrupt.phys_redist() as u32);
                    } else {
                        // GICD non`t need gicr_id
                        gic_set_state(interrupt.id() as usize, state, 0);
                    }
                }
                self.route(vcpu.clone(), interrupt.clone());
                vgic_int_yield_owner(vcpu, interrupt.clone());
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
            drop(interrupt_lock);
        }
    }

    fn set_active(&self, vcpu: Vcpu, int_id: usize, act: bool) {
        let interrupt_option = self.get_int(vcpu.clone(), bit_extract(int_id, 0, 10));
        if let Some(interrupt) = interrupt_option {
            let interrupt_lock = interrupt.lock.lock();
            if vgic_int_get_owner(vcpu.clone(), interrupt.clone()) {
                self.remove_lr(vcpu.clone(), interrupt.clone());
                let state = interrupt.state().to_num();
                if act && ((state & IrqState::IrqSActive.to_num()) == 0) {
                    interrupt.set_state(IrqState::num_to_state(state | IrqState::IrqSActive.to_num()));
                } else if !act && (state & IrqState::IrqSActive.to_num()) != 0 {
                    interrupt.set_state(IrqState::num_to_state(state & !IrqState::IrqSActive.to_num()));
                }
                let state = interrupt.state().to_num();
                if interrupt.hw() {
                    let vgic_int_id = interrupt.id() as usize;
                    if gic_is_priv(vgic_int_id) {
                        gic_set_state(
                            vgic_int_id,
                            if state == 1 { 2 } else { state },
                            interrupt.phys_redist() as u32,
                        );
                    } else {
                        gic_set_state(vgic_int_id, if state == 1 { 2 } else { state }, 0);
                    }
                }
                self.route(vcpu.clone(), interrupt.clone());
                vgic_int_yield_owner(vcpu, interrupt.clone());
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

    fn set_icfgr(&self, vcpu: Vcpu, int_id: usize, cfg: u8) {
        if let Some(interrupt) = self.get_int(vcpu.clone(), int_id) {
            let interrupt_lock = interrupt.lock.lock();
            if vgic_int_get_owner(vcpu.clone(), interrupt.clone()) {
                interrupt.set_cfg(cfg);
                if interrupt.hw() {
                    if gic_is_priv(interrupt.id() as usize) {
                        GICR.set_icfgr(interrupt.id() as usize, cfg, interrupt.phys_redist() as u32);
                    } else {
                        GICD.set_icfgr(interrupt.id() as usize, cfg);
                    }
                }
                self.route(vcpu.clone(), interrupt.clone());
                vgic_int_yield_owner(vcpu, interrupt.clone());
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

    fn get_icfgr(&self, vcpu: Vcpu, int_id: usize) -> u8 {
        let interrupt_option = self.get_int(vcpu, int_id);
        if let Some(interrupt) = interrupt_option {
            interrupt.cfg()
        } else {
            unimplemented!();
        }
    }

    fn set_prio(&self, vcpu: Vcpu, int_id: usize, mut prio: u8) {
        let interrupt_option = self.get_int(vcpu.clone(), int_id);
        prio &= 0xf0; // gicv3 allows 8 priority bits in non-secure state

        if let Some(interrupt) = interrupt_option {
            let interrupt_lock = interrupt.lock.lock();
            if vgic_int_get_owner(vcpu.clone(), interrupt.clone()) {
                if interrupt.prio() != prio {
                    self.remove_lr(vcpu.clone(), interrupt.clone());
                    let prev_prio = interrupt.prio();
                    interrupt.set_prio(prio);
                    if prio <= prev_prio {
                        self.route(vcpu.clone(), interrupt.clone());
                    }
                    if interrupt.hw() {
                        if gic_is_priv(interrupt.id() as usize) {
                            GICR.set_prio(interrupt.id() as usize, prio, interrupt.phys_redist() as u32);
                        } else {
                            GICD.set_prio(interrupt.id() as usize, prio);
                        }
                    }
                }
                vgic_int_yield_owner(vcpu, interrupt.clone());
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

    fn get_prio(&self, vcpu: Vcpu, int_id: usize) -> u8 {
        let interrupt_option = self.get_int(vcpu, int_id);
        interrupt_option.unwrap().prio()
    }

    pub fn inject(&self, vcpu: Vcpu, int_id: usize) {
        let interrupt_option = self.get_int(vcpu.clone(), bit_extract(int_id, 0, 10));
        if let Some(interrupt) = interrupt_option {
            if interrupt.hw() {
                let interrupt_lock = interrupt.lock.lock();
                interrupt.set_owner(vcpu.clone());
                interrupt.set_state(IrqState::IrqSPend);
                self.update_int_list(vcpu.clone(), interrupt.clone());
                interrupt.set_in_lr(false);
                self.route(vcpu, interrupt.clone());
                drop(interrupt_lock);
            } else {
                self.set_pend(vcpu, int_id, true);
            }
        }
    }

    fn emu_razwi(&self, emu_ctx: &EmuContext) {
        if !emu_ctx.write {
            current_cpu().set_gpr(emu_ctx.reg, 0);
        }
    }

    fn emu_irouter_access(&self, emu_ctx: &EmuContext) {
        let first_int = (bit_extract(emu_ctx.address, 0, 16) - 0x6000) / 8;
        let idx = emu_ctx.reg;
        let mut val = if emu_ctx.write { current_cpu().get_gpr(idx) } else { 0 };

        if emu_ctx.write {
            self.vgicd_set_irouter(current_cpu().active_vcpu.clone().unwrap(), first_int, val);
        } else {
            if !gic_is_priv(first_int) {
                val = self
                    .get_int(current_cpu().active_vcpu.clone().unwrap(), first_int)
                    .unwrap()
                    .route() as usize;
            }
            current_cpu().set_gpr(idx, val);
        }
    }

    fn emu_ctrl_access(&self, emu_ctx: &EmuContext) {
        if emu_ctx.write {
            let prev_ctlr = self.vgicd_ctlr();
            let idx = emu_ctx.reg;
            self.set_vgicd_ctlr(current_cpu().get_gpr(idx) as u32 & 0x2 | GICD_CTLR_ARE_NS_BIT as u32);
            if prev_ctlr ^ self.vgicd_ctlr() != 0 {
                let enable = self.vgicd_ctlr() != 0;
                let hcr = GICH.hcr();
                if enable {
                    GICH.set_hcr(hcr | GICH_HCR_EN_BIT);
                } else {
                    GICH.set_hcr(hcr & !GICH_HCR_EN_BIT);
                }

                let m = IpiInitcMessage {
                    event: InitcEvent::VgicdGichEn,
                    vm_id: active_vm_id(),
                    int_id: 0,
                    val: enable as u8,
                };
                ipi_intra_broadcast_msg(active_vm().unwrap(), IpiType::IpiTIntc, IpiInnerMsg::Initc(m));
            }
        } else {
            let idx = emu_ctx.reg;
            let val = self.vgicd_ctlr() as usize;
            current_cpu().set_gpr(idx, val | GICD.ctlr() as usize);
        }
    }

    fn emu_typer_access(&self, emu_ctx: &EmuContext) {
        if !emu_ctx.write {
            let idx = emu_ctx.reg;
            let val = self.vgicd_typer() as usize;
            current_cpu().set_gpr(idx, val);
        } else {
            error!("emu_typer_access: can't write to RO reg");
        }
    }

    fn emu_iidr_access(&self, emu_ctx: &EmuContext) {
        if !emu_ctx.write {
            let idx = emu_ctx.reg;
            let val = self.vgicd_iidr() as usize;
            current_cpu().set_gpr(idx, val);
        } else {
            error!("emu_iidr_access: can't write to RO reg");
        }
    }

    fn emu_isenabler_access(&self, emu_ctx: &EmuContext) {
        let first_int = ((emu_ctx.address & 0xffff) - 0x100) * 8; //emu_ctx.address - offsetof(GICD,ISENABLER)
        let idx = emu_ctx.reg;
        let mut val = if emu_ctx.write { current_cpu().get_gpr(idx) } else { 0 };

        let vm_id = active_vm_id();
        let vm = match active_vm() {
            Some(vm) => vm,
            None => {
                panic!("emu_isenabler_access: current vcpu.vm is none");
            }
        };
        let mut vm_has_interrupt_flag = false;

        for i in 0..(emu_ctx.width * 8) {
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
            for i in 0..(emu_ctx.width * 8) {
                if bit_get(val, i) != 0 {
                    self.set_enable(current_cpu().active_vcpu.clone().unwrap(), first_int + i, true);
                }
            }
        } else {
            for i in 0..(emu_ctx.width * 8) {
                if self.get_enable(current_cpu().active_vcpu.clone().unwrap(), first_int + i) {
                    val |= 1 << i;
                }
            }
            let idx = emu_ctx.reg;
            current_cpu().set_gpr(idx, val);
        }
    }

    fn emu_pendr_access(&self, emu_ctx: &EmuContext, set: bool) {
        let first_int = if set {
            // ISPEND  emu_ctx.address - OFFSET(GICD/R,ISPENDR)
            ((emu_ctx.address & 0xffff) - 0x0200) * 8
        } else {
            // ICPEND  emu_ctx.address - OFFSET(GICD/R,ICPENDR)
            ((emu_ctx.address & 0xffff) - 0x0280) * 8
        };

        let idx = emu_ctx.reg;
        let mut val = if emu_ctx.write { current_cpu().get_gpr(idx) } else { 0 };

        if emu_ctx.write {
            for i in 0..(emu_ctx.width * 8) {
                if bit_get(val, i) != 0 {
                    self.set_pend(current_cpu().active_vcpu.clone().unwrap(), first_int + i, set);
                }
            }
        } else {
            for i in 0..32 {
                match self.get_int(current_cpu().active_vcpu.clone().unwrap(), first_int + i) {
                    Some(interrupt) => {
                        if vgic_get_state(interrupt.clone()) & IrqState::IrqSPend.to_num()
                            != IrqState::IrqSInactive.to_num()
                        {
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

    fn emu_pidr_access(&self, emu_ctx: &EmuContext) {
        if !emu_ctx.write {
            current_cpu().set_gpr(emu_ctx.reg, GICD.id(((emu_ctx.address & 0xff) - 0xd0) / 4) as usize);
        }
    }

    fn emu_ispendr_access(&self, emu_ctx: &EmuContext) {
        self.emu_pendr_access(emu_ctx, true);
    }

    fn emu_activer_access(&self, emu_ctx: &EmuContext, set: bool) {
        let first_int = if set {
            // ISACTIVE (emu_ctx.address - OFFSET(GICD/R, ISACTIVER)
            8 * ((emu_ctx.address & 0xffff) - 0x0300)
        } else {
            // ICACTIVE (emu_ctx.address - OFFSET(GICD/R, ICACTIVER)
            8 * ((emu_ctx.address & 0xffff) - 0x0380)
        };
        let idx = emu_ctx.reg;

        let mut val = if emu_ctx.write { current_cpu().get_gpr(idx) } else { 0 };
        let vm_id = active_vm_id();
        let vm = match active_vm() {
            Some(vm) => vm,
            None => {
                panic!("emu_activer_access: current vcpu.vm is none");
            }
        };
        let mut vm_has_interrupt_flag = false;

        for i in 0..(emu_ctx.width * 8) {
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
            for i in 0..(emu_ctx.width * 8) {
                if bit_get(val, i) != 0 {
                    self.set_active(current_cpu().active_vcpu.clone().unwrap(), first_int + i, set);
                }
            }
        } else {
            for i in 0..(emu_ctx.width * 8) {
                match self.get_int(current_cpu().active_vcpu.clone().unwrap(), first_int + i) {
                    Some(interrupt) => {
                        if vgic_get_state(interrupt.clone()) & IrqState::IrqSActive.to_num() != 0 {
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
        let first_int = ((emu_ctx.address & 0xffff) - 0x0180) * 8; //emu_ctx.address - OFFSET(GICR/D,ICENABLE)
        let idx = emu_ctx.reg;
        let mut val = if emu_ctx.write { current_cpu().get_gpr(idx) } else { 0 };

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
            for i in 0..(emu_ctx.width * 8) {
                if bit_get(val, i) != 0 {
                    self.set_enable(current_cpu().active_vcpu.clone().unwrap(), first_int + i, false);
                }
            }
        } else {
            for i in 0..(emu_ctx.width * 8) {
                if self.get_enable(current_cpu().active_vcpu.clone().unwrap(), first_int + i) {
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

    fn emu_probaser_access(&self, emu_ctx: &EmuContext) {
        if emu_ctx.write {
            GICR.set_propbaser(current_cpu().id, current_cpu().get_gpr(emu_ctx.reg));
        } else {
            current_cpu().set_gpr(emu_ctx.reg, GICR.get_propbaser(current_cpu().id) as usize);
        }
    }

    fn emu_pendbaser_access(&self, emu_ctx: &EmuContext) {
        if emu_ctx.write {
            GICR.set_pendbaser(current_cpu().id, current_cpu().get_gpr(emu_ctx.reg));
        } else {
            current_cpu().set_gpr(emu_ctx.reg, GICR.get_pendbaser(current_cpu().id) as usize);
        }
    }

    fn emu_icfgr_access(&self, emu_ctx: &EmuContext) {
        let first_int = ((emu_ctx.address & 0xffff) - 0x0C00) * 8 / GIC_CONFIG_BITS; // emu_ctx.address - OFFSET(GICR/D,ICFGR)

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
            while bit < (emu_ctx.width * 8) {
                self.set_icfgr(
                    current_cpu().active_vcpu.clone().unwrap(),
                    irq,
                    bit_extract(cfg as usize, bit, GIC_CONFIG_BITS) as u8,
                );
                bit += 2;
                irq += 1;
            }
        } else {
            let mut cfg = 0;
            let mut irq = first_int;
            let mut bit = 0;
            while bit < (emu_ctx.width * 8) {
                cfg |= (self.get_icfgr(current_cpu().active_vcpu.clone().unwrap(), irq) as usize) << bit;
                bit += 2;
                irq += 1;
            }
            let idx = emu_ctx.reg;
            current_cpu().set_gpr(idx, cfg);
        }
    }

    fn emu_ipriorityr_access(&self, emu_ctx: &EmuContext) {
        let first_int = ((emu_ctx.address & 0xffff) - 0x0400) * 8 / GIC_PRIO_BITS; // emu_ctx.address - OFFSET(GICR/D,IPRIORITYR)
        let idx = emu_ctx.reg;
        let mut val = if emu_ctx.write { current_cpu().get_gpr(idx) } else { 0 };

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
                    current_cpu().active_vcpu.clone().unwrap(),
                    first_int + i,
                    bit_extract(val, GIC_PRIO_BITS * i, GIC_PRIO_BITS) as u8,
                );
            }
        } else {
            for i in 0..emu_ctx.width {
                val |= (self.get_prio(current_cpu().active_vcpu.clone().unwrap(), first_int + i) as usize)
                    << (GIC_PRIO_BITS * i);
            }
            let idx = emu_ctx.reg;
            current_cpu().set_gpr(idx, val);
        }
    }

    fn handle_trapped_eoir(&self, vcpu: Vcpu) {
        let gic_lrs = gic_lrs();
        // eisr():Interrupt Controller End of Interrupt Status Register
        let mut lr_idx_opt = bitmap_find_nth(GICH.eisr() as usize, 0, gic_lrs, 1, true);

        while lr_idx_opt.is_some() {
            let lr_idx = lr_idx_opt.unwrap();
            let lr_val = GICH.lr(lr_idx) as usize;
            GICH.set_lr(lr_idx, 0);

            match self.get_int(vcpu.clone(), bit_extract(lr_val, GICH_LR_VID_OFF, GICH_LR_VID_LEN)) {
                Some(interrupt) => {
                    let interrupt_lock = interrupt.lock.lock();
                    interrupt.set_in_lr(false);
                    if (interrupt.id() as usize) < GIC_SGIS_NUM {
                        self.add_lr(vcpu.clone(), interrupt.clone());
                    } else {
                        vgic_int_yield_owner(vcpu.clone(), interrupt.clone());
                    }
                    drop(interrupt_lock);
                }
                None => {
                    continue;
                }
            }
            lr_idx_opt = bitmap_find_nth(GICH.eisr() as usize, 0, gic_lrs, 1, true);
        }
    }

    fn vgic_highest_proi_spilled(&self, vcpu: &Vcpu, flag: bool) -> Option<VgicInt> {
        let cpu_priv = &self.cpu_priv.lock()[vcpu.id()];
        // info!("binding list_len:{}", binding.len());
        let array = [
            Some(cpu_priv.pend_list.iter()),
            if flag { None } else { Some(cpu_priv.act_list.iter()) },
        ];
        let binding = array.into_iter().flatten().flatten();
        binding
            .min_by_key(|x| (((x.prio() as u32) << 10) | x.id() as u32))
            .cloned()
    }

    fn refill_lrs(&self, vcpu: Vcpu, flag: bool) {
        let gic_lrs = gic_lrs();
        // ICH_ELRSR_EL2:locate a usable List register when the hypervisor is delivering an interrupt to a Guest OS.
        let mut lr_idx_opt = bitmap_find_nth(GICH.elrsr(), 0, gic_lrs, 1, true);
        // flag indicates that is no pending or not true:no pending flase:have pending,the we will look up active and pend
        let mut new_flags = flag;
        while lr_idx_opt.is_some() {
            let interrupt_opt: Option<VgicInt> = self.vgic_highest_proi_spilled(&vcpu, new_flags);

            match interrupt_opt {
                Some(interrupt) => {
                    let interrupt_lock = interrupt.lock.lock();
                    let got_ownership = vgic_int_get_owner(vcpu.clone(), interrupt.clone());
                    if got_ownership {
                        self.write_lr(vcpu.clone(), interrupt.clone(), lr_idx_opt.unwrap());
                    }
                    drop(interrupt_lock);
                    if !got_ownership {
                        continue;
                    }
                }
                None => {
                    let hcr = GICH.hcr();
                    GICH.set_hcr(hcr & !(GICH_HCR_NPIE_BIT | GICH_HCR_UIE_BIT));
                    break;
                }
            }

            new_flags = false;
            lr_idx_opt = bitmap_find_nth(GICH.elrsr(), 0, gic_lrs, 1, true);
        }
    }

    fn eoir_highest_spilled_active(&self, vcpu: Vcpu) {
        let cpu_priv = self.cpu_priv.lock();
        let binding = &cpu_priv[vcpu.id()].act_list;
        let interrupt = binding
            .iter()
            .min_by_key(|x| (((x.prio() as u32) << 10) | x.id() as u32))
            .cloned();
        drop(cpu_priv);
        if let Some(int) = interrupt {
            int.lock.lock();
            vgic_int_get_owner(vcpu.clone(), int.clone());

            let state = int.state().to_num();
            int.set_state(IrqState::num_to_state(state & !2));
            self.update_int_list(vcpu.clone(), int.clone());

            if vgic_int_is_hw(int.clone()) {
                gic_set_act(int.id() as usize, false, current_cpu().id as u32);
            } else if int.state().to_num() & 1 != 0 {
                self.add_lr(vcpu, int.clone());
            }
        }
    }
}

#[inline(always)]
fn vgic_broadcast(interrupt: VgicInt) -> bool {
    (interrupt.route() as usize & GICD_IROUTER_IRM_BIT) != 0
}

#[inline(always)]
fn vgicr_get_id(emu_ctx: &EmuContext) -> u32 {
    ((emu_ctx.address - PLAT_DESC.arch_desc.gic_desc.gicr_addr) / size_of::<GicRedistributor>()) as u32
}

fn vgicr_emul_ctrl_access(emu_ctx: &EmuContext) {
    if !emu_ctx.write {
        current_cpu().set_gpr(emu_ctx.reg, GICR.get_ctrl(current_cpu().id as u32) as usize);
    } else {
        GICR.set_ctrlr(current_cpu().id, current_cpu().get_gpr(emu_ctx.reg));
    }
}

fn vgicr_emul_pidr_access(emu_ctx: &EmuContext, vgicr_id: usize) {
    if !emu_ctx.write {
        let pgicr_id = current_cpu()
            .active_vcpu
            .clone()
            .unwrap()
            .vm()
            .unwrap()
            .vcpuid_to_pcpuid(vgicr_id);
        if let Ok(pgicr_id) = pgicr_id {
            current_cpu().set_gpr(
                emu_ctx.reg,
                GICR.get_id(pgicr_id as u32, ((emu_ctx.address & 0xff) - 0xd0) / 4) as usize,
            );
        }
    }
}

fn vgic_int_vcpu_is_target(vcpu: &Vcpu, interrupt: &VgicInt) -> bool {
    let pri = gic_is_priv(interrupt.id() as usize);
    let local = pri && (interrupt.phys_redist() as usize == vcpu.phys_id());
    let mut res: u64;
    mrs!(res, MPIDR_EL1);
    let routed_here = !pri && (interrupt.phys_route() as usize ^ (res as usize & MPIDR_AFF_MSK)) == 0;
    let any = !pri && vgic_broadcast(interrupt.clone());

    local || routed_here || any
}

fn vgic_int_has_other_target(interrupt: VgicInt) -> bool {
    let pri = gic_is_priv(interrupt.id() as usize);
    if pri {
        return false;
    }
    let mut res: u64;
    mrs!(res, MPIDR_EL1);

    let routed_here = !pri && (interrupt.phys_route() as usize ^ (res as usize & MPIDR_AFF_MSK)) == 0;
    let route_valid = interrupt.phys_route() as usize != GICD_IROUTER_INV;
    let any = !pri && vgic_broadcast(interrupt.clone());

    any || (!routed_here && route_valid)
}

fn vgic_int_ptarget_mask(interrupt: VgicInt) -> usize {
    if vgic_broadcast(interrupt.clone()) {
        current_cpu().active_vcpu.clone().unwrap().vm().clone().unwrap().ncpu() & !(1 << current_cpu().id)
    } else if cfg!(feature = "rk3588") {
        (1 << (interrupt.phys_route() >> 8)) as usize
    } else {
        (1 << interrupt.phys_route()) as usize
    }
}

fn vgic_target_translate(vm: Vm, trgt: u32, v2p: bool) -> u32 {
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

fn vgic_owns(vcpu: Vcpu, interrupt: VgicInt) -> bool {
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

fn vgic_get_state(interrupt: VgicInt) -> usize {
    let mut state = interrupt.state().to_num();

    if interrupt.in_lr() && interrupt.owner_phys_id().unwrap() == current_cpu().id {
        let lr_option = gich_get_lr(interrupt.clone());
        if let Some(lr_val) = lr_option {
            state = bit_extract(lr_val, GICH_LR_STATE_OFF, GICH_LR_STATE_LEN);
        }
    }

    state
}

fn vgic_int_yield_owner(vcpu: Vcpu, interrupt: VgicInt) {
    if !vgic_owns(vcpu, interrupt.clone())
        || interrupt.in_lr()
        || (vgic_get_state(interrupt.clone()) & IrqState::IrqSActive.to_num() != 0)
    {
        return;
    }

    interrupt.clear_owner();
}

#[inline(always)]
fn vgic_int_is_hw(interrupt: VgicInt) -> bool {
    interrupt.id() as usize >= GIC_SGIS_NUM && interrupt.hw()
}

fn gich_get_lr(interrupt: VgicInt) -> Option<usize> {
    let cpu_id = current_cpu().id;
    let phys_id = interrupt.owner_phys_id().unwrap();

    if !interrupt.in_lr() || phys_id != cpu_id {
        return None;
    }

    let lr_val = GICH.lr(interrupt.lr() as usize);
    if (bit_extract(lr_val, GICH_LR_VID_OFF, GICH_LR_VID_LEN) == interrupt.id() as usize)
        && (bit_extract(lr_val, GICH_LR_STATE_OFF, GICH_LR_STATE_LEN) != IrqState::IrqSInactive.to_num())
    {
        return Some(lr_val);
    }
    None
}

fn vgic_int_get_owner(vcpu: Vcpu, interrupt: VgicInt) -> bool {
    let vcpu_id = vcpu.id();
    let vcpu_vm_id = vcpu.vm_id();

    match interrupt.owner() {
        Some(owner) => {
            let owner_vcpu_id = owner.id();
            let owner_vm_id = owner.vm_id();

            owner_vm_id == vcpu_vm_id && owner_vcpu_id == vcpu_id
        }
        None => {
            interrupt.set_owner(vcpu);
            true
        }
    }
}

pub fn gic_maintenance_handler(_arg: usize) {
    let misr = GICH.misr();
    let vm = match active_vm() {
        Some(vm) => vm,
        None => {
            panic!("gic_maintenance_handler: current vcpu.vm is None");
        }
    };
    let vgic = vm.vgic();

    if misr & (GICH_MISR_EOI as u32) != 0 {
        vgic.handle_trapped_eoir(current_cpu().active_vcpu.clone().unwrap());
    }

    // NP:List Register Entry Not Present.
    // U: underflow Zero or one of the List register entries are marked as a valid interrupt, that is, if the corresponding ICH_LR<n>_EL2.State bits do not equal 0x0.
    if misr & (GICH_MISR_NP as u32 | GICH_MISR_U as u32) != 0 {
        vgic.refill_lrs(
            current_cpu().active_vcpu.clone().unwrap(),
            (misr & GICH_MISR_NP as u32) != 0,
        );
    }

    if misr & (GICH_MISR_LRPEN as u32) != 0 {
        let mut hcr = GICH.hcr();
        while hcr & GICH_HCR_EOIC_MSK != 0 {
            vgic.eoir_highest_spilled_active(current_cpu().active_vcpu.clone().unwrap());
            hcr -= 1 << GICH_HCR_EOIC_OFF;
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

pub fn emu_intc_handler(_emu_dev_id: usize, emu_ctx: &EmuContext) -> bool {
    let offset = emu_ctx.address & 0xffff;

    let vm = match crate::kernel::active_vm() {
        None => {
            panic!("emu_intc_handler: vm is None");
        }
        Some(x) => x,
    };

    let vgic = vm.vgic();
    let vgicd_offset_prefix = offset >> 7;
    trace!(
        "current_cpu:{} emu_intc_handler offset:{:#x} is write:{},val:{:#x}",
        current_cpu().id,
        emu_ctx.address,
        emu_ctx.write,
        current_cpu().get_gpr(emu_ctx.reg)
    );

    match vgicd_offset_prefix {
        VGICD_REG_OFFSET_PREFIX_ISENABLER => {
            vgic.emu_isenabler_access(emu_ctx);
        }
        VGICD_REG_OFFSET_PREFIX_ISPENDR => {
            vgic.emu_ispendr_access(emu_ctx);
        }
        VGICD_REG_OFFSET_PREFIX_ISACTIVER => {
            vgic.emu_isactiver_access(emu_ctx);
        }
        VGICD_REG_OFFSET_PREFIX_ICENABLER => {
            vgic.emu_icenabler_access(emu_ctx);
        }
        VGICD_REG_OFFSET_PREFIX_ICPENDR => {
            vgic.emu_icpendr_access(emu_ctx);
        }
        VGICD_REG_OFFSET_PREFIX_ICACTIVER => {
            vgic.emu_icativer_access(emu_ctx);
        }
        VGICD_REG_OFFSET_PREFIX_ICFGR => {
            vgic.emu_icfgr_access(emu_ctx);
        }
        _ => {
            match offset {
                // VGICD_REG_OFFSET(CTLR)
                0 => {
                    vgic.emu_ctrl_access(emu_ctx);
                }
                // VGICD_REG_OFFSET(TYPER)
                0x004 => {
                    vgic.emu_typer_access(emu_ctx);
                }
                // VGICD_REG_OFFSET(IIDR)
                0x008 => {
                    vgic.emu_iidr_access(emu_ctx);
                }
                0xf00 => {
                    vgic.emu_razwi(emu_ctx);
                }
                _ => {
                    if (0x400..0x800).contains(&offset) {
                        vgic.emu_ipriorityr_access(emu_ctx);
                    } else if (0x800..0xc00).contains(&offset) {
                        vgic.emu_razwi(emu_ctx);
                    } else if (0x6000..0x8000).contains(&offset) {
                        vgic.emu_irouter_access(emu_ctx);
                    } else if (0xffd0..0x10000).contains(&offset) {
                        //ffe8 is GICD_PIDR2, Peripheral ID2 Register
                        vgic.emu_pidr_access(emu_ctx);
                    } else {
                        vgic.emu_razwi(emu_ctx);
                    }
                }
            }
        }
    }
    true
}

pub fn vgicd_emu_access_is_vaild(emu_ctx: &EmuContext) -> bool {
    let offset = emu_ctx.address & 0xffff;
    let offset_prefix = (offset & 0xff80) >> 7;
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

pub fn partial_passthrough_intc_handler(_emu_dev_id: usize, emu_ctx: &EmuContext) -> bool {
    if !vgicd_emu_access_is_vaild(emu_ctx) {
        return false;
    }
    let offset = emu_ctx.address & 0xfff;
    if emu_ctx.write {
        // todo: add offset match
        let val = current_cpu().get_gpr(emu_ctx.reg);
        ptr_read_write(Platform::GICD_BASE + offset, emu_ctx.width, val, false);
    } else {
        let res = ptr_read_write(Platform::GICD_BASE + offset, emu_ctx.width, 0, true);
        current_cpu().set_gpr(emu_ctx.reg, res);
    }

    true
}

pub fn vgic_ipi_handler(msg: &IpiMessage) {
    let vm_id;
    let int_id;
    let val;
    match &msg.ipi_message {
        IpiInnerMsg::Initc(intc) => {
            vm_id = intc.vm_id;
            int_id = intc.int_id;
            val = intc.val;
        }
        _ => {
            error!("vgic_ipi_handler: illegal ipi");
            return;
        }
    }
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
    if let IpiInnerMsg::Initc(intc) = &msg.ipi_message {
        match intc.event {
            InitcEvent::VgicdGichEn => {
                let hcr = GICH.hcr();
                if val != 0 {
                    GICH.set_hcr(hcr | GICH_HCR_EN_BIT);
                } else {
                    GICH.set_hcr(hcr & !GICH_HCR_EN_BIT);
                }
            }
            InitcEvent::VgicdSetEn => {
                vgic.set_enable(trgt_vcpu.clone(), int_id as usize, val != 0);
            }
            InitcEvent::VgicdSetPend => {
                vgic.set_pend(trgt_vcpu.clone(), int_id as usize, val != 0);
            }
            InitcEvent::VgicdSetPrio => {
                vgic.set_prio(trgt_vcpu.clone(), int_id as usize, val);
            }
            InitcEvent::VgicdSetCfg => {
                vgic.set_icfgr(trgt_vcpu.clone(), int_id as usize, val);
            }
            InitcEvent::VgicdRoute => {
                let interrupt_option = vgic.get_int(trgt_vcpu.clone(), bit_extract(int_id as usize, 0, 10));
                if let Some(interrupt) = interrupt_option {
                    let interrupt_lock = interrupt.lock.lock();
                    if vgic_int_get_owner(trgt_vcpu.clone(), interrupt.clone()) {
                        if vgic_int_vcpu_is_target(&trgt_vcpu, &interrupt) {
                            vgic.add_lr(trgt_vcpu.clone(), interrupt.clone());
                        }
                        vgic_int_yield_owner(trgt_vcpu.clone(), interrupt.clone());
                    }
                    drop(interrupt_lock);
                }
            }
            InitcEvent::Vgicdinject => {
                crate::kernel::interrupt_vm_inject(trgt_vcpu.vm().unwrap(), trgt_vcpu.clone(), int_id as usize, 0);
            }
            _ => {
                error!("vgic_ipi_handler: core {} received unknown event", current_cpu().id)
            }
        }
    }
    save_vcpu_gic(current_cpu().active_vcpu.clone(), trgt_vcpu);
}

pub fn emu_intc_init(vm: Vm, emu_dev_id: usize) {
    let vgic_cpu_num = vm.config().cpu_num();
    vm.init_intc_mode(true);

    let vgic = Arc::new(Vgic::default());

    let mut vgicd = vgic.vgicd.lock();
    vgicd.typer = (GICD.typer() & !(GICD_TYPER_CPUNUM_MSK | GICD_TYPER_LPIS) as u32)
        | ((((vm.cpu_num() - 1) << GICD_TYPER_CPUNUM_OFF) & GICD_TYPER_CPUNUM_MSK) as u32);
    vgicd.iidr = GICD.iidr();
    vgicd.ctlr = 0b10;

    for i in 0..GIC_SPI_MAX {
        vgicd.interrupts.push(VgicInt::new(i));
    }
    drop(vgicd);

    for i in 0..vgic_cpu_num {
        let mut typer = i << GICR_TYPER_PRCNUM_OFF;
        let vmpidr = vm.vcpu(i).unwrap().get_vmpidr();
        typer |= (vmpidr & MPIDR_AFF_MSK) << GICR_TYPER_AFFVAL_OFF;
        typer |= !!((i == (vm.cpu_num() - 1)) as usize) << GICR_TYPER_LAST_OFF;
        //need the low 6 bits for LPI/ITS init
        //DPGS, bit [5]:Sets support for GICR_CTLR.DPG* bits
        //DirectLPI, bit [3]: Indicates whether this Redistributor supports direct injection of LPIs.
        //Dirty, bit [2]: Controls the functionality of GICR_VPENDBASER.Dirty.
        //LPI VLPIS, bit [1]: Indicates whether the GIC implementation supports virtual LPIs and the direct injection of virtual LPIs
        //PLPIS, bit [0]: Indicates whether the GIC implementation supports physical LPIs
        typer |= 0b10_0001;

        let mut cpu_priv = VgicCpuPriv::new(
            typer,
            GICR.get_ctrl(vm.vcpu(i).unwrap().phys_id() as u32) as usize,
            GICR.get_iidr(vm.vcpu(i).unwrap().phys_id()) as usize,
        );

        for int_idx in 0..GIC_SGIS_NUM {
            let vcpu = vm.vcpu(i).unwrap();
            let phys_id = vcpu.phys_id();

            cpu_priv.interrupts.push(VgicInt::priv_new(
                int_idx,
                vcpu.clone(),
                1 << phys_id,
                true,
                phys_id,
                0b10,
            ));
        }

        for int_idx in GIC_SGIS_NUM..GIC_PRIVINT_NUM {
            let vcpu = vm.vcpu(i).unwrap();
            let phys_id = vcpu.phys_id();

            cpu_priv.interrupts.push(VgicInt::priv_new(
                int_idx,
                vcpu.clone(),
                1 << phys_id,
                false,
                phys_id,
                0b0,
            ));
        }

        let mut vgic_cpu_priv = vgic.cpu_priv.lock();
        vgic_cpu_priv.push(cpu_priv);
        drop(vgic_cpu_priv);
    }

    vm.set_emu_devs(emu_dev_id, EmuDevs::Vgic(vgic.clone()));
}

pub fn emu_vgicr_init(vm: Vm, emu_dev_id: usize) {
    let vigc = vm.emu_dev(vm.intc_dev_id()).clone();
    vm.set_emu_devs(emu_dev_id, vigc);
}

const VGICR_REG_OFFSET_CLTR: usize = 0x0;
const VGICR_REG_OFFSET_TYPER: usize = 0x8;
const VGICR_REG_OFFSET_PROPBASER: usize = 0x70;
const VGICR_REG_OFFSET_PENDBASER: usize = 0x78;
const VGICR_REG_OFFSET_ISENABLER0: usize = 0x10100;
const VGICR_REG_OFFSET_ISPENDR0: usize = 0x10200;
const VGICR_REG_OFFSET_ISACTIVER0: usize = 0x10300;
const VGICR_REG_OFFSET_ICENABLER0: usize = 0x10180;
const VGICR_REG_OFFSET_ICPENDR0: usize = 0x10280;
const VGICR_REG_OFFSET_ICACTIVER0: usize = 0x10380;
const VGICR_REG_OFFSET_ICFGR0: usize = 0x10c00;
const VGICR_REG_OFFSET_ICFGR1: usize = 0x10c04;

#[derive(Debug)]
enum GicrRegs {
    CLTR = 0x0,
    TYPER = 0x8,
    ISENABLER0 = 0x10100,
    ISPENDR0 = 0x10200,
    ISACTIVER0 = 0x10300,
    ICENABLER0 = 0x10180,
    ICPENDR0 = 0x10280,
    ICACTIVER0 = 0x10380,
    ICFGR0 = 0x10c00,
    ICFGR1 = 0x10c04,
    Others,
}

impl From<usize> for GicrRegs {
    fn from(val: usize) -> Self {
        match val {
            0x0 => Self::CLTR,
            0x8 => Self::TYPER,
            0x10100 => Self::ISENABLER0,
            0x10200 => Self::ISPENDR0,
            0x10300 => Self::ISACTIVER0,
            0x10180 => Self::ICENABLER0,
            0x10280 => Self::ICPENDR0,
            0x10380 => Self::ICACTIVER0,
            0x10c00 => Self::ICFGR0,
            0x10c04 => Self::ICFGR1,
            _ => Self::Others,
        }
    }
}

pub fn emul_vgicr_handler(_emu_dev_id: usize, emu_ctx: &EmuContext) -> bool {
    let vgic = current_cpu().active_vcpu.clone().unwrap().vm().unwrap().vgic();
    let vgicr_id = vgicr_get_id(emu_ctx);
    let offset = emu_ctx.address & 0x1ffff;

    trace!(
        "current_cpu:{}emul_vgicr_handler addr:{:#x} reg {:?} offset {:#x} is write:{}, val:{:#x}",
        current_cpu().id,
        emu_ctx.address,
        GicrRegs::from(offset),
        offset,
        emu_ctx.write,
        current_cpu().get_gpr(emu_ctx.reg)
    );

    match offset {
        VGICR_REG_OFFSET_CLTR => {
            vgicr_emul_ctrl_access(emu_ctx);
        }
        VGICR_REG_OFFSET_TYPER => {
            vgic.vgicr_emul_typer_access(emu_ctx, vgicr_id as usize);
        }
        VGICR_REG_OFFSET_ISENABLER0 => {
            vgic.emu_isenabler_access(emu_ctx);
        }
        VGICR_REG_OFFSET_ISPENDR0 => {
            vgic.emu_ispendr_access(emu_ctx);
        }
        VGICR_REG_OFFSET_ISACTIVER0 => {
            vgic.emu_isactiver_access(emu_ctx);
        }
        VGICR_REG_OFFSET_ICENABLER0 => {
            vgic.emu_icenabler_access(emu_ctx);
        }
        VGICR_REG_OFFSET_ICPENDR0 => {
            vgic.emu_icpendr_access(emu_ctx);
        }
        VGICR_REG_OFFSET_ICACTIVER0 => {
            vgic.emu_icativer_access(emu_ctx);
        }
        VGICR_REG_OFFSET_ICFGR0 | VGICR_REG_OFFSET_ICFGR1 => {
            vgic.emu_icfgr_access(emu_ctx);
        }
        VGICR_REG_OFFSET_PROPBASER => {
            vgic.emu_probaser_access(emu_ctx);
        }
        VGICR_REG_OFFSET_PENDBASER => {
            vgic.emu_pendbaser_access(emu_ctx);
        }
        _ => {
            if (0x10400..0x10420).contains(&offset) {
                vgic.emu_ipriorityr_access(emu_ctx);
            } else if (0xffd0..0x10000).contains(&offset) {
                vgicr_emul_pidr_access(emu_ctx, vgicr_id as usize);
            } else {
                vgic.emu_razwi(emu_ctx);
            }
        }
    }
    true
}

pub fn partial_passthrough_intc_init(vm: Vm) {
    vm.init_intc_mode(false);
}

pub fn vgic_set_hw_int(vm: Vm, int_id: usize) {
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

pub fn vgic_icc_sre_handler(_emu_dev_id: usize, emu_ctx: &EmuContext) -> bool {
    if !emu_ctx.write {
        current_cpu().set_gpr(emu_ctx.reg, 0x1);
    }
    true
}

pub fn vgic_send_sgi_msg(vcpu: Vcpu, pcpu_mask: usize, int_id: usize) {
    let m = IpiInitcMessage {
        event: InitcEvent::Vgicdinject,
        vm_id: vcpu.vm().clone().unwrap().id(),
        int_id: int_id as u16,
        val: true as u8,
    };
    for i in 0..PLAT_DESC.cpu_desc.num {
        if (pcpu_mask & (1 << i)) != 0 {
            ipi_send_msg(i, IpiType::IpiTIntc, IpiInnerMsg::Initc(m));
        }
    }
}

pub fn vgic_icc_sgir_handler(_emu_dev_id: usize, emu_ctx: &EmuContext) -> bool {
    if emu_ctx.write {
        let sgir = current_cpu().get_gpr(emu_ctx.reg);
        let int_id = bit_extract(sgir, GICC_SGIR_SGIINTID_OFF, GICC_SGIR_SGIINTID_LEN);
        let targtlist = if (sgir & GICC_SGIR_IRM_BIT) != 0 {
            current_cpu().active_vcpu.clone().unwrap().vm().unwrap().ncpu() & !(1 << current_cpu().id)
        } else {
            let vm = match current_cpu().active_vcpu.clone().unwrap().vm() {
                Some(tvm) => tvm,
                None => {
                    panic!("vgic_icc_sgir_handler: current vcpu.vm is none");
                }
            };
            let mut vtarget = sgir & 0xffff;
            // maybe surrort more cluseter (aff1 != 0)
            if sgir & 0xff0000 != 0 && cfg!(feature = "rk3588") {
                //for rk3588 the aff1
                vtarget <<= (sgir & 0xf0000) >> 16;
            }
            vgic_target_translate(vm, vtarget as u32, true) as usize
        };
        vgic_send_sgi_msg(current_cpu().active_vcpu.clone().unwrap(), targtlist, int_id);
    }
    true
}
