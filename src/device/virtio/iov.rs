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
use core::slice::from_raw_parts;

use spin::Mutex;

use crate::utils::{memcpy, trace};

/// Represents a Virtio I/O vector.
#[derive(Clone)]
pub struct VirtioIov {
    inner: Arc<Mutex<VirtioIovInner>>,
}

impl VirtioIov {
    /// Creates a new `VirtioIov` with default inner values.
    pub fn default() -> VirtioIov {
        VirtioIov {
            inner: Arc::new(Mutex::new(VirtioIovInner::default())),
        }
    }

    /// Clears the I/O vector by removing all data.
    pub fn clear(&self) {
        let mut inner = self.inner.lock();
        inner.vector.clear();
    }

    /// Adds a data segment to the I/O vector.
    pub fn push_data(&self, buf: usize, len: usize) {
        let mut inner = self.inner.lock();
        inner.vector.push(VirtioIovData { buf, len });
    }

    /// Retrieves the buffer address at the specified index.
    pub fn get_buf(&self, idx: usize) -> usize {
        let inner = self.inner.lock();
        inner.vector[idx].buf
    }

    /// Copies data from the I/O vector to the specified buffer.
    pub fn to_buf(&self, addr: usize, len: usize) {
        let mut size = len;
        let inner = self.inner.lock();
        for iov_data in &inner.vector {
            let offset = len - size;
            let dst = addr + offset;
            if iov_data.len >= size {
                // SAFETY:
                // We have both read and write access to the src and dst memory regions.
                // The copied size will not exceed the memory region.
                unsafe {
                    memcpy(dst as *const u8, iov_data.buf as *const u8, size);
                }
                break;
            } else {
                // SAFETY:
                // We have both read and write access to the src and dst memory regions.
                // The copied size will not exceed the memory region.
                unsafe {
                    memcpy(dst as *const u8, iov_data.buf as *const u8, iov_data.len);
                }
                size -= iov_data.len;
            }
        }
    }

    /// Copies data from the specified buffer to the I/O vector.
    pub fn from_buf(&self, addr: usize, len: usize) {
        let mut size = len;
        let inner = self.inner.lock();
        for iov_data in &inner.vector {
            let offset = len - size;
            let src = addr + offset;
            if iov_data.len >= size {
                // SAFETY:
                // We have both read and write access to the src and dst memory regions.
                // The copied size will not exceed the memory region.
                unsafe {
                    memcpy(iov_data.buf as *const u8, src as *const u8, size);
                }
                break;
            } else {
                // SAFETY:
                // We have both read and write access to the src and dst memory regions.
                // The copied size will not exceed the memory region.
                unsafe {
                    memcpy(iov_data.buf as *const u8, src as *const u8, iov_data.len);
                }
                size -= iov_data.len;
            }
        }
    }

    /// Retrieves the number of data segments in the I/O vector.
    pub fn num(&self) -> usize {
        let inner = self.inner.lock();
        inner.vector.len()
    }

    /// Retrieves the length of the data segment at the specified index.
    pub fn get_len(&self, idx: usize) -> usize {
        let inner = self.inner.lock();
        inner.vector[idx].len
    }

    /// Retrieves a pointer to the data in the I/O vector.
    pub fn get_ptr(&self, size: usize) -> &'static [u8] {
        let inner = self.inner.lock();
        // let mut iov_idx = 0;
        let mut idx = size;

        for iov_data in &inner.vector {
            if iov_data.len > idx {
                if trace() && iov_data.buf + idx < 0x1000 {
                    panic!("illegal addr {:x}", iov_data.buf + idx);
                }
                // SAFETY:
                // The 'iov_data.buf' is a valid address, and iov_data.len is the length of the buffer.
                return unsafe { from_raw_parts((iov_data.buf + idx) as *const u8, 14) };
            } else {
                idx -= iov_data.len;
            }
        }

        debug!("iov get_ptr failed");
        debug!("get_ptr iov {:#?}", inner.vector);
        debug!("size {}, idx {}", size, idx);
        &[0]
    }

    /// Writes data from the I/O vector to another I/O vector.
    pub fn write_through_iov(&self, dst: VirtioIov, remain: usize) -> usize {
        let inner = self.inner.lock();

        let mut dst_iov_idx = 0;
        let mut src_iov_idx = 0;
        let mut dst_ptr = dst.get_buf(0);
        let mut src_ptr = inner.vector[0].buf;
        let mut dst_vlen_remain = dst.get_len(0);
        let mut src_vlen_remain = inner.vector[0].len;
        let mut remain = remain;

        while remain > 0 {
            if dst_iov_idx == dst.num() || src_iov_idx == inner.vector.len() {
                break;
            }

            let written;
            if dst_vlen_remain > src_vlen_remain {
                written = src_vlen_remain;
                if trace() && (dst_ptr < 0x1000 || src_ptr < 0x1000) {
                    panic!("illegal des addr {:x}, src addr {:x}", dst_ptr, src_ptr);
                }
                // SAFETY:
                // We have both read and write access to the src and dst memory regions.
                // The copied size will not exceed the memory region.
                unsafe {
                    memcpy(dst_ptr as *const u8, src_ptr as *const u8, written);
                }
                src_iov_idx += 1;
                if src_iov_idx < inner.vector.len() {
                    src_ptr = inner.vector[src_iov_idx].buf;
                    src_vlen_remain = inner.vector[src_iov_idx].len;
                    dst_ptr += written;
                    dst_vlen_remain -= written;
                }
            } else {
                written = dst_vlen_remain;
                if trace() && (dst_ptr < 0x1000 || src_ptr < 0x1000) {
                    panic!("illegal des addr {:x}, src addr {:x}", dst_ptr, src_ptr);
                }
                // SAFETY:
                // We have both read and write access to the src and dst memory regions.
                // The copied size will not exceed the memory region.
                unsafe {
                    memcpy(dst_ptr as *const u8, src_ptr as *const u8, written);
                }
                dst_iov_idx += 1;
                if dst_iov_idx < dst.num() {
                    dst_ptr = dst.get_buf(dst_iov_idx);
                    dst_vlen_remain = dst.get_len(dst_iov_idx);
                    src_ptr += written;
                    src_vlen_remain -= written;
                }
                if inner.vector[src_iov_idx].len == 0 {
                    src_iov_idx += 1;
                    if src_iov_idx < inner.vector.len() {
                        src_ptr = inner.vector[src_iov_idx].buf;
                        src_vlen_remain = inner.vector[src_iov_idx].len;
                    }
                }
            }
            remain -= written;
        }

        remain
    }
}

/// Represents a data segment in the Virtio I/O vector.
#[derive(Debug)]
struct VirtioIovData {
    buf: usize,
    len: usize,
}

/// Represents the inner data structure for `VirtioIov`.
#[derive(Debug)]
struct VirtioIovInner {
    vector: Vec<VirtioIovData>,
}

impl VirtioIovInner {
    /// Creates a new `VirtioIovInner` with an empty vector.
    pub fn default() -> VirtioIovInner {
        VirtioIovInner { vector: Vec::new() }
    }
}
