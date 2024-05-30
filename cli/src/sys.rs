// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use libc::{
    c_void, close, ioctl, lseek, open, read, size_t, MAP_LOCKED, MAP_SHARED, O_RDONLY, O_RDWR, PROT_READ, PROT_WRITE,
    SEEK_SET,
};

use crate::util::{bool_to_cint, file_size};

pub fn sys_reboot(force: bool) {
    unsafe {
        let fd = open("/dev/shyper\0".as_ptr() as *const u8, O_RDWR);
        println!("sys reboot fd {}", fd);
        if ioctl(fd, 0x0000, bool_to_cint(force)) != 0 {
            println!("err: ioctl fail!");
        }
        close(fd);
    }
}

pub fn sys_shutdown(force: bool) {
    unsafe {
        let fd = open("/dev/shyper\0".as_ptr() as *const u8, O_RDWR);
        println!("sys shutdown fd {}", fd);
        if ioctl(fd, 0x0001, bool_to_cint(force)) != 0 {
            println!("err: ioctl fail!");
        }
        close(fd);
    }
}

pub fn sys_test() {
    unsafe {
        let fd = open("/dev/shyper\0".as_ptr() as *const u8, O_RDWR);
        println!("sys test fd {}", fd);
        if ioctl(fd, 0x0004, 0) != 0 {
            println!("err: ioctl fail!");
        }
        close(fd);
    }
}

fn update_image(path: String) -> u64 {
    let size = file_size(&path).unwrap();
    if size == 0 {
        return 0;
    }

    unsafe {
        let share_mem_fd = open("/dev/hyper_update".as_ptr() as *const u8, O_RDWR);
        let addr = libc::mmap(
            0 as *mut c_void,
            0x8000000,
            PROT_READ | PROT_WRITE,
            MAP_SHARED | MAP_LOCKED,
            share_mem_fd,
            0,
        );
        if addr.is_null() {
            println!("[sys_update] update image mmap failed");
            return size;
        }
        let image_fd = open(path.as_ptr() as *const u8, O_RDONLY);

        // mkimage file has 64B header
        lseek(image_fd, 64, SEEK_SET);
        read(image_fd, addr, (size - 64) as size_t);
        close(image_fd);
        close(share_mem_fd);
    }
    size
}

pub fn sys_update(path: String) {
    let size = update_image(path.clone());
    if size == 0 {
        println!("File {} size is 0, abort", path);
        return;
    }

    unsafe {
        let fd = open("/dev/shyper\0".as_ptr() as *const u8, O_RDWR);
        if ioctl(fd, 0x0000, size) != 0 {
            println!("err: ioctl fail!");
        }
        close(fd);
    }
}
