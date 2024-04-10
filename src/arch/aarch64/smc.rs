// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use core::arch::asm;

#[inline(never)]
/// invoke a secure monitor call
/// # Safety:
/// It is unsafe to call this function directly.
/// The caller must ensure that
/// x0 is defined as the SMC function number referenced in the SMC Calling Convention
/// than the args later must be valid for the specified SMC function.
pub unsafe fn smc_call(x0: usize, x1: usize, x2: usize, x3: usize) -> (usize, usize, usize, usize) {
    #[cfg(target_arch = "aarch64")]
    {
        let r0;
        let r1;
        let r2;
        let r3;
        asm!(
            "smc #0",
            inout("x0") x0 => r0,
            inout("x1") x1 => r1,
            inout("x2") x2 => r2,
            inout("x3") x3 => r3,
            options(nomem, nostack)
        );
        (r0, r1, r2, r3)
    }

    #[cfg(not(target_arch = "aarch64"))]
    unimplemented!();
}
