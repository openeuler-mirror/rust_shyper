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

use core::slice;

use spin::Mutex;

use crate::device::VirtioDeviceType;
use crate::device::VirtioMmio;
use crate::kernel::{active_vm, Vm, vm_ipa2pa};
use crate::utils::trace;

pub const VIRTQ_READY: usize = 1;
pub const VIRTQ_DESC_F_NEXT: u16 = 1;
pub const VIRTQ_DESC_F_WRITE: u16 = 2;

pub const VRING_USED_F_NO_NOTIFY: usize = 1;

pub const DESC_QUEUE_SIZE: usize = 512;

#[repr(C, align(16))]
#[derive(Copy, Clone)]
struct VringDesc {
    /*Address (guest-physical)*/
    pub addr: usize,
    /* Length */
    len: u32,
    /* The flags as indicated above */
    flags: u16,
    /* We chain unused descriptors via this, too */
    next: u16,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct VringAvail {
    flags: u16,
    idx: u16,
    ring: [u16; 512],
}

#[repr(C)]
#[derive(Copy, Clone)]
struct VringUsedElem {
    pub id: u32,
    pub len: u32,
}
#[derive(Copy, Clone)]
pub struct VirtqData {
    pub ready: usize,
    pub vq_index: usize,
    pub num: usize,

    pub last_avail_idx: u16,
    pub last_used_idx: u16,
    pub used_flags: u16,

    pub desc_table_ipa: usize,
    pub avail_ipa: usize,
    pub used_ipa: usize,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct VringUsed {
    flags: u16,
    idx: u16,
    ring: [VringUsedElem; 512],
}

pub trait VirtioQueue {
    fn virtio_queue_init(&self, dev_type: VirtioDeviceType);
    fn virtio_queue_reset(&self, index: usize);
}

#[derive(Clone)]
pub struct Virtq {
    inner: Arc<Mutex<VirtqInner<'static>>>,
}

impl Virtq {
    pub fn default() -> Virtq {
        Virtq {
            inner: Arc::new(Mutex::new(VirtqInner::default())),
        }
    }

    pub fn reset(&self, index: usize) {
        let mut inner = self.inner.lock();
        inner.reset(index);
    }

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

    pub fn avail_is_avail(&self) -> bool {
        let inner = self.inner.lock();
        inner.avail.is_some()
    }

    pub fn disable_notify(&self) {
        let mut inner = self.inner.lock();
        if inner.used_flags & VRING_USED_F_NO_NOTIFY as u16 != 0 {
            return;
        }
        inner.used_flags |= VRING_USED_F_NO_NOTIFY as u16;
    }

    pub fn enable_notify(&self) {
        let mut inner = self.inner.lock();
        if inner.used_flags & VRING_USED_F_NO_NOTIFY as u16 == 0 {
            return;
        }
        inner.used_flags &= !VRING_USED_F_NO_NOTIFY as u16;
    }

    pub fn check_avail_idx(&self, avail_idx: u16) -> bool {
        let inner = self.inner.lock();
        inner.last_avail_idx == avail_idx
    }

    pub fn desc_is_writable(&self, idx: usize) -> bool {
        let inner = self.inner.lock();
        let desc_table = inner.desc_table.as_ref().unwrap();
        desc_table[idx].flags & VIRTQ_DESC_F_WRITE != 0
    }

    pub fn desc_has_next(&self, idx: usize) -> bool {
        let inner = self.inner.lock();
        let desc_table = inner.desc_table.as_ref().unwrap();
        desc_table[idx].flags & VIRTQ_DESC_F_NEXT != 0
    }

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

    pub fn set_notify_handler(&self, handler: fn(Virtq, VirtioMmio, Vm) -> bool) {
        let mut inner = self.inner.lock();
        inner.notify_handler = Some(handler);
    }

    pub fn call_notify_handler(&self, mmio: VirtioMmio) -> bool {
        let inner = self.inner.lock();
        match inner.notify_handler {
            Some(handler) => {
                drop(inner);
                // println!("call_notify_handler");
                // println!("handler addr {:x}", unsafe { *(&handler as *const _ as *const usize) });
                handler(self.clone(), mmio, active_vm().unwrap())
            }
            None => {
                error!("call_notify_handler: virtq notify handler is None");
                false
            }
        }
    }

    pub fn show_desc_info(&self, size: usize, vm: Vm) {
        let inner = self.inner.lock();
        let desc = inner.desc_table.as_ref().unwrap();
        info!("[*desc_ring*]");
        for i in 0..size {
            let desc_addr = vm_ipa2pa(vm.clone(), desc[i].addr);
            info!(
                "index {}   desc_addr_ipa 0x{:x}   desc_addr_pa 0x{:x}   len 0x{:x}   flags {}  next {}",
                i, desc[i].addr, desc_addr, desc[i].len, desc[i].flags, desc[i].next
            );
        }
    }

    pub fn show_avail_info(&self, size: usize) {
        let inner = self.inner.lock();
        let avail = inner.avail.as_ref().unwrap();
        info!("[*avail_ring*]");
        for i in 0..size {
            info!("index {} ring_idx {}", i, avail.ring[i]);
        }
    }

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

    pub fn show_addr_info(&self) {
        let inner = self.inner.lock();
        info!(
            "avail_addr {:x}, desc_addr {:x}, used_addr {:x}",
            inner.avail_addr, inner.desc_table_addr, inner.used_addr
        );
    }

    pub fn set_last_used_idx(&self, last_used_idx: u16) {
        let mut inner = self.inner.lock();
        inner.last_used_idx = last_used_idx;
    }

    pub fn set_num(&self, num: usize) {
        let mut inner = self.inner.lock();
        inner.num = num;
    }

    pub fn set_ready(&self, ready: usize) {
        let mut inner = self.inner.lock();
        inner.ready = ready;
    }

    pub fn or_desc_table_addr(&self, addr: usize) {
        let mut inner = self.inner.lock();
        inner.desc_table_addr |= addr;
    }

    pub fn or_avail_addr(&self, addr: usize) {
        let mut inner = self.inner.lock();
        inner.avail_addr |= addr;
    }

    pub fn or_used_addr(&self, addr: usize) {
        let mut inner = self.inner.lock();
        inner.used_addr |= addr;
    }

    pub fn set_desc_table(&self, addr: usize) {
        let mut inner = self.inner.lock();
        if trace() && addr < 0x1000 {
            panic!("illegal desc ring addr {:x}", addr);
        }
        inner.desc_table = Some(unsafe { slice::from_raw_parts_mut(addr as *mut VringDesc, DESC_QUEUE_SIZE) });
    }

    pub fn set_avail(&self, addr: usize) {
        if trace() && addr < 0x1000 {
            panic!("illegal avail ring addr {:x}", addr);
        }
        let mut inner = self.inner.lock();
        inner.avail = Some(unsafe { &mut *(addr as *mut VringAvail) });
    }

    pub fn set_used(&self, addr: usize) {
        if trace() && addr < 0x1000 {
            panic!("illegal used ring addr {:x}", addr);
        }
        let mut inner = self.inner.lock();
        inner.used = Some(unsafe { &mut *(addr as *mut VringUsed) });
    }

    pub fn last_used_idx(&self) -> u16 {
        let inner = self.inner.lock();
        inner.last_used_idx
    }

    pub fn desc_table_addr(&self) -> usize {
        let inner = self.inner.lock();
        inner.desc_table_addr
    }

    pub fn avail_addr(&self) -> usize {
        let inner = self.inner.lock();
        inner.avail_addr
    }

    pub fn used_addr(&self) -> usize {
        let inner = self.inner.lock();
        inner.used_addr
    }

    pub fn desc_table(&self) -> usize {
        let inner = self.inner.lock();
        match &inner.desc_table {
            None => 0,
            Some(desc_table) => &(desc_table[0]) as *const _ as usize,
        }
    }

    pub fn avail(&self) -> usize {
        let inner = self.inner.lock();
        match &inner.avail {
            None => 0,
            Some(avail) => (*avail) as *const _ as usize,
        }
    }

    pub fn used(&self) -> usize {
        let inner = self.inner.lock();
        match &inner.used {
            None => 0,
            Some(used) => (*used) as *const _ as usize,
        }
    }

    pub fn ready(&self) -> usize {
        let inner = self.inner.lock();
        inner.ready
    }

    pub fn vq_indx(&self) -> usize {
        let inner = self.inner.lock();
        inner.vq_index
    }

    pub fn num(&self) -> usize {
        let inner = self.inner.lock();
        inner.num
    }

    pub fn desc_addr(&self, idx: usize) -> usize {
        let inner = self.inner.lock();
        let desc_table = inner.desc_table.as_ref().unwrap();
        desc_table[idx].addr
    }

    pub fn desc_flags(&self, idx: usize) -> u16 {
        let inner = self.inner.lock();
        let desc_table = inner.desc_table.as_ref().unwrap();
        desc_table[idx].flags
    }

    pub fn desc_next(&self, idx: usize) -> u16 {
        let inner = self.inner.lock();
        let desc_table = inner.desc_table.as_ref().unwrap();
        desc_table[idx].next
    }

    pub fn desc_len(&self, idx: usize) -> u32 {
        let inner = self.inner.lock();
        let desc_table = inner.desc_table.as_ref().unwrap();
        desc_table[idx].len
    }

    pub fn avail_flags(&self) -> u16 {
        let inner = self.inner.lock();
        let avail = inner.avail.as_ref().unwrap();
        avail.flags
    }

    pub fn avail_idx(&self) -> u16 {
        let inner = self.inner.lock();
        let avail = inner.avail.as_ref().unwrap();
        avail.idx
    }

    pub fn last_avail_idx(&self) -> u16 {
        let inner = self.inner.lock();
        inner.last_avail_idx
    }

    pub fn used_idx(&self) -> u16 {
        let inner = self.inner.lock();
        let used = inner.used.as_ref().unwrap();
        used.idx
    }
}

pub struct VirtqInner<'a> {
    ready: usize,
    vq_index: usize,
    num: usize,
    desc_table: Option<&'a mut [VringDesc]>,
    avail: Option<&'a mut VringAvail>,
    used: Option<&'a mut VringUsed>,
    last_avail_idx: u16,
    last_used_idx: u16,
    used_flags: u16,

    desc_table_addr: usize,
    avail_addr: usize,
    used_addr: usize,

    notify_handler: Option<fn(Virtq, VirtioMmio, Vm) -> bool>,
}

impl VirtqInner<'_> {
    pub fn default() -> Self {
        VirtqInner {
            ready: 0,
            vq_index: 0,
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

            notify_handler: None,
        }
    }

    // virtio_queue_reset
    pub fn reset(&mut self, index: usize) {
        self.ready = 0;
        self.vq_index = index;
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
