// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

pub use self::config::*;
#[cfg(feature = "static-config")]
pub use self::vm_def::*;
#[cfg(feature = "pi4")]
pub use self::pi4_def::*;
#[cfg(feature = "qemu")]
pub use self::qemu_def::*;
#[cfg(feature = "tx2")]
pub use self::tx2_def::*;

mod config;
#[cfg(feature = "pi4")]
mod pi4_def;
#[cfg(feature = "qemu")]
mod qemu_def;
#[cfg(feature = "tx2")]
mod tx2_def;
#[cfg(feature = "static-config")]
mod vm_def;
