use crate::kernel::{IntCtrlType, Vm};

impl Vm {
    /// Init the VM's interrupt controller mode.
    pub fn init_intc_mode(&self, intc_type: IntCtrlType) {
        use super::{GICC_CTLR_EN_BIT, GICC_CTLR_EOIMODENS_BIT};
        use cortex_a::registers::HCR_EL2;

        let (gich_ctlr, hcr) = match intc_type {
            IntCtrlType::Emulated => (
                (GICC_CTLR_EN_BIT | GICC_CTLR_EOIMODENS_BIT) as u32,
                (HCR_EL2::VM::Enable
                    + HCR_EL2::IMO::EnableVirtualIRQ
                    + HCR_EL2::FMO::EnableVirtualFIQ
                    + HCR_EL2::TSC::EnableTrapEl1SmcToEl2
                    + HCR_EL2::RW::EL1IsAarch64)
                    .value,
            ),
            IntCtrlType::Passthrough => (
                (GICC_CTLR_EN_BIT) as u32,
                (HCR_EL2::VM::Enable + HCR_EL2::RW::EL1IsAarch64 + HCR_EL2::TSC::EnableTrapEl1SmcToEl2).value,
            ),
        };

        for vcpu in self.vcpu_list() {
            debug!("vm {} vcpu {} set {:?} hcr", self.id(), vcpu.id(), intc_type);
            vcpu.set_gich_ctlr(gich_ctlr);
            vcpu.set_hcr(hcr);
        }
    }
}
