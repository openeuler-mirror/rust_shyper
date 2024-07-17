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
use alloc::vec::Vec;

use core::mem::size_of;
use spin::Mutex;

use crate::device::{DevDesc, VirtioMmio, Virtq, VIRTQ_DESC_F_NEXT, VIRTQ_DESC_F_WRITE};
use crate::device::VirtioIov;
use crate::kernel::{current_cpu, vm_if_get_cpu_id};
use crate::kernel::{ipi_send_msg, IpiEthernetMsg, IpiInnerMsg, IpiType};
use crate::kernel::IpiMessage;
use crate::kernel::Vm;
use crate::utils::trace;

const VIRTIO_NET_OK: u8 = 0;
const VIRTIO_NET_ERR: u8 = 1;

const VIRTIO_F_VERSION_1: usize = 1 << 32;
const VIRTIO_NET_F_CSUM: usize = 1 << 0;
const VIRTIO_NET_F_GUEST_CSUM: usize = 1 << 1;
const VIRTIO_NET_F_MAC: usize = 1 << 5;
const VIRTIO_NET_F_GSO_DEPREC: usize = 1 << 6;
// deprecated: host handles GSO
const VIRTIO_NET_F_GUEST_TSO4: usize = 1 << 7;
// guest can rcv TSOv4 *
const VIRTIO_NET_F_GUEST_TSO6: usize = 1 << 8;
// guest can rcv TSOv6
const VIRTIO_NET_F_GUEST_ECN: usize = 1 << 9;
// guest can rcv TSO with ECN
const VIRTIO_NET_F_GUEST_UFO: usize = 1 << 10;
// guest can rcv UFO *
const VIRTIO_NET_F_HOST_TSO4: usize = 1 << 11;
// host can rcv TSOv4 *
const VIRTIO_NET_F_HOST_TSO6: usize = 1 << 12;
// host can rcv TSOv6
const VIRTIO_NET_F_HOST_ECN: usize = 1 << 13;
// host can rcv TSO with ECN
const VIRTIO_NET_F_HOST_UFO: usize = 1 << 14;
// host can rcv UFO *
const VIRTIO_NET_F_MRG_RXBUF: usize = 1 << 15;
// host can merge RX buffers *
const VIRTIO_NET_F_STATUS: usize = 1 << 16;
// config status field available *
const VIRTIO_NET_F_CTRL_VQ: usize = 1 << 17;
// control channel available
const VIRTIO_NET_F_CTRL_RX: usize = 1 << 18;
// control channel RX mode support
const VIRTIO_NET_F_CTRL_VLAN: usize = 1 << 19;
// control channel VLAN filtering
const VIRTIO_NET_F_GUEST_ANNOUNCE: usize = 1 << 21; // guest can send gratuitous pkts

const VIRTIO_NET_HDR_F_DATA_VALID: usize = 2;

const VIRTIO_NET_HDR_GSO_NONE: usize = 0;

/// Represents the header structure for VirtioNet.
#[repr(C)]
struct VirtioNetHdr {
    pub flags: u8,
    pub gso_type: u8,
    pub hdr_len: u16,
    pub gso_size: u16,
    pub csum_start: u16,
    pub csum_offset: u16,
    pub num_buffers: u16,
}

/// A cloneable wrapper for the `NetDescInner` structure.
pub struct NetDesc {
    inner: Mutex<NetDescInner>,
}

/// Holds data related to the network device.
pub struct NetDescData {
    pub mac: [u8; 6],
    pub status: u16,
}

impl NetDesc {
    /// Creates a new `NetDesc` instance with mac values.
    pub fn new(mac: &[usize]) -> NetDesc {
        let mut desc = NetDescInner::default();
        for (i, item) in mac.iter().enumerate().take(6) {
            desc.mac[i] = *item as u8;
        }
        NetDesc {
            inner: Mutex::new(desc),
        }
    }

    /// Set the status of the network device.
    pub fn set_status(&self, status: u16) {
        let mut inner = self.inner.lock();
        inner.status = status;
    }

    /// Retrieves the status of the network device.
    pub fn status(&self) -> u16 {
        let inner = self.inner.lock();
        inner.status
    }

    /// Computes the offset data within the `NetDesc` structure.
    /// # SAFETY:
    /// Caller must ensure offset is valid
    /// Offset must valid for virtio_mmio
    pub unsafe fn offset_data(&self, offset: usize, width: usize) -> usize {
        let inner = self.inner.lock();
        let start_addr = &inner.mac[0] as *const _ as usize;
        // TODO: Some out-of-bounds accesses beyond offset 6 May return incorrect values
        match width {
            1 => unsafe { *((start_addr + offset) as *const u8) as usize },
            2 => unsafe { *((start_addr + offset) as *const u16) as usize },
            4 => unsafe { *((start_addr + offset) as *const u32) as usize },
            8 => unsafe { *((start_addr + offset) as *const u64) as usize },
            _ => 0,
        }
    }
}

/// Constant representing the network link being up.
pub const VIRTIO_NET_S_LINK_UP: u16 = 1;
/// Constant representing network announcement.
pub const VIRTIO_NET_S_ANNOUNCE: u16 = 2;

/// Represents the inner data structure for `NetDesc`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NetDescInner {
    mac: [u8; 6],
    status: u16,
}

impl NetDescInner {
    /// Creates a new `NetDescInner` instance with default values.
    pub fn default() -> NetDescInner {
        NetDescInner {
            mac: [0; 6],
            status: VIRTIO_NET_S_LINK_UP,
        }
    }
}

/// Represents the control header structure for VirtioNet.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct VirtioNetCtrlHdr {
    class: u8,
    command: u8,
}

/// Retrieves network features.
pub fn net_features() -> usize {
    VIRTIO_F_VERSION_1
        | VIRTIO_NET_F_GUEST_CSUM
        | VIRTIO_NET_F_MAC
        | VIRTIO_NET_F_CSUM
        | VIRTIO_NET_F_GUEST_TSO4
        | VIRTIO_NET_F_GUEST_TSO6
        | VIRTIO_NET_F_GUEST_UFO
        | VIRTIO_NET_F_HOST_TSO4
        | VIRTIO_NET_F_HOST_TSO6
        | VIRTIO_NET_F_HOST_UFO
        | VIRTIO_NET_F_HOST_ECN
        | VIRTIO_NET_F_CTRL_VQ
        | VIRTIO_NET_F_GUEST_ANNOUNCE
        | VIRTIO_NET_F_STATUS
}

/// Constant representing VirtioNet control announcement.
const VIRTIO_NET_CTRL_ANNOUNCE: u8 = 3;
/// Constant representing VirtioNet control announcement acknowledgment.
const VIRTIO_NET_CTRL_ANNOUNCE_ACK: u8 = 0;

/// Handles VirtioNet control operations.
pub fn virtio_net_handle_ctrl(vq: Arc<Virtq>, nic: Arc<VirtioMmio>, vm: Arc<Vm>) -> bool {
    if vq.ready() == 0 {
        error!("virtio net control queue is not ready!");
        return false;
    }

    while let Some(next_desc_idx) = vq.pop_avail_desc_idx(vq.avail_idx()) {
        let mut idx = next_desc_idx as usize;
        let mut len = 0;
        let mut out_iov = VirtioIov::default();
        let mut in_iov = VirtioIov::default();

        loop {
            let addr = vm.ipa2pa(vq.desc_addr(idx));
            if addr == 0 {
                error!("virtio_net_handle_ctrl: failed to desc addr");
                return false;
            }
            if vq.desc_flags(idx) & VIRTQ_DESC_F_WRITE != 0 {
                in_iov.push_data(addr, vq.desc_len(idx) as usize);
            } else {
                out_iov.push_data(addr, vq.desc_len(idx) as usize);
            }
            len += vq.desc_len(idx) as usize;
            if vq.desc_flags(idx) != VIRTQ_DESC_F_NEXT {
                break;
            }
            idx = vq.desc_next(idx) as usize;
        }
        let ctrl = VirtioNetCtrlHdr::default();
        out_iov.to_buf(&ctrl as *const _ as usize, size_of::<VirtioNetCtrlHdr>());
        match ctrl.class {
            VIRTIO_NET_CTRL_ANNOUNCE => {
                let status: u8 = if ctrl.command == VIRTIO_NET_CTRL_ANNOUNCE_ACK {
                    match nic.dev().desc() {
                        DevDesc::NetDesc(desc) => {
                            desc.set_status(VIRTIO_NET_S_LINK_UP);
                            VIRTIO_NET_OK
                        }
                        _ => {
                            panic!("illegal dev type for nic");
                        }
                    }
                } else {
                    VIRTIO_NET_ERR
                };
                in_iov.from_buf(&status as *const _ as usize, size_of::<u8>());
            }
            _ => {
                warn!("Control queue header class can't match {}", ctrl.class);
            }
        }

        // update ctrl queue used ring
        if !vq.update_used_ring(len as u32, next_desc_idx as u32) {
            return false;
        }
    }
    nic.notify();
    true
}

/// Handles the notification from the VirtioNet device to the specified virtual queue (`vq`) in a virtual machine (`vm`).
/// Returns `true` if the notification is successfully processed; otherwise, returns `false`.
pub fn virtio_net_notify_handler(vq: Arc<Virtq>, nic: Arc<VirtioMmio>, vm: Arc<Vm>) -> bool {
    if vq.ready() == 0 {
        error!("net virt_queue is not ready!");
        return false;
    }

    if vq.vq_indx() != 1 {
        return true;
    }

    let mut nics_to_notify = vec![];

    while let Some(head_idx) = vq.pop_avail_desc_idx(vq.avail_idx()) {
        let mut idx = head_idx as usize;
        let mut len = 0;
        let mut tx_iov = VirtioIov::default();

        loop {
            let addr = vm.ipa2pa(vq.desc_addr(idx));
            if addr == 0 {
                error!("virtio_net_notify_handler: failed to desc addr");
                return false;
            }
            tx_iov.push_data(addr, vq.desc_len(idx) as usize);

            len += vq.desc_len(idx) as usize;
            if vq.desc_flags(idx) == 0 {
                break;
            }
            idx = vq.desc_next(idx) as usize;
        }

        if let Some(list) = ethernet_transmit(tx_iov, len, &vm) {
            nics_to_notify.extend(list);
        }

        if !vq.update_used_ring((len - size_of::<VirtioNetHdr>()) as u32, head_idx as u32) {
            return false;
        }
    }

    if !vq.avail_is_avail() {
        error!("invalid descriptor table index");
        return false;
    }

    nic.notify();
    for nic in nics_to_notify {
        let trgt_vm = nic.upper_vm().unwrap();
        let vcpu = trgt_vm.vcpu(0).unwrap();
        // When the target VM is not running on the current CPU, send an IPI message to the target CPU.
        if vcpu.phys_id() == current_cpu().id {
            let rx_vq = match nic.vq(0) {
                Ok(x) => x,
                Err(_) => {
                    error!(
                        "virtio_net_notify_handler: vm[{}] failed to get virtio net rx virt queue",
                        vm.id()
                    );
                    return false;
                }
            };
            if rx_vq.ready() != 0 && rx_vq.avail_flags() == 0 {
                nic.notify();
            }
        } else if let Some(cpu_trgt) = vm_if_get_cpu_id(trgt_vm.id()) {
            let msg = IpiEthernetMsg { trgt_nic: nic };
            if !ipi_send_msg(cpu_trgt, IpiType::IpiTEthernetMsg, IpiInnerMsg::EnternetMsg(msg)) {
                error!(
                    "virtio_net_notify_handler: failed to send IPI message to CPU {}",
                    cpu_trgt
                );
                return false;
            }
        } else {
            error!(
                "virtio_net_notify_handler: failed to get cpu id for vm {}",
                trgt_vm.id()
            );
            return false;
        }
    }
    true
}

/// Handles the IPI (Inter-Processor Interrupt) message related to Ethernet.
pub fn ethernet_ipi_rev_handler(msg: IpiMessage) {
    match msg.ipi_message {
        IpiInnerMsg::EnternetMsg(ethernet_msg) => {
            let nic = ethernet_msg.trgt_nic;
            let vm = nic.upper_vm().unwrap();
            let rx_vq = match nic.vq(0) {
                Ok(x) => x,
                Err(_) => {
                    error!(
                        "ethernet_ipi_rev_handler: vm[{}] failed to get virtio net rx virt queue",
                        vm.id()
                    );
                    return;
                }
            };

            if rx_vq.ready() != 0 && rx_vq.avail_flags() == 0 {
                nic.notify();
            }
        }
        _ => {
            panic!("illegal ipi message type in ethernet_ipi_rev_handler");
        }
    }
}

/// Transmits Ethernet frames using VirtioNet.
/// Returns a tuple with the first element indicating success (`true` if successful) and the second element
/// representing the target virtual machine's bitmask.
fn ethernet_transmit(tx_iov: VirtioIov, len: usize, vm: &Vm) -> Option<Vec<Arc<VirtioMmio>>> {
    // [ destination MAC - 6 ][ source MAC - 6 ][ EtherType - 2 ][ Payload ]
    if len < size_of::<VirtioNetHdr>() || len - size_of::<VirtioNetHdr>() < 6 + 6 + 2 {
        warn!(
            "Too short for an ethernet frame, len {}, size of head {}",
            len,
            size_of::<VirtioNetHdr>()
        );
        return None;
    }

    let frame: &[u8] = tx_iov.get_ptr(size_of::<VirtioNetHdr>());

    if frame[0..6] == [0xff, 0xff, 0xff, 0xff, 0xff, 0xff] {
        if ethernet_is_arp(frame) {
            return ethernet_broadcast(&tx_iov, len, vm);
        }
        return None;
    }

    if frame[0] == 0x33 && frame[1] == 0x33 {
        if !(frame[12] == 0x86 && frame[13] == 0xdd) {
            // Only IPV6 multicast packet is allowed to be broadcast
            return None;
        }
        return ethernet_broadcast(&tx_iov, len, vm);
    }

    match ethernet_mac_to_vm_id(frame) {
        Ok(nic) => {
            let vm = nic.upper_vm().unwrap();
            if ethernet_send_to(&vm, &nic, &tx_iov, len) {
                Some(vec![nic])
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

/// Broadcasts an Ethernet frame to all virtual machines, excluding the current one.
fn ethernet_broadcast(tx_iov: &VirtioIov, len: usize, cur_vm: &Vm) -> Option<Vec<Arc<VirtioMmio>>> {
    let mut nic_list = vec![];
    super::mac::virtio_nic_list_walker(|nic| {
        let vm = nic.upper_vm().unwrap();
        if vm.id() != cur_vm.id() && ethernet_send_to(&vm, nic, tx_iov, len) {
            nic_list.push(nic.clone());
        }
    });
    if nic_list.is_empty() {
        None
    } else {
        Some(nic_list)
    }
}

/// Sends an Ethernet frame to the specified virtual machine (`vmid`).
fn ethernet_send_to(vm: &Vm, nic: &VirtioMmio, tx_iov: &VirtioIov, len: usize) -> bool {
    if !nic.dev().activated() {
        return false;
    }

    let rx_vq = match nic.vq(0) {
        Ok(x) => x,
        Err(_) => {
            error!(
                "ethernet_send_to: vm[{}] failed to get virtio net rx virt queue",
                vm.id()
            );
            return false;
        }
    };

    let desc_header_idx_opt = rx_vq.pop_avail_desc_idx(rx_vq.avail_idx());
    if !rx_vq.avail_is_avail() {
        error!("ethernet_send_to: receive invalid avail desc idx");
        return false;
    } else if desc_header_idx_opt.is_none() {
        return false;
    }

    let desc_idx_header = desc_header_idx_opt.unwrap();
    let mut desc_idx = desc_header_idx_opt.unwrap() as usize;
    let mut rx_iov = VirtioIov::default();
    let mut rx_len = 0;

    loop {
        let dst = vm.ipa2pa(rx_vq.desc_addr(desc_idx));
        if dst == 0 {
            debug!(
                "rx_vq desc base table addr 0x{:x}, idx {}, avail table addr 0x{:x}, avail last idx {}",
                rx_vq.desc_table_addr(),
                desc_idx,
                rx_vq.avail_addr(),
                rx_vq.avail_idx()
            );
            error!("ethernet_send_to: failed to get dst {}", vm.id());
            return false;
        }
        let desc_len = rx_vq.desc_len(desc_idx) as usize;

        rx_iov.push_data(dst, desc_len);
        rx_len += desc_len;
        if rx_len >= len {
            break;
        }
        if rx_vq.desc_flags(desc_idx) & 0x1 == 0 {
            break;
        }
        desc_idx = rx_vq.desc_next(desc_idx) as usize;
    }

    if rx_len < len {
        rx_vq.put_back_avail_desc_idx();
        warn!("ethernet_send_to: rx_len smaller than tx_len");
        return false;
    }
    if trace() && tx_iov.get_buf(0) < 0x1000 {
        panic!("illegal header addr {}", tx_iov.get_buf(0));
    }
    let header = unsafe { &mut *(tx_iov.get_buf(0) as *mut VirtioNetHdr) };
    header.num_buffers = 1;

    if tx_iov.write_through_iov(&rx_iov, len) > 0 {
        error!(
            "ethernet_send_to: write through iov failed, rx_iov_num {} tx_iov_num {} rx_len {} tx_len {}",
            rx_iov.num(),
            tx_iov.num(),
            rx_len,
            len
        );
        return false;
    }

    if !rx_vq.update_used_ring(len as u32, desc_idx_header as u32) {
        return false;
    }

    true
}

/// Determines whether the given Ethernet frame is an ARP (Address Resolution Protocol) packet.
fn ethernet_is_arp(frame: &[u8]) -> bool {
    frame[12] == 0x8 && frame[13] == 0x6
}

/// Maps the MAC address in the Ethernet frame to the corresponding virtual machine ID.
fn ethernet_mac_to_vm_id(frame: &[u8]) -> Result<Arc<VirtioMmio>, ()> {
    let frame_mac = &frame[0..6];
    super::mac::mac_to_nic(frame_mac).ok_or(())
}

/// Handles the VirtioNet announcement in a virtual machine (`vm`).
pub fn virtio_net_announce(vm: Arc<Vm>) {
    super::mac::virtio_nic_list_walker(|nic| {
        if let Some(nic_vm) = nic.upper_vm() {
            if Arc::ptr_eq(&nic_vm, &vm) {
                nic.notify_config();
            }
        }
    })
}
