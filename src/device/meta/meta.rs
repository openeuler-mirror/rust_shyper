use core::ops::Range;
use alloc::collections::BTreeMap;
use alloc::boxed::Box;

use alloc::sync::Arc;
use spin::RwLock;

use crate::config::VmEmulatedDeviceConfig;
use crate::device::{EmuContext, EmuDev, EmuDeviceType, meta};
use crate::device::meta::dispatch;
use crate::kernel::Vm;
use crate::error::{ErrorKind, Result};

/// Metadata context containing device ID and emulation context.
#[derive(Debug, Clone, Copy)]
pub struct MetaContext {
    pub dev_id: usize,
    pub ctx: EmuContext,
}

/// Trait for meta devices, defining methods for instantiation and handling.
pub trait MetaDevice {
    fn new(vm: &Vm, dev_id: usize, args: &str) -> Result<Self>
    where
        Self: Sized;
    fn handle(&self, ctx: MetaContext);
}

/// Type alias for a boxed trait object representing a meta device.
pub type MetaDev = Box<dyn MetaDevice + Send + Sync>;

/// Static storage for meta devices, protected by a read-write lock.
static META_DEVICES: RwLock<BTreeMap<usize, MetaDev>> = RwLock::new(BTreeMap::new());

/// Handles the emulation of meta devices.
pub fn emu_meta_handler(dev_id: usize, ctx: &EmuContext) -> bool {
    let devs = META_DEVICES.read();
    if let Some(dev) = devs.get(&dev_id) {
        dev.handle(MetaContext { dev_id, ctx: *ctx });
        true
    } else {
        false
    }
}

/// Registers a meta device in the system.
pub fn register(dev_id: usize, vm: &Vm, cfg: &VmEmulatedDeviceConfig) -> Result<()> {
    let dev = dispatch(vm, dev_id, &cfg.name)?;
    let mut devs = META_DEVICES.write();
    if devs.contains_key(&dev_id) {
        return ErrorKind::AlreadyExists.into();
    }
    devs.insert(dev_id, dev);
    Ok(())
}

/// Unregisters a meta device from the system.
pub fn unregister(dev_id: usize) {
    let mut devs = META_DEVICES.write();
    devs.remove(&dev_id).unwrap();
}

pub struct EmuMetaDev {
    address_range: Range<usize>,
    dev_id: usize,
}

impl EmuDev for EmuMetaDev {
    fn emu_type(&self) -> crate::device::EmuDeviceType {
        crate::device::EmuDeviceType::EmuDeviceTMeta
    }

    fn address_range(&self) -> Range<usize> {
        self.address_range.clone()
    }

    fn handler(&self, emu_ctx: &EmuContext) -> bool {
        emu_meta_handler(self.dev_id, emu_ctx)
    }
}

pub fn emu_meta_init(
    dev_id: usize,
    vm: &Vm,
    cfg: &VmEmulatedDeviceConfig,
) -> core::result::Result<Arc<dyn EmuDev>, ()> {
    if cfg.emu_type == EmuDeviceType::EmuDeviceTMeta {
        if meta::register(dev_id, vm, cfg).is_ok() {
            Ok(Arc::new(EmuMetaDev {
                address_range: cfg.base_ipa..cfg.base_ipa + cfg.length,
                dev_id,
            }))
        } else {
            Err(())
        }
    } else {
        Err(())
    }
}
