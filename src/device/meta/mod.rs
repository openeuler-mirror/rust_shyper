//! Dispatches the creation of a meta device based on the provided arguments.

use crate::kernel::Vm;
use crate::error::{Result, ErrorKind};
use alloc::boxed::Box;

mod meta;
pub mod rk_cru;

pub use meta::*;

// TODO: dispatch via a derive macro and link-time symbols
fn dispatch(vm: &Vm, dev_id: usize, arg: &str) -> Result<MetaDev> {
    let p = arg.find(' ').unwrap_or(arg.len());
    Ok(Box::new(match &arg[..p] {
        "rk_cru" => rk_cru::RockchipGuestCru::new(vm, dev_id, arg),
        _ => {
            return ErrorKind::NotFound.into();
        }
    }?))
}
