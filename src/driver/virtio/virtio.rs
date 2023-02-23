// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

pub const VIRTIO_CONFIG_S_ACKNOWLEDGE: usize = 1;
pub const VIRTIO_CONFIG_S_DRIVER: usize = 2;
pub const VIRTIO_CONFIG_S_DRIVER_OK: usize = 4;
pub const VIRTIO_CONFIG_S_FEATURES_OK: usize = 8;
pub const VIRTIO_CONFIG_S_NEEDS_RESET: usize = 0x40;
pub const VIRTIO_CONFIG_S_FAILED: usize = 0x80;

pub const VIRTIO_F_ANY_LAYOUT: usize = 27;
