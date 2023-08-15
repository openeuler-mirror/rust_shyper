// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

pub use self::platform_common::*;

#[cfg(feature = "pi4")]
pub use self::pi4::{Pi4Platform as Platform, PLAT_DESC};
#[cfg(feature = "qemu")]
pub use self::qemu::{QemuPlatform as Platform, PLAT_DESC};
#[cfg(feature = "tx2")]
pub use self::tx2::{Tx2Platform as Platform, PLAT_DESC};
pub use self::tx2::*;
#[cfg(feature = "rk3588")]
pub use self::rk3588::*;

#[cfg(feature = "pi4")]
mod pi4;
mod platform_common;
#[cfg(feature = "qemu")]
mod qemu;
#[cfg(feature = "rk3588")]
mod rk3588;
#[cfg(feature = "tx2")]
mod tx2;
