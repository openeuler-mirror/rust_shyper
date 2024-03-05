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

use crate::arch::PAGE_SIZE;
use crate::device::{mediated_blk_list_get, VirtioMmio, Virtq};
use crate::kernel::{
    active_vm_id, add_async_task, async_blk_id_req, async_blk_io_req, async_ipi_req, AsyncTask, AsyncTaskData,
    AsyncTaskState, IoAsyncMsg, IoIdAsyncMsg, IpiMediatedMsg, push_used_info, Vm,
};
use crate::utils::{memcpy, trace};

use super::mmio::VIRTIO_F_VERSION_1;

/* VIRTIO_QUEUE_MAX_SIZE */
pub const VIRTQUEUE_BLK_MAX_SIZE: usize = 256;
pub const VIRTQUEUE_NET_MAX_SIZE: usize = 256;

/* VIRTIO_BLK_FEATURES*/
pub const VIRTIO_BLK_F_SIZE_MAX: usize = 1 << 1;
pub const VIRTIO_BLK_F_SEG_MAX: usize = 1 << 2;

/* BLOCK PARAMETERS*/
pub const SECTOR_BSIZE: usize = 512;
pub const BLOCKIF_SIZE_MAX: usize = 128 * PAGE_SIZE;
pub const BLOCKIF_IOV_MAX: usize = 512;

/* BLOCK REQUEST TYPE*/
pub const VIRTIO_BLK_T_IN: usize = 0;
pub const VIRTIO_BLK_T_OUT: usize = 1;
pub const VIRTIO_BLK_T_FLUSH: usize = 4;
pub const VIRTIO_BLK_T_GET_ID: usize = 8;

/* BLOCK REQUEST STATUS*/
pub const VIRTIO_BLK_S_OK: usize = 0;
// pub const VIRTIO_BLK_S_IOERR: usize = 1;
pub const VIRTIO_BLK_S_UNSUPP: usize = 2;

pub fn blk_features() -> usize {
    VIRTIO_F_VERSION_1 | VIRTIO_BLK_F_SIZE_MAX | VIRTIO_BLK_F_SEG_MAX
}

/// Represents the geometry information of a block device.
#[repr(C)]
#[derive(Copy, Clone, Default)]
struct BlkGeometry {
    cylinders: u16,
    heads: u8,
    sectors: u8,
}

/// Represents the topology information of a block device.
#[repr(C)]
#[derive(Copy, Clone, Default)]
struct BlkTopology {
    // # of logical blocks per physical block (log2)
    physical_block_exp: u8,
    // offset of first aligned logical block
    alignment_offset: u8,
    // suggested minimum I/O size in blocks
    min_io_size: u16,
    // optimal (suggested maximum) I/O size in blocks
    opt_io_size: u32,
}

/// Represents a block descriptor.
pub struct BlkDesc {
    inner: BlkDescInner,
}

impl BlkDesc {
    /// Creates a default `BlkDesc` instance.
    pub fn new(bsize: usize) -> BlkDesc {
        let desc = BlkDescInner {
            capacity: bsize,
            size_max: BLOCKIF_SIZE_MAX as u32,
            seg_max: BLOCKIF_IOV_MAX as u32,
            ..Default::default()
        };
        BlkDesc { inner: desc }
    }

    /// Gets the start address of the block descriptor.
    pub fn start_addr(&self) -> usize {
        &self.inner.capacity as *const _ as usize
    }

    /// # Safety:
    /// Caller must ensure offset is valid
    /// Offset must valid for virtio_mmio
    pub unsafe fn offset_data(&self, offset: usize, width: usize) -> usize {
        let start_addr = self.start_addr();
        match width {
            1 => unsafe { *((start_addr + offset) as *const u8) as usize },
            2 => unsafe { *((start_addr + offset) as *const u16) as usize },
            4 => unsafe { *((start_addr + offset) as *const u32) as usize },
            8 => unsafe { *((start_addr + offset) as *const u64) as usize },
            _ => 0,
        }
    }
}

/// Represents the inner data structure of a block descriptor.
#[repr(C)]
#[derive(Copy, Clone, Default)]
pub struct BlkDescInner {
    capacity: usize,
    size_max: u32,
    seg_max: u32,
    geometry: BlkGeometry,
    blk_size: usize,
    topology: BlkTopology,
    writeback: u8,
    unused0: [u8; 3],
    max_discard_sectors: u32,
    max_discard_seg: u32,
    discard_sector_alignment: u32,
    max_write_zeroes_sectors: u32,
    max_write_zeroes_seg: u32,
    write_zeroes_may_unmap: u8,
    unused1: [u8; 3],
}

/// Represents a block I/O vector.
#[repr(C)]
#[derive(Clone)]
pub struct BlkIov {
    pub data_bg: usize,
    pub len: u32,
}

/// Represents a region of a block request.
#[repr(C)]
pub struct BlkReqRegion {
    pub start: usize,
    pub size: usize,
}

/// Represents a VirtioBlk request.
#[repr(C)]
pub struct VirtioBlkReq {
    region: BlkReqRegion,
    mediated: bool,
}

impl VirtioBlkReq {
    /// Creates a default `VirtioBlkReq` instance.
    pub fn default() -> VirtioBlkReq {
        VirtioBlkReq {
            region: BlkReqRegion { start: 0, size: 0 },
            mediated: false,
        }
    }

    /// Sets the start address of the block request region.
    pub fn set_start(&mut self, start: usize) {
        self.region.start = start;
    }

    /// Sets the size of the block request region.
    pub fn set_size(&mut self, size: usize) {
        self.region.size = size;
    }

    /// Sets whether the request is mediated.
    pub fn set_mediated(&mut self, mediated: bool) {
        self.mediated = mediated;
    }

    /// Checks if the request is mediated.
    pub fn mediated(&self) -> bool {
        self.mediated
    }

    /// Gets the start address of the block request region.
    pub fn region_start(&self) -> usize {
        self.region.start
    }

    /// Gets the size of the block request region.
    pub fn region_size(&self) -> usize {
        self.region.size
    }
}

/// Represents a node in a VirtioBlk request.
#[repr(C)]
#[derive(Clone)]
pub struct VirtioBlkReqNode {
    req_type: u32,
    reserved: u32,
    sector: usize,
    desc_chain_head_idx: u32,
    iov: Vec<BlkIov>,
    // sum up byte for req
    iov_sum_up: usize,
    // total byte for current req
    iov_total: usize,
}

impl VirtioBlkReqNode {
    /// Creates a default `VirtioBlkReqNode` instance.
    pub fn default() -> VirtioBlkReqNode {
        VirtioBlkReqNode {
            req_type: 0,
            reserved: 0,
            sector: 0,
            desc_chain_head_idx: 0,
            iov: vec![],
            iov_sum_up: 0,
            iov_total: 0,
        }
    }
}

/// Generates a block request using the provided parameters.
pub fn generate_blk_req(
    req: &VirtioBlkReq,
    vq: Arc<Virtq>,
    dev: Arc<VirtioMmio>,
    cache: usize,
    vm: Arc<Vm>,
    req_node_list: Vec<VirtioBlkReqNode>,
) {
    let region_start = req.region_start();
    let region_size = req.region_size();
    let mut cache_ptr = cache;
    for req_node in req_node_list {
        let sector = req_node.sector;
        if sector + req_node.iov_sum_up / SECTOR_BSIZE > region_start + region_size {
            warn!(
                "blk_req_handler: {} out of vm range",
                if req_node.req_type == VIRTIO_BLK_T_IN as u32 {
                    "read"
                } else {
                    "write"
                }
            );
            continue;
        }
        match req_node.req_type as usize {
            VIRTIO_BLK_T_IN => {
                if req.mediated() {
                    // mediated blk read
                    let task = AsyncTask::new(
                        AsyncTaskData::AsyncIoTask(IoAsyncMsg {
                            src_vmid: vm.id(),
                            vq: vq.clone(),
                            dev: dev.clone(),
                            io_type: VIRTIO_BLK_T_IN,
                            blk_id: vm.med_blk_id(),
                            sector: sector + region_start,
                            count: req_node.iov_sum_up / SECTOR_BSIZE,
                            cache,
                            iov_list: Arc::new(req_node.iov.clone()),
                        }),
                        vm.id(),
                        async_blk_io_req(),
                    );
                    add_async_task(task, false);
                } else {
                    todo!();
                }
                for iov in req_node.iov.iter() {
                    let data_bg = iov.data_bg;
                    let len = iov.len as usize;

                    if len < SECTOR_BSIZE {
                        warn!("blk_req_handler: read len < SECTOR_BSIZE");
                        continue;
                    }
                    if !req.mediated() {
                        if trace() && (data_bg < 0x1000 || cache_ptr < 0x1000) {
                            panic!("illegal des addr {:x}, src addr {:x}", data_bg, cache_ptr);
                        }
                        // SAFETY:
                        // We have both read and write access to the src and dst memory regions.
                        // The copied size will not exceed the memory region.
                        unsafe {
                            memcpy(data_bg as *mut u8, cache_ptr as *mut u8, len);
                        }
                    }
                    cache_ptr += len;
                }
            }
            VIRTIO_BLK_T_OUT => {
                for iov in req_node.iov.iter() {
                    let data_bg = iov.data_bg;
                    let len = iov.len as usize;
                    if len < SECTOR_BSIZE {
                        warn!("blk_req_handler: read len < SECTOR_BSIZE");
                        continue;
                    }
                    if !req.mediated() {
                        if trace() && (data_bg < 0x1000 || cache_ptr < 0x1000) {
                            panic!("illegal des addr {:x}, src addr {:x}", cache_ptr, data_bg);
                        }
                        // SAFETY:
                        // We have both read and write access to the src and dst memory regions.
                        // The copied size will not exceed the memory region.
                        unsafe {
                            memcpy(cache_ptr as *mut u8, data_bg as *mut u8, len);
                        }
                    }
                    cache_ptr += len;
                }
                if req.mediated() {
                    // mediated blk write
                    let task = AsyncTask::new(
                        AsyncTaskData::AsyncIoTask(IoAsyncMsg {
                            src_vmid: vm.id(),
                            vq: vq.clone(),
                            dev: dev.clone(),
                            io_type: VIRTIO_BLK_T_OUT,
                            blk_id: vm.med_blk_id(),
                            sector: sector + region_start,
                            count: req_node.iov_sum_up / SECTOR_BSIZE,
                            cache,
                            iov_list: Arc::new(req_node.iov.clone()),
                        }),
                        vm.id(),
                        async_blk_io_req(),
                    );
                    add_async_task(task, false);
                } else {
                    todo!();
                }
            }
            VIRTIO_BLK_T_FLUSH => {
                todo!();
            }
            VIRTIO_BLK_T_GET_ID => {
                let data_bg = req_node.iov[0].data_bg;
                let name = "virtio-blk".as_ptr();
                if trace() && (data_bg < 0x1000) {
                    panic!("illegal des addr {:x}", cache_ptr);
                }
                // SAFETY:
                // We have both read and write access to the src and dst memory regions.
                // The copied size will not exceed the memory region.
                unsafe {
                    memcpy(data_bg as *mut u8, name, 20);
                }
                let task = AsyncTask::new(
                    AsyncTaskData::AsyncNoneTask(IoIdAsyncMsg {
                        vq: vq.clone(),
                        dev: dev.clone(),
                    }),
                    vm.id(),
                    async_blk_id_req(),
                );
                task.set_state(AsyncTaskState::Finish);
                add_async_task(task, false);
            }
            _ => {
                warn!("Wrong block request type {} ", req_node.req_type);
                continue;
            }
        }

        // update used ring
        if !req.mediated() {
            todo!("reset num to vq size");
        } else {
            push_used_info(req_node.desc_chain_head_idx, req_node.iov_total as u32, vm.id());
        }
    }
}

/// Handles the notification for a mediated block request on the specified Virtqueue (`vq`) and Virtio block device (`blk`)
/// associated with the virtual machine (`vm`). This function creates an asynchronous IPI task to process the mediated
/// block request.
pub fn virtio_mediated_blk_notify_handler(vq: Arc<Virtq>, blk: Arc<VirtioMmio>, vm: Arc<Vm>) -> bool {
    let src_vmid = vm.id();
    let task = AsyncTask::new(
        AsyncTaskData::AsyncIpiTask(IpiMediatedMsg { src_vm: vm, vq, blk }),
        src_vmid,
        async_ipi_req(),
    );
    add_async_task(task, true);
    true
}

/// Handles the notification for a Virtio block request on the specified Virtqueue (`vq`) and Virtio block device (`blk`)
/// associated with the virtual machine (`vm`). This function processes the available descriptors in the Virtqueue and
/// generates block requests accordingly. The function returns `true` upon successful handling.
pub fn virtio_blk_notify_handler(vq: Arc<Virtq>, blk: Arc<VirtioMmio>, vm: Arc<Vm>) -> bool {
    if vm.id() == 0 && active_vm_id() == 0 {
        panic!("src vm should not be 0");
    }

    let avail_idx = vq.avail_idx();

    if vq.ready() == 0 {
        error!("blk virt_queue is not ready!");
        return false;
    }

    let dev = blk.dev();
    let req = match dev.req() {
        Some(blk_req) => blk_req,
        _ => {
            panic!("virtio_blk_notify_handler: illegal req")
        }
    };

    let mut next_desc_idx_opt = vq.pop_avail_desc_idx(avail_idx);
    let mut process_count: i32 = 0;
    let mut req_node_list: Vec<VirtioBlkReqNode> = vec![];

    // let time0 = time_current_us();

    while next_desc_idx_opt.is_some() {
        let mut next_desc_idx = next_desc_idx_opt.unwrap() as usize;
        vq.disable_notify();
        if vq.check_avail_idx(avail_idx) {
            vq.enable_notify();
        }

        let mut head = true;

        let mut req_node = VirtioBlkReqNode::default();
        req_node.desc_chain_head_idx = next_desc_idx as u32;

        loop {
            if vq.desc_has_next(next_desc_idx) {
                if head {
                    if vq.desc_is_writable(next_desc_idx) {
                        error!(
                            "Failed to get virt blk queue desc header, idx = {}, flag = {:x}",
                            next_desc_idx,
                            vq.desc_flags(next_desc_idx)
                        );
                        blk.notify();
                        return false;
                    }
                    head = false;
                    let vreq_addr = vm.ipa2pa(vq.desc_addr(next_desc_idx));
                    if vreq_addr == 0 {
                        error!("virtio_blk_notify_handler: failed to get vreq");
                        return false;
                    }
                    // SAFETY: 'vreq_addr' is checked
                    let vreq = unsafe { &mut *(vreq_addr as *mut VirtioBlkReqNode) };
                    req_node.req_type = vreq.req_type;
                    req_node.sector = vreq.sector;
                } else {
                    /*data handler*/
                    if (vq.desc_flags(next_desc_idx) & 0x2) as u32 >> 1 == req_node.req_type {
                        error!(
                            "Failed to get virt blk queue desc data, idx = {}, req.type = {}, desc.flags = {}",
                            next_desc_idx,
                            req_node.req_type,
                            vq.desc_flags(next_desc_idx)
                        );
                        blk.notify();
                        return false;
                    }
                    let data_bg = vm.ipa2pa(vq.desc_addr(next_desc_idx));
                    if data_bg == 0 {
                        error!("virtio_blk_notify_handler: failed to get iov data begin");
                        return false;
                    }

                    let iov = BlkIov {
                        data_bg,
                        len: vq.desc_len(next_desc_idx),
                    };
                    req_node.iov_sum_up += iov.len as usize;
                    req_node.iov.push(iov);
                }
            } else {
                /*state handler*/
                if !vq.desc_is_writable(next_desc_idx) {
                    error!("Failed to get virt blk queue desc status, idx = {}", next_desc_idx);
                    blk.notify();
                    return false;
                }
                let vstatus_addr = vm.ipa2pa(vq.desc_addr(next_desc_idx));
                if vstatus_addr == 0 {
                    error!("virtio_blk_notify_handler: vm[{}] failed to vstatus", vm.id());
                    return false;
                }
                // SAFETY: 'vstatus_addr' is checked
                let vstatus = unsafe { &mut *(vstatus_addr as *mut u8) };
                if req_node.req_type > 1 && req_node.req_type != VIRTIO_BLK_T_GET_ID as u32 {
                    *vstatus = VIRTIO_BLK_S_UNSUPP as u8;
                } else {
                    *vstatus = VIRTIO_BLK_S_OK as u8;
                }
                break;
            }
            next_desc_idx = vq.desc_next(next_desc_idx) as usize;
        }
        req_node.iov_total = req_node.iov_sum_up;
        req_node_list.push(req_node);

        process_count += 1;
        next_desc_idx_opt = vq.pop_avail_desc_idx(avail_idx);
    }

    if !req.mediated() {
        unimplemented!("!!req.mediated()");
    } else {
        let mediated_blk = mediated_blk_list_get(vm.med_blk_id());
        let cache = mediated_blk.cache_pa();
        generate_blk_req(req, vq.clone(), blk.clone(), cache, vm.clone(), req_node_list);
    };

    // let time1 = time_current_us();

    if vq.avail_flags() == 0 && process_count > 0 && !req.mediated() {
        trace!("virtio blk notify");
        blk.notify();
    }

    true
}
