// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

pub const IOCTL_SYS: usize = 0x10;

// ioctl_sys_event
pub const IOCTL_SYS_GET_STATE: usize = 0;
pub const IOCTL_SYS_RECEIVE_MSG: usize = 1;
pub const IOCTL_SYS_INIT_USR_PID: usize = 2;
pub const IOCTL_SYS_GET_SEND_IDX: usize = 3;
pub const IOCTL_SYS_GET_VMID: usize = 4;
pub const IOCTL_SYS_SET_KERNEL_IMG_NAME: usize = 5;
pub const IOCTL_SYS_GET_KERNEL_IMG_NAME: usize = 6;
pub const IOCTL_SYS_APPEND_MED_BLK: usize = 0x10;
