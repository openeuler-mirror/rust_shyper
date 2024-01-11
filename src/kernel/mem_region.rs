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

const TOTAL_MEM_REGION_MAX: usize = 16;

#[derive(Copy, Clone, Eq, Debug)]
/// Memory Region Descriptor Struct, used to describe a continuous platform memory region
pub struct MemRegion {
    pub base: usize,
    pub size: usize,
    // bit
    pub free: usize,
    // bit
    pub last: usize, // bit
}

impl PartialEq for MemRegion {
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base && self.size == other.size && self.free == other.free && self.last == other.last
    }
}

impl MemRegion {
    pub const fn new() -> MemRegion {
        MemRegion {
            base: 0,
            size: 0,
            free: 0,
            last: 0,
        }
    }

    pub fn init(&mut self, base: usize, size: usize, free: usize, last: usize) {
        self.base = base;
        self.size = size;
        self.free = free;
        self.last = last;
    }
}

/// Vm memory region struct
pub struct VmRegion {
    pub region: Vec<MemRegion>,
}

impl VmRegion {
    pub fn push(&mut self, region: MemRegion) {
        self.region.push(region);
    }
}

pub static VM_REGION: Mutex<VmRegion> = Mutex::new(VmRegion {
    region: Vec::<MemRegion>::new(),
});
