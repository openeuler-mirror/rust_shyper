// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use alloc::vec::Vec;

use spin::Mutex;

use crate::device::{virtio_blk_notify_handler, VIRTIO_BLK_T_IN, VIRTIO_BLK_T_OUT};
use crate::kernel::{
    active_vm, async_task_exe, AsyncTaskState, finish_async_task, hvc_send_msg_to_vm, HvcDefaultMsg, HvcGuestMsg,
    IpiInnerMsg, set_front_io_task_state, vm_ipa2pa, vm_list_walker,
};
use crate::kernel::IpiMessage;

/// Mutex for the list of mediated block devices.
pub static MEDIATED_BLK_LIST: Mutex<Vec<MediatedBlk>> = Mutex::new(Vec::new());

/// Adds a mediated block to the list and assigns it to a VM if needed.
pub fn mediated_blk_list_push(mut blk: MediatedBlk) {
    let mut list = MEDIATED_BLK_LIST.lock();
    vm_list_walker(|vm| {
        if let Some(id) = vm.config().mediated_block_index() {
            if id == list.len() {
                info!("Assign blk[{}] to VM {}", list.len(), vm.id());
                blk.avail = false;
                #[cfg(feature = "static-config")]
                {
                    // NOTE: here, VM0 must monopolize Core 0
                    use crate::vmm::vmm_boot_vm;
                    vmm_boot_vm(vm.id());
                }
            }
        }
    });
    list.push(blk);
}

/// Requests a mediated block device from the list.
/// Returns the index of the available block device.
// TODO: not concern abort the num of sectors
pub fn mediated_blk_request() -> Result<usize, ()> {
    let mut list = MEDIATED_BLK_LIST.lock();
    for (idx, blk) in list.iter_mut().enumerate() {
        if blk.avail {
            blk.avail = false;
            return Ok(idx);
        }
    }
    Err(())
}

/// Frees a mediated block device back to the list.
pub fn mediated_blk_free(idx: usize) {
    let mut list = MEDIATED_BLK_LIST.lock();
    list[idx].avail = true;
}

/// Retrieves a mediated block device from the list by index.
pub fn mediated_blk_list_get(idx: usize) -> MediatedBlk {
    let list = MEDIATED_BLK_LIST.lock();
    list[idx].clone()
}

/// Retrieves a mediated block device from the list by physical address.
pub fn mediated_blk_list_get_from_pa(pa: usize) -> Option<MediatedBlk> {
    let list = MEDIATED_BLK_LIST.lock();
    for blk in &*list {
        if blk.content as usize == pa {
            return Some(blk.clone());
        }
    }
    None
}

/// Represents a mediated block device.
#[derive(Clone)]
pub struct MediatedBlk {
    content: *mut MediatedBlkContent,
    avail: bool, // mediated blk will not be removed after append
}

// SAFETY: MediatedBlk is only used in VM0 and VM0 is only one core when muti-vm is supported
unsafe impl Send for MediatedBlk {}

impl MediatedBlk {
    /// # Safety:
    /// Addr must be a valid MMIO address of virtio-blk config
    pub unsafe fn from_addr(addr: usize) -> Self {
        Self {
            content: unsafe { &mut *(addr as *mut MediatedBlkContent) },
            avail: true,
        }
    }

    fn content(&self) -> &'static mut MediatedBlkContent {
        // SAFETY: 'content' is a valid pointer after `from_addr`
        unsafe { &mut *self.content }
    }

    /// Retrieves the maximum number of DMA blocks supported by the mediated block.
    pub fn dma_block_max(&self) -> usize {
        self.content().cfg.dma_block_max
    }

    /// Retrieves the number of requests in the mediated block.
    pub fn nreq(&self) -> usize {
        self.content().nreq
    }

    /// Retrieves the IPA (Intermediate Physical Address) of the cache associated with the mediated block.
    pub fn cache_ipa(&self) -> usize {
        self.content().cfg.cache_ipa
    }

    /// Retrieves the physical address of the cache associated with the mediated block.
    pub fn cache_pa(&self) -> usize {
        self.content().cfg.cache_pa
    }

    /// Sets the number of requests in the mediated block.
    pub fn set_nreq(&self, nreq: usize) {
        self.content().nreq = nreq;
    }

    /// Sets the type of request for the mediated block.
    pub fn set_type(&self, req_type: usize) {
        self.content().req.req_type = req_type as u32;
    }

    /// Sets the sector for the mediated block request.
    pub fn set_sector(&self, sector: usize) {
        self.content().req.sector = sector;
    }

    /// Sets the count for the mediated block request.
    pub fn set_count(&self, count: usize) {
        self.content().req.count = count;
    }

    /// Sets the physical address of the cache associated with the mediated block.
    pub fn set_cache_pa(&self, cache_pa: usize) {
        self.content().cfg.cache_pa = cache_pa;
    }
}

/// Represents the content of a mediated block.
#[repr(C)]
pub struct MediatedBlkContent {
    nreq: usize,
    cfg: MediatedBlkCfg,
    req: MediatedBlkReq,
}

/// Represents the configuration of a mediated block.
#[repr(C)]
pub struct MediatedBlkCfg {
    name: [u8; 32],
    block_dev_path: [u8; 32],
    block_num: usize,
    dma_block_max: usize,
    cache_size: usize,
    idx: u16,
    // TODO: enable page cache
    pcache: bool,
    cache_va: usize,
    cache_ipa: usize,
    cache_pa: usize,
}

/// Represents the request of a mediated block.
#[repr(C)]
pub struct MediatedBlkReq {
    req_type: u32,
    sector: usize,
    count: usize,
}

/// Appends a mediated block device to VM0 and the mediated block list.
// only run in vm0
pub fn mediated_dev_append(_class_id: usize, mmio_ipa: usize) -> Result<usize, ()> {
    let vm = active_vm().unwrap();
    let blk_pa = vm_ipa2pa(&vm, mmio_ipa);
    // TODO: check weather the blk_pa is valid
    // SAFETY:'blk_pa' is valid MMIO address of virtio-blk config
    let mediated_blk = unsafe { MediatedBlk::from_addr(blk_pa) };
    mediated_blk.set_nreq(0);

    let cache_pa = vm_ipa2pa(&vm, mediated_blk.cache_ipa());
    info!(
        "mediated_dev_append: dev_ipa_reg 0x{:x}, cache ipa 0x{:x}, cache_pa 0x{:x}, dma_block_max 0x{:x}",
        mmio_ipa,
        mediated_blk.cache_ipa(),
        cache_pa,
        mediated_blk.dma_block_max()
    );
    mediated_blk.set_cache_pa(cache_pa);
    mediated_blk_list_push(mediated_blk);
    Ok(0)
}

/// Handles the completion of a mediated block request and notifies the requested VM.
// service VM finish blk request, and inform the requested VM
pub fn mediated_blk_notify_handler(dev_ipa_reg: usize) -> Result<usize, ()> {
    let dev_pa_reg = vm_ipa2pa(&active_vm().unwrap(), dev_ipa_reg);

    // check weather src vm is still alive
    let mediated_blk = match mediated_blk_list_get_from_pa(dev_pa_reg) {
        Some(blk) => blk,
        None => {
            error!("illegal mediated blk pa {:x} ipa {:x}", dev_pa_reg, dev_ipa_reg);
            return Err(());
        }
    };
    if !mediated_blk.avail {
        // finish current IO task
        set_front_io_task_state(AsyncTaskState::Finish);
    } else {
        warn!("Mediated blk not belong to any VM");
    }
    // invoke the excuter to handle finished IO task
    async_task_exe();
    Ok(0)
}

// call by normal VMs ipi request (generated by mediated virtio blk)
pub fn mediated_ipi_handler(msg: IpiMessage) {
    // println!("core {} mediated_ipi_handler", current_cpu().id);
    if let IpiInnerMsg::MediatedMsg(mediated_msg) = msg.ipi_message {
        // generate IO request in `virtio_blk_notify_handler`
        virtio_blk_notify_handler(mediated_msg.vq, mediated_msg.blk, mediated_msg.src_vm);
        // mark the ipi task as finish (pop it from the ipi queue)
        finish_async_task(true);
        // invoke the executor to do IO request
        async_task_exe();
    }
}

/// Initiates a read operation on a mediated block device.
pub fn mediated_blk_read(blk_idx: usize, sector: usize, count: usize) {
    let mediated_blk = mediated_blk_list_get(blk_idx);
    let nreq = mediated_blk.nreq();
    mediated_blk.set_nreq(nreq + 1);
    mediated_blk.set_type(VIRTIO_BLK_T_IN);
    mediated_blk.set_sector(sector);
    mediated_blk.set_count(count);

    let med_msg = HvcDefaultMsg {
        fid: 3,    // HVC_MEDIATED
        event: 50, // HVC_MEDIATED_DEV_NOTIFY
    };

    if !hvc_send_msg_to_vm(0, &HvcGuestMsg::Default(med_msg)) {
        error!("mediated_blk_read: failed to notify VM 0");
    }
}

/// Initiates a write operation on a mediated block device.
pub fn mediated_blk_write(blk_idx: usize, sector: usize, count: usize) {
    let mediated_blk = mediated_blk_list_get(blk_idx);
    let nreq = mediated_blk.nreq();
    mediated_blk.set_nreq(nreq + 1);
    mediated_blk.set_type(VIRTIO_BLK_T_OUT);
    mediated_blk.set_sector(sector);
    mediated_blk.set_count(count);

    let med_msg = HvcDefaultMsg {
        fid: 3,    // HVC_MEDIATED
        event: 50, // HVC_MEDIATED_DRV_NOTIFY
    };

    // println!("mediated_blk_write send msg to vm0");
    if !hvc_send_msg_to_vm(0, &HvcGuestMsg::Default(med_msg)) {
        error!("mediated_blk_write: failed to notify VM 0");
    }
}
