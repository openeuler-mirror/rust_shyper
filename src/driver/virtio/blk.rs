// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

// Feature bits
pub const VIRTIO_BLK_F_SIZE_MAX: usize = 1;
pub const VIRTIO_BLK_F_SEG_MAX: usize = 2;
pub const VIRTIO_BLK_F_GEOMETRY: usize = 4;
pub const VIRTIO_BLK_F_RO: usize = 5;
pub const VIRTIO_BLK_F_BLK_SIZE: usize = 6;
pub const VIRTIO_BLK_F_TOPOLOGY: usize = 10;
pub const VIRTIO_BLK_F_MQ: usize = 12;

// Legacy feature bits
pub const VIRTIO_BLK_F_BARRIER: usize = 0;
pub const VIRTIO_BLK_F_SCSI: usize = 7;
pub const VIRTIO_BLK_F_FLUSH: usize = 9;
pub const VIRTIO_BLK_F_CONFIG_WCE: usize = 11;
