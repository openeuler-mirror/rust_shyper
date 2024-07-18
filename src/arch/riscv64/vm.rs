use crate::kernel::{IntCtrlType, Vm};

impl Vm {
    pub fn init_intc_mode(&self, intc_type: IntCtrlType) {
        // Note: do nothing here
        info!("arch_init_intc_mode: {:?}", intc_type);
    }
}
