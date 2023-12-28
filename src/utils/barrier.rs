// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;

use crate::board::PLAT_DESC;
use super::round_up;

struct CpuSyncToken {
    n: usize,
    count: AtomicUsize,
}

static CPU_GLB_SYNC: CpuSyncToken = CpuSyncToken {
    n: PLAT_DESC.cpu_desc.num,
    count: AtomicUsize::new(0),
};

#[inline(never)]
/// Wait for all CPUs to reach the barrier.
pub fn barrier() {
    let ori = CPU_GLB_SYNC.count.fetch_add(1, Ordering::Release);
    let next_count = round_up(ori + 1, CPU_GLB_SYNC.n);
    while CPU_GLB_SYNC.count.load(Ordering::Acquire) < next_count {
        core::hint::spin_loop();
    }
}

pub fn reset_barrier() {
    CPU_GLB_SYNC.count.store(0, Ordering::Relaxed);
}
