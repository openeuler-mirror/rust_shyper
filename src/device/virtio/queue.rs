// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use alloc::sync::{Arc, Weak};

use core::slice;

use spin::Mutex;

use crate::device::VirtioMmio;
use crate::kernel::{active_vm, Vm, vm_ipa2pa};

pub const VIRTQ_READY: usize = 1;
pub const VIRTQ_DESC_F_NEXT: u16 = 1;
pub const VIRTQ_DESC_F_WRITE: u16 = 2;

pub const VRING_USED_F_NO_NOTIFY: usize = 1;

pub const DESC_QUEUE_SIZE: usize = 512;

/// Represents a descriptor in a VirtIO ring buffer.
#[repr(C, align(16))]
#[derive(Copy, Clone)]
struct VringDesc {
    /// Guest-physical address of the descriptor.
    pub addr: usize,
    /// Length of the descriptor.
    len: u32,
    /// Flags indicating descriptor properties.
    flags: u16,
    /// Index of the next descriptor in the chain.
    next: u16,
}

/// Represents the available ring in a VirtIO queue.
#[repr(C)]
#[derive(Copy, Clone)]
struct VringAvail {
    /// Flags indicating the state of the available ring.
    flags: u16,
    /// Index pointing to the next available descriptor in the ring.
    idx: u16,
    /// Array representing the available ring.
    ring: [u16; 512],
}

/// Represents an element in the used ring of a VirtIO queue.
#[repr(C)]
#[derive(Copy, Clone)]
struct VringUsedElem {
    /// Identifier of the descriptor.
    pub id: u32,
    /// Length of the used descriptor.
    pub len: u32,
}

/// Represents the data associated with a VirtIO queue.
#[derive(Copy, Clone)]
pub struct VirtqData {
    /// Indicates whether the VirtIO queue is ready.
    pub ready: usize,
    /// Index of the VirtIO queue.
    pub vq_index: usize,
    /// Number of descriptors in the queue.
    pub num: usize,

    /// Last available index in the available ring.
    pub last_avail_idx: u16,
    /// Last used index in the used ring.
    pub last_used_idx: u16,
    /// Flags indicating the state of the used ring.
    pub used_flags: u16,

    /// Guest-physical address of the descriptor table.
    pub desc_table_ipa: usize,
    /// Guest-physical address of the available ring.
    pub avail_ipa: usize,
    /// Guest-physical address of the used ring.
    pub used_ipa: usize,
}

/// Represents the used ring in a VirtIO queue.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct VringUsed {
    /// Flags indicating the state of the used ring.
    flags: u16,
    /// Index pointing to the next used descriptor in the ring.
    idx: u16,
    /// Array representing the used ring, containing VringUsedElem elements.
    ring: [VringUsedElem; 512],
}

/// A wrapper struct representing a VirtIO queue.
pub struct Virtq {
    /// The index of the VirtIO queue.
    vq_index: usize,
    /// Optional function to handle VirtIO queue notifications.
    notify_handler: fn(Arc<Self>, Arc<VirtioMmio>, Arc<Vm>) -> bool,
    mmio: Weak<VirtioMmio>,
    inner: Mutex<VirtqInner<'static>>,
}

impl Virtq {
    /// Creates a new Virtq instance.
    pub fn new(
        vq_index: usize,
        mmio: Weak<VirtioMmio>,
        notify_handler: fn(Arc<Self>, Arc<VirtioMmio>, Arc<Vm>) -> bool,
    ) -> Arc<Self> {
        Arc::new(Self {
            vq_index,
            notify_handler,
            mmio,
            inner: Mutex::new(VirtqInner::default()),
        })
    }

    /// Resets the VirtIO queue at the specified index.
    pub fn reset(&self) {
        let mut inner = self.inner.lock();
        inner.reset();
    }

    /// Pops the next available descriptor index from the available ring.
    pub fn pop_avail_desc_idx(&self, avail_idx: u16) -> Option<u16> {
        let mut inner = self.inner.lock();
        match &inner.avail {
            Some(avail) => {
                if avail_idx == inner.last_avail_idx {
                    return None;
                }
                let idx = inner.last_avail_idx as usize % inner.num;
                let avail_desc_idx = avail.ring[idx];
                inner.last_avail_idx = inner.last_avail_idx.wrapping_add(1);
                Some(avail_desc_idx)
            }
            None => {
                error!("pop_avail_desc_idx: failed to avail table");
                None
            }
        }
    }

    /// Puts back the last popped available descriptor index.
    pub fn put_back_avail_desc_idx(&self) {
        let mut inner = self.inner.lock();
        match &inner.avail {
            Some(_) => {
                inner.last_avail_idx -= 1;
            }
            None => {
                error!("put_back_avail_desc_idx: failed to avail table");
            }
        }
    }

    /// Checks if the available ring has available descriptors.
    pub fn avail_is_avail(&self) -> bool {
        let inner = self.inner.lock();
        inner.avail.is_some()
    }

    /// Disables notifications for the used ring.
    pub fn disable_notify(&self) {
        let mut inner = self.inner.lock();
        if inner.used_flags & VRING_USED_F_NO_NOTIFY as u16 != 0 {
            return;
        }
        inner.used_flags |= VRING_USED_F_NO_NOTIFY as u16;
    }

    /// Enables notifications for the used ring.
    pub fn enable_notify(&self) {
        let mut inner = self.inner.lock();
        if inner.used_flags & VRING_USED_F_NO_NOTIFY as u16 == 0 {
            return;
        }
        inner.used_flags &= !VRING_USED_F_NO_NOTIFY as u16;
    }

    /// Checks if the available index matches the last available index.
    pub fn check_avail_idx(&self, avail_idx: u16) -> bool {
        let inner = self.inner.lock();
        inner.last_avail_idx == avail_idx
    }

    /// Checks if the descriptor at the given index is writable.
    pub fn desc_is_writable(&self, idx: usize) -> bool {
        let inner = self.inner.lock();
        let desc_table = inner.desc_table.as_ref().unwrap();
        desc_table[idx].flags & VIRTQ_DESC_F_WRITE != 0
    }

    /// Checks if the descriptor at the given index has a next descriptor in the chain.
    pub fn desc_has_next(&self, idx: usize) -> bool {
        let inner = self.inner.lock();
        let desc_table = inner.desc_table.as_ref().unwrap();
        desc_table[idx].flags & VIRTQ_DESC_F_NEXT != 0
    }

    /// Updates the used ring with the provided information.
    pub fn update_used_ring(&self, len: u32, desc_chain_head_idx: u32) -> bool {
        let mut inner = self.inner.lock();
        let num = inner.num;
        let flag = inner.used_flags;
        match &mut inner.used {
            Some(used) => {
                used.flags = flag;
                used.ring[used.idx as usize % num].id = desc_chain_head_idx;
                used.ring[used.idx as usize % num].len = len;
                used.idx = used.idx.wrapping_add(1);
                true
            }
            None => {
                error!("update_used_ring: failed to used table");
                false
            }
        }
    }

    /// Calls the registered notify handler function.
    pub fn call_notify_handler(self: &Arc<Self>) -> bool {
        if let Some(mmio) = self.mmio.upgrade() {
            (self.notify_handler)(self.clone(), mmio, active_vm().unwrap())
        } else {
            false
        }
    }

    /// Displays information about the descriptors in the VirtIO queue.
    pub fn show_desc_info(&self, size: usize, vm: Arc<Vm>) {
        let inner = self.inner.lock();
        let desc = inner.desc_table.as_ref().unwrap();
        info!("[*desc_ring*]");
        for i in 0..size {
            let desc_addr = vm_ipa2pa(&vm, desc[i].addr);
            info!(
                "index {}   desc_addr_ipa 0x{:x}   desc_addr_pa 0x{:x}   len 0x{:x}   flags {}  next {}",
                i, desc[i].addr, desc_addr, desc[i].len, desc[i].flags, desc[i].next
            );
        }
    }

    /// Displays information about the available ring in the VirtIO queue.
    pub fn show_avail_info(&self, size: usize) {
        let inner = self.inner.lock();
        let avail = inner.avail.as_ref().unwrap();
        info!("[*avail_ring*]");
        for i in 0..size {
            info!("index {} ring_idx {}", i, avail.ring[i]);
        }
    }

    /// Displays information about the used ring in the VirtIO queue.
    pub fn show_used_info(&self, size: usize) {
        let inner = self.inner.lock();
        let used = inner.used.as_ref().unwrap();
        info!("[*used_ring*]");
        for i in 0..size {
            info!(
                "index {} ring_id {} ring_len {:x}",
                i, used.ring[i].id, used.ring[i].len
            );
        }
    }

    /// Displays information about the guest-physical addresses of the VirtIO queue components.
    pub fn show_addr_info(&self) {
        let inner = self.inner.lock();
        info!(
            "avail_addr {:x}, desc_addr {:x}, used_addr {:x}",
            inner.avail_addr, inner.desc_table_addr, inner.used_addr
        );
    }

    /// Sets the last used index for the used ring.
    pub fn set_last_used_idx(&self, last_used_idx: u16) {
        let mut inner: spin::MutexGuard<'_, VirtqInner<'_>> = self.inner.lock();
        inner.last_used_idx = last_used_idx;
    }

    /// Sets the number of descriptors in the VirtIO queue.
    pub fn set_num(&self, num: usize) {
        let mut inner = self.inner.lock();
        inner.num = num;
    }

    /// Sets the ready state of the VirtIO queue.
    pub fn set_ready(&self, ready: usize) {
        let mut inner = self.inner.lock();
        inner.ready = ready;
    }

    /// Combines the descriptor table address with the provided address using a bitwise OR operation.
    pub fn or_desc_table_addr(&self, addr: usize) {
        let mut inner = self.inner.lock();
        inner.desc_table_addr |= addr;
    }

    /// Combines the available ring address with the provided address using a bitwise OR operation.
    pub fn or_avail_addr(&self, addr: usize) {
        let mut inner = self.inner.lock();
        inner.avail_addr |= addr;
    }

    /// Combines the used ring address with the provided address using a bitwise OR operation.
    pub fn or_used_addr(&self, addr: usize) {
        let mut inner = self.inner.lock();
        inner.used_addr |= addr;
    }

    /// Sets the descriptor table for the VirtIO queue.
    /// # Safety:
    /// The 'desc_table_addr' must be valid MMIO address of virtio queue
    /// And it must be in range of the vm memory
    pub unsafe fn set_desc_table(&self, addr: usize) {
        let mut inner = self.inner.lock();
        inner.desc_table = Some(slice::from_raw_parts_mut(addr as *mut VringDesc, DESC_QUEUE_SIZE));
    }

    /// Sets the available ring for the VirtIO queue.
    /// # Safety:
    /// The 'avail_addr' must be valid MMIO address of virtio queue
    /// And it must be in range of the vm memory
    pub unsafe fn set_avail(&self, addr: usize) {
        let mut inner = self.inner.lock();
        inner.avail = Some(&mut *(addr as *mut VringAvail));
    }

    /// Sets the used ring for the VirtIO queue.
    /// # Safety:
    /// The 'used_addr' must be valid MMIO address of virtio queue
    /// And it must be in range of the vm memory
    pub unsafe fn set_used(&self, addr: usize) {
        let mut inner = self.inner.lock();
        inner.used = Some(&mut *(addr as *mut VringUsed));
    }

    /// Returns the last used index for the used ring.
    pub fn last_used_idx(&self) -> u16 {
        let inner = self.inner.lock();
        inner.last_used_idx
    }

    /// Returns the descriptor table address for the VirtIO queue.
    pub fn desc_table_addr(&self) -> usize {
        let inner = self.inner.lock();
        inner.desc_table_addr
    }

    /// Returns the available ring address for the VirtIO queue.
    pub fn avail_addr(&self) -> usize {
        let inner = self.inner.lock();
        inner.avail_addr
    }

    /// Returns the used ring address for the VirtIO queue.
    pub fn used_addr(&self) -> usize {
        let inner = self.inner.lock();
        inner.used_addr
    }

    /// Returns a pointer to the descriptor table for the VirtIO queue.
    pub fn desc_table(&self) -> usize {
        let inner = self.inner.lock();
        match &inner.desc_table {
            None => 0,
            Some(desc_table) => &(desc_table[0]) as *const _ as usize,
        }
    }

    /// Returns the address of the available ring for the VirtIO queue.
    pub fn avail(&self) -> usize {
        let inner = self.inner.lock();
        match &inner.avail {
            None => 0,
            Some(avail) => (*avail) as *const _ as usize,
        }
    }

    /// Returns the address of the used ring for the VirtIO queue.
    pub fn used(&self) -> usize {
        let inner = self.inner.lock();
        match &inner.used {
            None => 0,
            Some(used) => (*used) as *const _ as usize,
        }
    }

    /// Returns the ready state of the VirtIO queue.
    pub fn ready(&self) -> usize {
        let inner = self.inner.lock();
        inner.ready
    }

    /// Returns the VirtIO queue index.
    pub fn vq_indx(&self) -> usize {
        self.vq_index
    }

    /// Returns the number of descriptors in the VirtIO queue.
    pub fn num(&self) -> usize {
        let inner = self.inner.lock();
        inner.num
    }

    /// Returns the guest-physical address of the descriptor at the specified index.
    pub fn desc_addr(&self, idx: usize) -> usize {
        let inner = self.inner.lock();
        let desc_table = inner.desc_table.as_ref().unwrap();
        desc_table[idx].addr
    }

    /// Returns the flags of the descriptor at the specified index.
    pub fn desc_flags(&self, idx: usize) -> u16 {
        let inner = self.inner.lock();
        let desc_table = inner.desc_table.as_ref().unwrap();
        desc_table[idx].flags
    }

    /// Returns the 'next' field of the descriptor at the specified index.
    pub fn desc_next(&self, idx: usize) -> u16 {
        let inner = self.inner.lock();
        let desc_table = inner.desc_table.as_ref().unwrap();
        desc_table[idx].next
    }

    /// Returns the length of the descriptor at the specified index.
    pub fn desc_len(&self, idx: usize) -> u32 {
        let inner = self.inner.lock();
        let desc_table = inner.desc_table.as_ref().unwrap();
        desc_table[idx].len
    }

    /// Returns the flags of the available ring.
    pub fn avail_flags(&self) -> u16 {
        let inner = self.inner.lock();
        let avail = inner.avail.as_ref().unwrap();
        avail.flags
    }

    /// Returns the index of the available ring.
    pub fn avail_idx(&self) -> u16 {
        let inner = self.inner.lock();
        let avail = inner.avail.as_ref().unwrap();
        avail.idx
    }

    /// Returns the last available index in the VirtIO queue.
    pub fn last_avail_idx(&self) -> u16 {
        let inner = self.inner.lock();
        inner.last_avail_idx
    }

    /// Returns the index of the used ring.
    pub fn used_idx(&self) -> u16 {
        let inner = self.inner.lock();
        let used = inner.used.as_ref().unwrap();
        used.idx
    }
}

/// Represents the inner state of a VirtIO queue.
pub struct VirtqInner<'a> {
    /// The ready state of the VirtIO queue.
    ready: usize,
    /// The number of descriptors in the VirtIO queue.
    num: usize,
    /// Optional reference to the descriptor table.
    desc_table: Option<&'a mut [VringDesc]>,
    /// Optional reference to the available ring.
    avail: Option<&'a mut VringAvail>,
    /// Optional reference to the used ring.
    used: Option<&'a mut VringUsed>,
    /// The last available index in the VirtIO queue.
    last_avail_idx: u16,
    /// The last used index in the VirtIO queue.
    last_used_idx: u16,
    /// Flags for the used ring.
    used_flags: u16,

    /// Guest-physical address of the descriptor table.
    desc_table_addr: usize,
    /// Guest-physical address of the available ring.
    avail_addr: usize,
    /// Guest-physical address of the used ring.
    used_addr: usize,
}

impl VirtqInner<'_> {
    /// Creates a new default instance of `VirtqInner`.
    pub fn default() -> Self {
        VirtqInner {
            ready: 0,
            num: 0,
            desc_table: None,
            avail: None,
            used: None,
            last_avail_idx: 0,
            last_used_idx: 0,
            used_flags: 0,

            desc_table_addr: 0,
            avail_addr: 0,
            used_addr: 0,
        }
    }

    /// Resets the VirtIO queue to its initial state.
    pub fn reset(&mut self) {
        self.ready = 0;
        self.num = 0;
        self.last_avail_idx = 0;
        self.last_used_idx = 0;
        self.used_flags = 0;
        self.desc_table_addr = 0;
        self.avail_addr = 0;
        self.used_addr = 0;

        self.desc_table = None;
        self.avail = None;
        self.used = None;
    }
}
