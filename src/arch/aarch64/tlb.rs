// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use crate::arch::{dsb, isb};

// Translation Look-a-side Buffer Instrution ('TLBI') operations.
// SAFETY:
// TLBI operations can't trigger any side effects on the safety of the system.
pub mod tlbi {
    macro_rules! define_tlbi {
        ($mode:ident) => {
            pub fn $mode() {
                unsafe {
                    sysop!(tlbi, $mode);
                }
            }
        };
    }

    macro_rules! define_tlbi_ipa {
        ($mode:ident) => {
            pub fn $mode(ipa: u64) {
                unsafe {
                    sysop!(tlbi, $mode, ipa);
                }
            }
        };
    }

    macro_rules! define_tlbi_va {
        ($mode:ident) => {
            pub fn $mode(va: usize) {
                unsafe {
                    sysop!(tlbi, $mode, va as u64);
                }
            }
        };
    }

    define_tlbi!(alle1is);
    define_tlbi!(alle1);
    define_tlbi!(alle2is);
    define_tlbi!(alle2);
    define_tlbi!(vmalle1);
    define_tlbi!(vmalle1is);
    define_tlbi!(vmalle12e1);
    define_tlbi!(vmalle12e1is);
    define_tlbi!(vmalls12e1is);
    define_tlbi_ipa!(ipas2e1is);
    define_tlbi_ipa!(ipas2e1);
    define_tlbi_ipa!(ipas2le1is);
    define_tlbi_ipa!(ipas2le1);
    define_tlbi_va!(vaae1);
    define_tlbi_va!(vaae1is);
    define_tlbi_va!(vae1is);
    define_tlbi_va!(vae1);
    define_tlbi_va!(vae2is);
    define_tlbi_va!(vae2);
}

/// invalidate all guest tlb entries
pub fn tlb_invalidate_guest_all() {
    dsb::ish();
    tlbi::vmalls12e1is();
    dsb::ish();
    isb();
}

/// invalidate all hypervisor tlb entries
pub fn invalid_hypervisor_all() {
    dsb::ish();
    tlbi::alle2is();
    dsb::ish();
    isb();
}
// TODO: add more TLB operations
