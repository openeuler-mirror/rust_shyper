use crate::{
    arch::{ContextFrameTrait, SSTATUS_FS, SSTATUS_VS},
    config::VmConfigEntry,
    kernel::{current_cpu, Vcpu, VmType},
};

use super::{A0_NUM, A1_NUM, SSTATUS_SPIE, SSTATUS_SPP};

impl Vcpu {
    /// init boot info, including argument and exception pc
    pub fn init_boot_info(&self, config: &VmConfigEntry) {
        // Since this function is not necessarily executed on the core corresponding to the vcpu,
        // the privilege level register should not be set directly, but the context variable should be set

        info!("init boot info for vcpu {}", self.id());

        let mut vcpu_inner = self.inner.inner_mut.lock();
        match config.os_type {
            VmType::VmTOs => {
                // a0 = hartid, a1 = dtb
                vcpu_inner.vcpu_ctx.set_gpr(A0_NUM, current_cpu().id); // a0 = hart_id
                vcpu_inner.vcpu_ctx.set_gpr(A1_NUM, config.device_tree_load_ipa());
                // a1 = dtb
            }
            _ => {
                let arg = &config.memory_region()[0];
                vcpu_inner.vcpu_ctx.set_argument(arg.ipa_start + arg.length);
            }
        }
        vcpu_inner.vcpu_ctx.set_exception_pc(config.kernel_entry_point());
        vcpu_inner.vcpu_ctx.sstatus = SSTATUS_SPIE | SSTATUS_SPP | SSTATUS_FS | SSTATUS_VS;
    }
}
