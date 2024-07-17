// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.
use alloc::sync::Arc;

use crate::arch::traits::InterruptController;
use crate::board::PLAT_DESC;
use crate::device::{VirtioMmio, Virtq};
use crate::kernel::{CPU_IF_LIST, current_cpu, interrupt_cpu_ipi_send};
use crate::vmm::{VmmEvent, VmmPercoreEvent};

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
/// CPU Power event enum
pub enum PowerEvent {
    PsciIpiCpuOn,
    PsciIpiCpuOff,
    PsciIpiCpuReset,
    PsciIpiVcpuAssignAndCpuOn,
}

#[derive(Copy, Clone)]
/// Event message struct transfered by IPI
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
/// Power Message Struct transfered by IPI
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

#[derive(Clone)]
/// Ethernet Message Struct transfered by IPI
pub struct IpiEthernetMsg {
    pub trgt_nic: Arc<VirtioMmio>,
}

#[derive(Copy, Clone)]
/// VM Management Message Struct transfered by IPI
pub struct IpiVmmMsg {
    pub vmid: usize,
    pub event: VmmEvent,
}

#[derive(Clone)]
/// VCPU Message Struct transfered by IPI
pub struct IpiVmmPercoreMsg {
    pub vm: Arc<Vm>,
    pub event: VmmPercoreEvent,
}

// only support for mediated blk
#[derive(Clone)]
/// Mediated Device Message Struct transfered by IPI
pub struct IpiMediatedMsg {
    pub src_vm: Arc<Vm>,
    pub vq: Arc<Virtq>,
    pub blk: Arc<VirtioMmio>,
}

#[derive(Clone, Copy)]
/// Mediated Device Notify Message Struct transfered by IPI
pub struct IpiMediatedNotifyMsg {
    pub vm_id: usize,
}

#[derive(Clone, Copy)]
/// HVC Message Struct transfered by IPI
pub struct IpiHvcMsg {
    pub src_vmid: usize,
    pub trgt_vmid: usize,
    pub fid: usize,
    pub event: usize,
}

#[derive(Clone, Copy)]
/// Interrupt Inject Message Struct transfered by IPI
pub struct IpiIntInjectMsg {
    pub vm_id: usize,
    pub int_id: usize,
}

declare_enum_with_handler! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    #[repr(usize)]
    pub enum IpiType [pub IPI_HANDLER_LIST => fn(IpiMessage)] {
        IpiTIntc => crate::arch::vgic_ipi_handler,
        IpiTPower => crate::arch::psci_ipi_handler,
        IpiTEthernetMsg => crate::device::ethernet_ipi_rev_handler,
        IpiTHvc => crate::kernel::hvc_ipi_handler,
        IpiTVMM => crate::vmm::vmm_ipi_handler,
        IpiTMediatedDev => crate::device::mediated_ipi_handler,
        IpiTIntInject => crate::kernel::interrupt_inject_ipi_handler,
    }
}

#[derive(Clone)]
/// Struct for all types of IPI Message
pub enum IpiInnerMsg {
    Initc(IpiInitcMessage),
    Power(IpiPowerMessage),
    EnternetMsg(IpiEthernetMsg),
    VmmMsg(IpiVmmMsg),
    VmmPercoreMsg(IpiVmmPercoreMsg),
    MediatedMsg(IpiMediatedMsg),
    MediatedNotifyMsg(IpiMediatedNotifyMsg),
    HvcMsg(IpiHvcMsg),
    IntInjectMsg(IpiIntInjectMsg),
    HyperFreshMsg(),
    None,
}

/// Struct for IPI Message
pub struct IpiMessage {
    pub ipi_type: IpiType,
    pub ipi_message: IpiInnerMsg,
}

const IPI_HANDLER_MAX: usize = 16;

/// ipi handler entry, scanning the received ipi list and call the coresponding handler
pub fn ipi_irq_handler() {
    let cpu_id = current_cpu().id;
    let mut cpu_if_list = CPU_IF_LIST.lock();
    let mut msg: Option<IpiMessage> = cpu_if_list[cpu_id].pop();
    drop(cpu_if_list);

    #[cfg(target_arch = "riscv64")]
    crate::arch::interrupt::deactivate_soft_intr();

    while msg.is_some() {
        let ipi_msg = msg.unwrap();
        let ipi_type = ipi_msg.ipi_type as usize;

        if let Some(handler) = IPI_HANDLER_LIST.get(ipi_type) {
            handler(ipi_msg);
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

    #[cfg(target_arch = "aarch64")]
    crate::arch::dsb::ishst();
    #[cfg(target_arch = "riscv64")]
    crate::arch::fence();

    interrupt_cpu_ipi_send(target_id, crate::arch::IntCtrl::IRQ_IPI);

    true
}

/// send ipi to target cpu
pub fn ipi_send_msg(target_id: usize, ipi_type: IpiType, ipi_message: IpiInnerMsg) -> bool {
    let msg = IpiMessage { ipi_type, ipi_message };
    ipi_send(target_id, msg)
}

pub fn ipi_intra_broadcast_msg(vm: &Vm, ipi_type: IpiType, msg: IpiInnerMsg) -> bool {
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
