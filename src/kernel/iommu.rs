use crate::arch::{smmu_add_device, smmu_vm_init};
use crate::kernel::Vm;

pub fn iommu_init() {
    if cfg!(feature = "tx2") {
        crate::arch::smmu_init();
        println!("IOMMU init ok");
    } else {
        println!("Platform not support IOMMU");
    }
}

pub fn iommmu_vm_init(vm: Vm) -> bool {
    if cfg!(feature = "tx2") {
        return smmu_vm_init(vm.clone());
    } else {
        println!("Platform not support IOMMU");
        return false;
    }
}

pub fn iommu_add_device(vm: Vm, stream_id: usize) -> bool {
    if cfg!(feature = "tx2") {
        return smmu_add_device(vm.iommu_ctx_id(), stream_id);
    } else {
        println!("Platform not support IOMMU");
        return false;
    }
}
