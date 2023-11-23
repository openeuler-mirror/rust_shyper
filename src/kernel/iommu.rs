// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use crate::arch::{smmu_add_device, smmu_vm_init};
use crate::kernel::Vm;

pub fn iommu_init() {
    if cfg!(feature = "tx2") {
        crate::arch::smmu_init();
        info!("IOMMU init ok");
    } else {
        warn!("Platform not support IOMMU");
    }
}

pub fn iommmu_vm_init(vm: Vm) -> bool {
    if cfg!(feature = "tx2") {
        smmu_vm_init(vm)
    } else {
        warn!("Platform not support IOMMU");
        false
    }
}

pub fn iommu_add_device(vm: Vm, stream_id: usize) -> bool {
    if cfg!(feature = "tx2") {
        smmu_add_device(vm.iommu_ctx_id(), stream_id)
    } else {
        warn!("Platform not support IOMMU");
        false
    }
}
