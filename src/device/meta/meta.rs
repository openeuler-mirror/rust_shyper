use crate::config::VmEmulatedDeviceConfig;
use crate::device::EmuContext;
use crate::device::meta::dispatch;
use crate::kernel::Vm;
use crate::error::{ErrorKind, Result};
use alloc::collections::BTreeMap;
use alloc::boxed::Box;

use spin::RwLock;

#[derive(Debug, Clone, Copy)]
pub struct MetaContext {
    pub dev_id: usize,
    pub ctx: EmuContext,
}

pub trait MetaDevice {
    fn new(vm: &Vm, dev_id: usize, args: &str) -> Result<Self>
    where
        Self: Sized;
    fn handle(&self, ctx: MetaContext);
}

pub type MetaDev = Box<dyn MetaDevice + Send + Sync>;

static META_DEVICES: RwLock<BTreeMap<usize, MetaDev>> = RwLock::new(BTreeMap::new());

pub fn emu_meta_handler(dev_id: usize, ctx: &EmuContext) -> bool {
    let devs = META_DEVICES.read();
    if let Some(dev) = devs.get(&dev_id) {
        dev.handle(MetaContext { dev_id, ctx: *ctx });
        true
    } else {
        false
    }
}

pub fn register(dev_id: usize, vm: &Vm, cfg: &VmEmulatedDeviceConfig) -> Result<()> {
    let dev = dispatch(vm, dev_id, &cfg.name)?;
    let mut devs = META_DEVICES.write();
    if devs.contains_key(&dev_id) {
        return ErrorKind::AlreadyExists.into();
    }
    devs.insert(dev_id, dev);
    Ok(())
}

pub fn unregister(dev_id: usize) {
    let mut devs = META_DEVICES.write();
    devs.remove(&dev_id).unwrap();
}
