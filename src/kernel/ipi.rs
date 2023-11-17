// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use crate::arch::INTERRUPT_IRQ_IPI;
use crate::board::PLAT_DESC;
use crate::device::{VirtioMmio, Virtq};
use crate::kernel::{CPU_IF_LIST, current_cpu, interrupt_cpu_ipi_send};
use crate::vmm::VmmEvent;

use super::Vm;

#[derive(Copy, Clone, Debug)]
pub enum InitcEvent {
    VgicdGichEn,
    VgicdSetEn,
    VgicdSetAct,
    VgicdSetPend,
    VgicdSetPrio,
    VgicdSetTrgt,
    VgicdSetCfg,
    VgicdRoute,
    Vgicdinject,
    None,
}

#[derive(Copy, Clone)]
pub enum PowerEvent {
    PsciIpiCpuOn,
    PsciIpiCpuOff,
    PsciIpiCpuReset,
    PsciIpiVcpuAssignAndCpuOn,
}

#[derive(Copy, Clone)]
pub struct IpiInitcMessage {
    pub event: InitcEvent,
    pub vm_id: usize,
    pub int_id: u16,
    pub val: u8,
}

/*
* src: src vm id
*/
#[derive(Copy, Clone)]
pub struct IpiPowerMessage {
    pub src: usize,
    pub vcpuid: usize,
    pub event: PowerEvent,
    pub entry: usize,
    pub context: usize,
}

// #[derive(Copy, Clone)]
// pub struct IpiEthernetAckMsg {
//     pub len: usize,
//     pub succeed: bool,
// }

#[derive(Copy, Clone)]
pub struct IpiEthernetMsg {
    pub src_vmid: usize,
    pub trgt_vmid: usize,
}

#[derive(Copy, Clone)]
pub struct IpiVmmMsg {
    pub vmid: usize,
    pub event: VmmEvent,
}

#[derive(Copy, Clone)]
pub struct IpiVcpuMsg {
    pub vmid: usize,
    pub vcpuid: usize,
    pub event: VmmEvent,
}

// only support for mediated blk
#[derive(Clone)]
pub struct IpiMediatedMsg {
    pub src_id: usize,
    pub vq: Virtq,
    pub blk: VirtioMmio,
    // pub avail_idx: u16,
}

#[derive(Clone, Copy)]
pub struct IpiMediatedNotifyMsg {
    pub vm_id: usize,
}

#[derive(Clone, Copy)]
pub struct IpiHvcMsg {
    pub src_vmid: usize,
    pub trgt_vmid: usize,
    pub fid: usize,
    pub event: usize,
}

#[derive(Clone, Copy)]
pub struct IpiIntInjectMsg {
    pub vm_id: usize,
    pub int_id: usize,
}

declare_enum_with_handler! {
    pub enum IpiType [pub IPI_HANDLER_LIST => IpiHandlerFunc] {
        IpiTIntc => crate::arch::vgic_ipi_handler,
        IpiTPower => crate::arch::psci_ipi_handler,
        IpiTEthernetMsg => crate::device::ethernet_ipi_rev_handler,
        IpiTHyperFresh => crate::kernel::hyper_fresh_ipi_handler,
        IpiTHvc => crate::kernel::hvc_ipi_handler,
        IpiTVMM => crate::vmm::vmm_ipi_handler,
        IpiTMediatedDev => crate::device::mediated_ipi_handler,
        IpiTIntInject => crate::kernel::interrupt_inject_ipi_handler,
    }
}

#[derive(Clone)]
pub enum IpiInnerMsg {
    Initc(IpiInitcMessage),
    Power(IpiPowerMessage),
    EnternetMsg(IpiEthernetMsg),
    VmmMsg(IpiVmmMsg),
    VcpuMsg(IpiVcpuMsg),
    MediatedMsg(IpiMediatedMsg),
    MediatedNotifyMsg(IpiMediatedNotifyMsg),
    HvcMsg(IpiHvcMsg),
    IntInjectMsg(IpiIntInjectMsg),
    HyperFreshMsg(),
    None,
}

pub struct IpiMessage {
    pub ipi_type: IpiType,
    pub ipi_message: IpiInnerMsg,
}

const IPI_HANDLER_MAX: usize = 16;

pub type IpiHandlerFunc = fn(&IpiMessage);

pub struct IpiHandler {
    pub handler: IpiHandlerFunc,
    pub ipi_type: IpiType,
}

impl IpiHandler {
    fn new(handler: IpiHandlerFunc, ipi_type: IpiType) -> IpiHandler {
        IpiHandler { handler, ipi_type }
    }
}

pub fn ipi_irq_handler() {
    // println!("ipi handler");
    let cpu_id = current_cpu().id;
    let mut cpu_if_list = CPU_IF_LIST.lock();
    let mut msg: Option<IpiMessage> = cpu_if_list[cpu_id].pop();
    drop(cpu_if_list);

    while !msg.is_none() {
        let ipi_msg = msg.unwrap();
        let ipi_type = ipi_msg.ipi_type as usize;

        if let Some(handler) = IPI_HANDLER_LIST.get(ipi_type) {
            handler(&ipi_msg);
        } else {
            error!("illegal ipi type {}", ipi_type)
        }
        let mut cpu_if_list = CPU_IF_LIST.lock();
        msg = cpu_if_list[cpu_id].pop();
    }
}

fn ipi_send(target_id: usize, msg: IpiMessage) -> bool {
    if target_id >= PLAT_DESC.cpu_desc.num {
        warn!("ipi_send: core {} not exist", target_id);
        return false;
    }

    let mut cpu_if_list = CPU_IF_LIST.lock();
    cpu_if_list[target_id].msg_queue.push(msg);
    drop(cpu_if_list);
    unsafe {
        core::arch::asm!("dsb ishst");
    }
    interrupt_cpu_ipi_send(target_id, INTERRUPT_IRQ_IPI);

    true
}

pub fn ipi_send_msg(target_id: usize, ipi_type: IpiType, ipi_message: IpiInnerMsg) -> bool {
    let msg = IpiMessage { ipi_type, ipi_message };
    ipi_send(target_id, msg)
}

pub fn ipi_intra_broadcast_msg(vm: Vm, ipi_type: IpiType, msg: IpiInnerMsg) -> bool {
    let mut i = 0;
    let mut n = 0;
    while n < (vm.cpu_num() - 1) {
        if ((1 << i) & vm.ncpu()) != 0 && i != current_cpu().id {
            n += 1;
            if !ipi_send_msg(i, ipi_type, msg.clone()) {
                error!(
                    "ipi_intra_broadcast_msg: Failed to send ipi request, cpu {} type {}",
                    i, ipi_type as usize
                );
                return false;
            }
        }

        i += 1;
    }
    true
}
