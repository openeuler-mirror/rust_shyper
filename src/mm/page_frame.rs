// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.
use alloc::alloc;
use core::alloc::Layout;

use crate::arch::PAGE_SIZE;
use crate::kernel::AllocError;

#[derive(Debug)]
/// PageFrame struct represents a page frame, consisting of physical address and page number.
pub struct PageFrame {
    pub pa: usize,
    pub page_num: usize,
    layout: Layout,
}

impl PageFrame {
    pub fn new(pa: usize, page_num: usize, layout: Layout) -> Self {
        Self { pa, page_num, layout }
    }

    /// Allocate a page frame with given page number.
    pub fn alloc_pages(page_num: usize) -> Result<Self, AllocError> {
        if page_num == 0 {
            return Err(AllocError::AllocZeroPage);
        }

        match Layout::from_size_align(page_num * PAGE_SIZE, PAGE_SIZE) {
            Ok(layout) => {
                let pa = unsafe { alloc::alloc_zeroed(layout) as usize };
                Ok(Self::new(pa, page_num, layout))
            }
            Err(err) => {
                error!("alloc_pages: Layout error {}", err);
                return Err(AllocError::OutOfFrame);
            }
        }
    }

    pub fn pa(&self) -> usize {
        self.pa
    }
}

impl Drop for PageFrame {
    fn drop(&mut self) {
        info!("drop page frame: {:#x}, with {} page(s)", self.pa, self.page_num);
        unsafe { alloc::dealloc(self.pa as *mut u8, self.layout) }
    }
}
