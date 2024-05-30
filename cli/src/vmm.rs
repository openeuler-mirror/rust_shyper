// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use std::mem;

use libc::{close, ioctl, open, O_RDWR};
use log::{error, info};

use crate::util::cstr_arr_to_string;

pub const NAME_MAX_LEN: usize = 32;
const VM_NUM_MAX: usize = 16;
const VM_PAGE_SIZE: usize = 0x1000;

#[repr(C)]
#[derive(Clone)]
struct VMInfo {
    pub id: u32,
    pub vm_name: [u8; NAME_MAX_LEN],
    pub vm_type: u32,
    pub vm_state: u32,
}

const VM_T_LINUX: u32 = 0;
const VM_T_BARE_MATEL_APP: u32 = 1;
const VM_T_FREERTOS: u32 = 2;

const VM_S_INV: u32 = 0;
const VM_S_PENDING: u32 = 1;
const VM_S_ACTIVE: u32 = 2;

#[repr(C)]
struct VMInfoList {
    pub vm_num: usize,
    pub info_list: [VMInfo; VM_NUM_MAX],
}

pub fn vmm_boot(vm_id: u32) {
    unsafe {
        let fd = open("/dev/shyper\0".as_ptr() as *const u8, O_RDWR);
        if ioctl(fd, 0x0102, vm_id) != 0 {
            error!("err: ioctl fail!");
        }
        close(fd);
    }
}

pub fn vmm_shutdown(force: bool, vm_id: u32) {
    unsafe {
        let fd = open("/dev/shyper\0".as_ptr() as *const u8, O_RDWR);
        let arg: u64 = match force {
            true => ((1 << 16) | vm_id) as u64,
            false => vm_id as u64,
        };
        if ioctl(fd, 0x0103, arg) != 0 {
            error!("err: ioctl fail!");
        }
        close(fd);
    }
}

pub fn vmm_reboot(force: bool, vm_id: u32) {
    unsafe {
        let fd = open("/dev/shyper\0".as_ptr() as *const u8, O_RDWR);
        let arg: u64 = match force {
            true => ((1 << 16) | vm_id) as u64,
            false => vm_id as u64,
        };
        if ioctl(fd, 0x0104, arg) != 0 {
            error!("err: ioctl fail!");
        }
        close(fd);
    }
}

pub fn vmm_remove(vm_id: u32) {
    unsafe {
        let fd = open("/dev/shyper\0".as_ptr() as *const u8, O_RDWR);
        if ioctl(fd, 0x0110, vm_id) != 0 {
            error!("err: ioctl fail!");
        }
        close(fd);
    }
}

pub fn vmm_getvmid() {
    unsafe {
        let fd = open("/dev/shyper\0".as_ptr() as *const u8, O_RDWR);
        let mut id: u32 = 0;
        if ioctl(fd, 0x0108, &mut id as *mut u32) != 0 {
            error!("err: ioctl fail!");
        } else {
            info!("Current VM id is {}", id);
        }
        close(fd);
    }
}

pub fn vmm_list_vm_info() {
    let vm_info_list: VMInfoList;
    const VM_INFO_LIST_SIZE: usize = mem::size_of::<VMInfoList>();
    unsafe {
        let fd = open("/dev/shyper\0".as_ptr() as *const u8, O_RDWR);
        let buf: [u8; VM_PAGE_SIZE] = [0; VM_PAGE_SIZE];
        if ioctl(fd, 0x0100, buf.as_ptr()) != 0 {
            error!("err: ioctl fail!");
            close(fd);
            return;
        } else {
            vm_info_list =
                mem::transmute::<[u8; VM_INFO_LIST_SIZE], VMInfoList>(buf[0..VM_INFO_LIST_SIZE].try_into().unwrap());
        }
    }
    display_vm_list_info(vm_info_list);
}

fn display_vm_list_info(vm_info_list: VMInfoList) {
    let vm_num = vm_info_list.vm_num;
    for i in 0..vm_num {
        let info = &vm_info_list.info_list[i];
        println!("----------vm[{}]----------", i);
        println!(
            "vm id [{}] name: {} type: {}\n",
            info.id,
            cstr_arr_to_string(info.vm_name.as_slice()),
            info.vm_type
        );
        match info.vm_type {
            VM_T_LINUX => {
                println!("vm type: Linux");
            }
            VM_T_BARE_MATEL_APP => {
                println!("vm type: Bare Metal App");
            }
            VM_T_FREERTOS => {
                println!("vm type: FreeRTOS");
            }
            _ => {
                println!("vm type: illegal type");
            }
        }

        match info.vm_state {
            VM_S_INV => {
                println!("vm state: Inactive");
            }
            VM_S_PENDING => {
                println!("vm state: Pending");
            }
            VM_S_ACTIVE => {
                println!("vm state: Active");
            }
            _ => {
                println!("vm state: illegal state");
            }
        }
    }
}
