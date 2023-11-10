use core::sync::atomic::{AtomicBool, Ordering};
use crate::device::meta::*;
use crate::kernel::{current_cpu, Vm};
use crate::error::Result;

pub struct RockchipGuestCru {
    flag: AtomicBool,
}

impl MetaDevice for RockchipGuestCru {
    fn new(vm: &Vm, dev_id: usize, arg: &str) -> Result<Self> {
        info!("rk_cru: vm {}, dev {dev_id}: {arg}", vm.id());
        Ok(Self {
            flag: AtomicBool::new(false),
        })
    }

    fn handle(&self, ctx: MetaContext) {
        let dev_id = ctx.dev_id;
        let ctx = ctx.ctx;

        if ctx.write {
            let val = current_cpu().get_gpr(ctx.reg);
            trace!(
                "rk_cru: {} writing {:x} to {:x}, {:?}",
                dev_id,
                val,
                ctx.address,
                current_cpu().ctx().unwrap()
            );
            if ctx.address == 0xfd7c0834 || ctx.address == 0xfd7c087c {
                debug!("rk_cru: {dev_id} refusing to write at {:x}", ctx.address);
                self.flag.store(true, Ordering::SeqCst);
            } else if self.flag.load(Ordering::SeqCst) {
                debug!("rk_cru: {dev_id} still refusing to write at {:x}", ctx.address);
            } else {
                unsafe { ctx.write(val) };
            }
        } else {
            let val = unsafe { ctx.read() };
            current_cpu().set_gpr(ctx.reg, val);
        }
    }
}
