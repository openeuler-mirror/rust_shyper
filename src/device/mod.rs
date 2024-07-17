// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

//! Device module, including device tree, device model, and device emulation.

#[allow(unused_imports)]
pub use self::device::*;
pub use self::device_tree::*;
pub use self::emu::*;
pub use self::virtio::*;
#[cfg(feature = "memrsv")]
pub use self::memrsv::*;

mod device;
mod device_tree;
mod emu;
#[cfg(feature = "memrsv")]
mod memrsv;
pub mod meta;
mod virtio;
