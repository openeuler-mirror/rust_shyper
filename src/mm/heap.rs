// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use buddy_system_allocator::LockedHeap;

// rCore buddy system allocator
use crate::arch::PAGE_SIZE;

const HEAP_SIZE: usize = 10 * 1024 * PAGE_SIZE; // 40MB

#[repr(align(4096))]
struct HeapRegion([u8; HEAP_SIZE]);

static mut HEAP_REGION: HeapRegion = HeapRegion([0; HEAP_SIZE]);

#[global_allocator]
pub static HEAP_ALLOCATOR: LockedHeap<32> = LockedHeap::empty();

pub fn heap_init() {
    unsafe {
        println!(
            "init buddy system, heap start from {:x}",
            HEAP_REGION.0.as_mut_ptr() as usize
        );
        HEAP_ALLOCATOR
            .lock()
            .init(HEAP_REGION.0.as_mut_ptr() as usize, HEAP_SIZE);
    }
}

#[alloc_error_handler]
fn alloc_error_handler(_: core::alloc::Layout) -> ! {
    panic!("alloc_error_handler: heap Out Of Memory");
}
