pub mod cache;
mod context_frame;
mod cpu;
mod exception;
pub mod interface;
pub mod interrupt;
mod page_fault;
mod page_table;
mod plic;
pub mod power;
pub mod regs;
#[cfg(not(feature = "sbi_legacy"))]
mod sbicall;
#[cfg(feature = "sbi_legacy")]
mod sbicall_legacy;
mod smmu;
mod start;
pub mod timer;
pub mod tlb;
mod vcpu;
mod vm;
mod vplic;

use alloc::sync::Arc;
pub use cache::*;
pub use interface::*;
pub use interrupt::*;
pub use context_frame::*;
pub use plic::*;
pub use regs::*;
#[cfg(not(feature = "sbi_legacy"))]
pub use sbicall::*;
#[cfg(feature = "sbi_legacy")]
pub use sbicall_legacy::*;
pub use start::*;
pub use timer::*;
pub use tlb::*;
pub use vplic::*;
pub use page_table::*;
pub use power::*;
pub use cpu::*;
pub use smmu::*;
pub use page_fault::*;

/// TODO: fake implementations
pub struct GicDesc {
    pub gicd_addr: usize,
    pub gicc_addr: usize,
    pub gich_addr: usize,
    pub gicv_addr: usize,
    pub maintenance_int_id: usize,
}

pub struct SmmuDesc {
    pub base: usize,
    pub interrupt_id: usize,
    pub global_mask: u16,
}

#[repr(C)]
pub struct ArchDesc {
    pub gic_desc: GicDesc,
    pub smmu_desc: SmmuDesc,
}

pub const GIC_SGIS_NUM: usize = 16;

#[derive(Default)]
pub struct GicContext;

#[allow(unused_variables)]
pub fn gicc_clear_current_irq(for_hypervisor: bool) {
    IntCtrl::clear();
}

use crate::{config::VmEmulatedDeviceConfig, device::EmuDev};

use super::InterruptController;

#[allow(unused_variables)]
pub fn emu_smmu_init(emu_cfg: &VmEmulatedDeviceConfig) -> Result<Arc<dyn EmuDev>, ()> {
    todo!()
}
