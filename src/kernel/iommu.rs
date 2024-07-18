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

use crate::arch::{smmu_add_device, smmu_vm_init};
use crate::device::EmuDev;
use crate::config::VmEmulatedDeviceConfig;
use crate::kernel::Vm;

/// init iommu
pub fn iommu_init() {
    if cfg!(feature = "tx2") {
        crate::arch::smmu_init();
        info!("IOMMU init ok");
    } else {
        warn!("Platform not support IOMMU");
    }
}

/// init iommu for vm
pub fn iommmu_vm_init(vm: &Vm) -> bool {
    if cfg!(feature = "tx2") {
        smmu_vm_init(vm)
    } else {
        warn!("Platform not support IOMMU");
        false
    }
}

/// add device to iommu
pub fn iommu_add_device(vm: &Vm, stream_id: usize) -> bool {
    if cfg!(feature = "tx2") {
        smmu_add_device(vm.iommu_ctx_id(), stream_id)
    } else {
        warn!("Platform not support IOMMU");
        false
    }
}

/// init emu_iommu for vm
pub fn emu_iommu_init(emu_cfg: &VmEmulatedDeviceConfig) -> Result<Arc<dyn EmuDev>, ()> {
    if cfg!(feature = "tx2") {
        crate::arch::emu_smmu_init(emu_cfg)
    } else {
        Err(())
    }
}
