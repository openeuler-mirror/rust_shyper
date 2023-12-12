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
use core::ops::Range;
use spin::Mutex;

use crate::arch::*;
use crate::mm::PageFrame;

use self::Error::*;

#[derive(Copy, Clone, Debug)]
/// Error type for page pool allocator.
pub enum Error {
    /// No free page frame.
    OutOfFrame,
    /// Free a page frame that is not allocated.
    FreeNotAllocated,
}

struct PagePool {
    free: Vec<usize>,
    allocated: Vec<usize>,
}

/// PagePoolTrait is a trait for page pool allocator, which manages a range of memory and allocates/free page frames.
pub trait PagePoolTrait {
    fn init(&mut self, range: Range<usize>);
    fn allocate(&mut self) -> Result<PageFrame, Error>;
    fn free(&mut self, pa: usize) -> Result<(), Error>;
}

impl PagePoolTrait for PagePool {
    fn init(&mut self, range: Range<usize>) {
        assert_eq!(range.start % PAGE_SIZE, 0);
        assert_eq!(range.end % PAGE_SIZE, 0);
        for pa in range.step_by(PAGE_SIZE) {
            self.free.push(pa);
        }
    }

    fn allocate(&mut self) -> Result<PageFrame, Error> {
        if let Some(pa) = self.free.pop() {
            self.allocated.push(pa);
            Ok(PageFrame::new(pa))
        } else {
            Err(OutOfFrame)
        }
    }

    fn free(&mut self, pa: usize) -> Result<(), Error> {
        if !self.allocated.contains(&pa) {
            Err(FreeNotAllocated)
        } else {
            self.allocated.retain(|p| { *p != pa });
            self.free.push(pa);
            Ok(())
        }
    }
}


static PAGE_POOL: Mutex<PagePool> = Mutex::new(PagePool {
    free: Vec::new(),
    allocated: Vec::new(),
});

/// Initialize the page pool allocator.
pub fn init() {
    let range = super::config::paged_range();
    let mut pool = PAGE_POOL.lock();
    pool.init(range);
}

/// Allocate a page frame, panic when error happens.
pub fn alloc() -> PageFrame {
    let mut pool = PAGE_POOL.lock();
    if let Ok(frame) = pool.allocate() {
        frame
    } else {
        panic!("page_pool: alloc failed")
    }
}


/// Try to alloc a page frame, return the error when error happens.
pub fn try_alloc() -> Result<PageFrame, Error> {
    let mut pool = PAGE_POOL.lock();
    let r = pool.allocate();
    r
}

/// Free a page frame, return the error when error happens.
pub fn free(pa: usize) -> Result<(), Error> {
    let mut pool = PAGE_POOL.lock();
    pool.free(pa)
}