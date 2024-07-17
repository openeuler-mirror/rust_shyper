// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use std::{
    ffi::CString,
    fs::File,
    io::{Read, Seek},
    mem,
    slice::from_raw_parts_mut,
};

use libc::{c_int, c_uchar, c_ulong, c_void, sysconf, _SC_PAGE_SIZE};

pub fn bool_to_cint(var: bool) -> c_int {
    if var {
        c_int::from(1)
    } else {
        c_int::from(0)
    }
}

pub fn file_size(path: &String) -> Result<u64, String> {
    if let Ok(file) = File::open(&path) {
        if let Ok(metadata) = file.metadata() {
            Ok(metadata.len())
        } else {
            Err(format!("Get file {} metadata err", path))
        }
    } else {
        Err(format!("Open file {} err", path))
    }
}

pub fn cstr_arr_to_string(buf: &[u8]) -> String {
    let mut vec: Vec<u8> = Vec::new();
    for i in buf {
        if *i == 0 {
            // terminate char
            break;
        } else {
            vec.push(*i);
        }
    }
    unsafe { CString::from_vec_unchecked(vec).to_string_lossy().into() }
}

pub fn string_to_cstr_arr(s: String) -> [u8; 32] {
    let mut buf: [u8; 32] = [0; 32];
    let s = s.as_bytes();
    for i in 0..s.len() {
        buf[i] = s[i];
    }
    buf
}

// convert program of pid's virtual address to physical address
// returns paddr
pub fn virt_to_phys_user(pid: u32, vaddr: u64) -> Result<u64, String> {
    let pagemap_path = format!("/proc/{}/pagemap", pid);
    let mut file = File::open(&pagemap_path).map_err(|err| format!("Open {} err: {}", &pagemap_path, err))?;

    let page_size: usize = unsafe { sysconf(_SC_PAGE_SIZE) } as usize;
    let offset = ((vaddr as usize) / page_size) * (mem::size_of::<c_ulong>() as usize);

    if file.seek(std::io::SeekFrom::Start(offset as u64)).is_err() {
        return Err(format!(
            "File {} is not big enough to access offset {}",
            pagemap_path, offset
        ));
    }

    let mut pagemap_entry: c_ulong = 0;
    if file
        .read_exact(unsafe { from_raw_parts_mut(&mut pagemap_entry as *mut _ as *mut u8, mem::size_of::<c_ulong>()) })
        .is_err()
    {
        return Err(format!("Read page table entry err"));
    }

    if (pagemap_entry & (1 << 63)) == 0 {
        return Err(format!(
            "Virtual Address 0x{:#x} converts to paddr err: page not in memory",
            vaddr
        ));
    }

    // Note: 以下注释是pagemap_entry每一位的意义
    // entry->soft_dirty = (data >> 55) & 1;
    // entry->file_page = (data >> 61) & 1;
    // entry->swapped = (data >> 62) & 1;
    // entry->present = (data >> 63) & 1;

    let pfn = pagemap_entry & ((1 << 55) - 1);
    let paddr = pfn * (page_size as u64) + vaddr % (page_size as u64);
    Ok(paddr)
}

pub fn check_cache_address(cache_va: *mut c_void, len: u64) -> Result<(), String> {
    let cache_va = cache_va as *mut c_uchar;
    // write_bytes
    for i in 0..len {
        unsafe {
            *cache_va.add(i as usize) = (i % 128) as u8;
        }
    }
    // read and check bytes
    for i in 0..len {
        unsafe {
            if *cache_va.add(i as usize) != (i % 128) as u8 {
                return Err(format!("check_cache_address: Mismatch at {} offset", i));
            }
        }
    }
    Ok(())
}

pub fn string_to_u64(s: String) -> Result<u64, String> {
    let s = s.trim().to_string();
    if s.starts_with("0x") {
        match u64::from_str_radix(&s[2..], 16) {
            Ok(num) => Ok(num),
            Err(_) => Err(format!("Not hex string: {}", s)),
        }
    } else if s.starts_with("0b") {
        match u64::from_str_radix(&s[2..], 2) {
            Ok(num) => Ok(num),
            Err(_) => Err(format!("Not binary string: {}", s)),
        }
    } else {
        // must be decimal
        match u64::from_str_radix(&s, 10) {
            Ok(num) => Ok(num),
            Err(_) => Err(format!("String {} is not in hex/bin/decimal format!", s)),
        }
    }
}
