// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use alloc::sync::Arc;
use alloc::vec::Vec;

use spin::Mutex;

use crate::arch::ArchPageTableEntryTrait;
use crate::arch::WORD_SIZE;
use crate::kernel::mem_page_alloc;
use crate::utils::round_up;
use crate::mm::PageFrame;

use super::{PAGE_SIZE, PTE_PER_PAGE};

// page_table const
pub const LVL0_SHIFT: usize = 39;
pub const LVL1_SHIFT: usize = 30;
pub const LVL2_SHIFT: usize = 21;
pub const LVL3_SHIFT: usize = 12;

pub const PTE_TABLE: usize = 0b11;
pub const PTE_PAGE: usize = 0b11;
pub const PTE_BLOCK: usize = 0b01;

pub const PTE_S1_FIELD_AP_RW_EL0_NONE: usize = 0b00 << 6;
pub const PTE_S1_FIELD_AP_RW_EL0_RW: usize = 0b01 << 6;
pub const PTE_S1_FIELD_AP_R0_EL0_NONE: usize = 0b10 << 6;
pub const PTE_S1_FIELD_AP_R0_EL0_RW: usize = 0b11 << 6;

pub const PTE_S1_FIELD_SH_NON_SHAREABLE: usize = 0b00 << 8;
pub const PTE_S1_FIELD_SH_RESERVED: usize = 0b01 << 8;
pub const PTE_S1_FIELD_SH_OUTER_SHAREABLE: usize = 0b10 << 8;
pub const PTE_S1_FIELD_SH_INNER_SHAREABLE: usize = 0b11 << 8;

pub const PTE_S1_FIELD_AF: usize = 1 << 10;

pub const PTE_S2_FIELD_MEM_ATTR_DEVICE_NGNRNE: usize = 0;

pub const PTE_S2_FIELD_MEM_ATTR_NORMAL_OUTER_WRITE_BACK_CACHEABLE: usize = 0b11 << 4;
pub const PTE_S2_FIELD_MEM_ATTR_NORMAL_OUTER_WRITE_BACK_NOCACHEABLE: usize = 0b1 << 4;

pub const PTE_S2_FIELD_MEM_ATTR_NORMAL_INNER_WRITE_BACK_CACHEABLE: usize = 0b11 << 2;

pub const PTE_S2_FIELD_AP_NONE: usize = 0b00 << 6;
pub const PTE_S2_FIELD_AP_RO: usize = 0b01 << 6;
pub const PTE_S2_FIELD_AP_WO: usize = 0b10 << 6;
pub const PTE_S2_FIELD_AP_RW: usize = 0b11 << 6;

pub const PTE_S2_FIELD_SH_NON_SHAREABLE: usize = 0b00 << 8;
pub const PTE_S2_FIELD_SH_RESERVED: usize = 0b01 << 8;
pub const PTE_S2_FIELD_SH_OUTER_SHAREABLE: usize = 0b10 << 8;
pub const PTE_S2_FIELD_SH_INNER_SHAREABLE: usize = 0b11 << 8;

pub const PTE_S2_FIELD_AF: usize = 1 << 10;

pub const PTE_S1_NORMAL: usize =
    pte_s1_field_attr_indx(1) | PTE_S1_FIELD_AP_RW_EL0_NONE | PTE_S1_FIELD_SH_OUTER_SHAREABLE | PTE_S1_FIELD_AF;

pub const PTE_S2_DEVICE: usize =
    PTE_S2_FIELD_MEM_ATTR_DEVICE_NGNRNE | PTE_S2_FIELD_AP_RW | PTE_S2_FIELD_SH_OUTER_SHAREABLE | PTE_S2_FIELD_AF;

pub const PTE_S2_NORMAL: usize = PTE_S2_FIELD_MEM_ATTR_NORMAL_INNER_WRITE_BACK_CACHEABLE
    | PTE_S2_FIELD_MEM_ATTR_NORMAL_OUTER_WRITE_BACK_CACHEABLE
    | PTE_S2_FIELD_AP_RW
    | PTE_S2_FIELD_SH_OUTER_SHAREABLE
    | PTE_S2_FIELD_AF;

pub const PTE_S2_NORMALNOCACHE: usize = PTE_S2_FIELD_MEM_ATTR_NORMAL_INNER_WRITE_BACK_CACHEABLE
    | PTE_S2_FIELD_MEM_ATTR_NORMAL_OUTER_WRITE_BACK_NOCACHEABLE
    | PTE_S2_FIELD_AP_RW
    | PTE_S2_FIELD_SH_OUTER_SHAREABLE
    | PTE_S2_FIELD_AF;

pub const PTE_S2_RO: usize = PTE_S2_FIELD_MEM_ATTR_NORMAL_INNER_WRITE_BACK_CACHEABLE
    | PTE_S2_FIELD_MEM_ATTR_NORMAL_OUTER_WRITE_BACK_CACHEABLE
    | PTE_S2_FIELD_AP_RO
    | PTE_S2_FIELD_SH_OUTER_SHAREABLE
    | PTE_S2_FIELD_AF;

pub const fn pte_s1_field_attr_indx(idx: usize) -> usize {
    idx << 2
}

// page_table function
pub fn pt_lvl0_idx(va: usize) -> usize {
    (va >> LVL0_SHIFT) & (PTE_PER_PAGE - 1)
}

pub fn pt_lvl1_idx(va: usize) -> usize {
    (va >> LVL1_SHIFT) & (PTE_PER_PAGE - 1)
}

pub fn pt_lvl2_idx(va: usize) -> usize {
    (va >> LVL2_SHIFT) & (PTE_PER_PAGE - 1)
}

pub fn pt_lvl3_idx(va: usize) -> usize {
    (va >> LVL3_SHIFT) & (PTE_PER_PAGE - 1)
}

#[repr(transparent)]
#[derive(Copy, Clone, Debug)]
/// Aarch64PageTableEntry struct represents a page table entry.
pub struct Aarch64PageTableEntry(usize);

impl ArchPageTableEntryTrait for Aarch64PageTableEntry {
    fn from_pte(value: usize) -> Self {
        Aarch64PageTableEntry(value)
    }

    fn from_pa(pa: usize) -> Self {
        Aarch64PageTableEntry(pa)
    }

    fn to_pte(&self) -> usize {
        self.0
    }

    fn to_pa(&self) -> usize {
        self.0 & 0x0000_FFFF_FFFF_F000
    }

    fn valid(&self) -> bool {
        self.0 & 0b11 != 0
    }

    fn entry(&self, index: usize) -> Aarch64PageTableEntry {
        let addr = self.to_pa() + index * WORD_SIZE;
        // SAFETY: The read of any address is safe in EL2
        unsafe { Aarch64PageTableEntry((addr as *const usize).read_volatile()) }
    }

    /// # Safety：
    /// 1. The index can't be out of [0, PAGE_SIZE / WORD_SIZE)
    /// 2. The page table entry have write permission
    unsafe fn set_entry(&self, index: usize, value: Aarch64PageTableEntry) {
        let addr = self.to_pa() + index * WORD_SIZE;
        unsafe { (addr as *mut usize).write_volatile(value.0) }
    }

    fn make_table(frame_pa: usize) -> Self {
        Aarch64PageTableEntry::from_pa(frame_pa | PTE_TABLE)
    }
}

#[derive(Clone)]
/// PageTable struct represents a page table, consisting of a directory and a list of pages.
pub struct PageTable {
    pub directory: Arc<PageFrame>,
    pub pages: Arc<Mutex<Vec<PageFrame>>>,
}

impl PageTable {
    /// Create a new page table with given directory.
    pub fn new(directory: PageFrame) -> PageTable {
        PageTable {
            directory: Arc::new(directory),
            pages: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn base_pa(&self) -> usize {
        self.directory.pa()
    }

    /// modify a range of ipa's access permission
    pub fn access_permission(&self, start_ipa: usize, len: usize, ap: usize) -> (usize, usize) {
        let directory = Aarch64PageTableEntry::from_pa(self.directory.pa());
        let mut ipa = start_ipa;
        let mut size = 0;
        let mut pa = 0;
        while ipa < (start_ipa + len) {
            let l1e = if cfg!(feature = "lvl4") {
                let l0e = directory.entry(pt_lvl0_idx(ipa));
                if !l0e.valid() {
                    ipa += 512 * 512 * 512 * 4096; // 512GB
                    continue;
                }
                l0e.entry(pt_lvl1_idx(ipa))
            } else {
                directory.entry(pt_lvl1_idx(ipa))
            };
            if !l1e.valid() {
                ipa += 512 * 512 * 4096; // 1GB: 9 + 9 + 12 bits
                continue;
            }
            let l2e = l1e.entry(pt_lvl2_idx(ipa));
            if !l2e.valid() {
                ipa += 512 * 4096; // 2MB: 9 + 12 bits
                continue;
            } else if l2e.to_pte() & 0b11 == PTE_BLOCK {
                let pte = l2e.to_pte() & !(0b11 << 6) | ap;
                debug!("access_permission set 512 page ipa {:x}", ipa);
                // SAFETY:
                // We set the page table entry to read-write
                // And the idx will "& (PTE_PER_PAGE - 1)" so it will not out of range
                unsafe {
                    l1e.set_entry(pt_lvl2_idx(ipa), Aarch64PageTableEntry::from_pa(pte));
                }
                ipa += 512 * 4096; // 2MB: 9 + 12 bits
                pa = l2e.to_pa();
                size += 512 * 4096;
                continue;
            }
            let l3e = l2e.entry(pt_lvl3_idx(ipa));
            if l3e.valid() {
                let pte = l3e.to_pte() & !(0b11 << 6) | ap;
                // SAFETY:
                // We set the page table entry to read-write
                // And the idx will "& (PTE_PER_PAGE - 1)" so it will not out of range
                unsafe {
                    l2e.set_entry(pt_lvl3_idx(ipa), Aarch64PageTableEntry::from_pa(pte));
                }
                pa = l3e.to_pa();
                size += 4096;
            }
            ipa += 4096; // 4KB: 12 bits
        }
        (pa, size)
    }

    /// map a 2mb page of ipa to a physical address
    pub fn map_2mb(&self, ipa: usize, pa: usize, pte: usize) {
        let directory = Aarch64PageTableEntry::from_pa(self.directory.pa());
        let l0e = if cfg!(feature = "lvl4") {
            let mut l0e = directory.entry(pt_lvl0_idx(ipa));
            if !l0e.valid() {
                if let Ok(frame) = mem_page_alloc() {
                    l0e = Aarch64PageTableEntry::make_table(frame.pa());
                    let mut pages = self.pages.lock();
                    pages.push(frame);
                    // SAFETY:
                    // We set the page table entry to read-write
                    // And the idx will "& (PTE_PER_PAGE - 1)" so it will not out of range
                    unsafe {
                        directory.set_entry(pt_lvl0_idx(ipa), l0e);
                    }
                } else {
                    error!("map lv0 page failed");
                    return;
                }
            }
            l0e
        } else {
            directory
        };
        let mut l1e = l0e.entry(pt_lvl1_idx(ipa));
        if !l1e.valid() {
            let result = mem_page_alloc();
            if let Ok(frame) = result {
                l1e = Aarch64PageTableEntry::make_table(frame.pa());
                let mut pages = self.pages.lock();
                pages.push(frame);
                if cfg!(feature = "lvl4") {
                    // SAFETY:
                    // We set the page table entry to read-write
                    // And the idx will "& (PTE_PER_PAGE - 1)" so it will not out of range
                    unsafe {
                        l0e.set_entry(pt_lvl1_idx(ipa), l1e);
                    }
                } else {
                    // SAFETY:
                    // We set the page table entry to read-write
                    // And the idx will "& (PTE_PER_PAGE - 1)" so it will not out of range
                    unsafe {
                        directory.set_entry(pt_lvl1_idx(ipa), l1e);
                    }
                }
            } else {
                error!("map lv1 page failed");
                return;
            }
        }

        let l2e = l1e.entry(pt_lvl2_idx(ipa));
        if l2e.valid() {
            debug!("map_2mb lvl 2 already mapped with 0x{:x}", l2e.to_pte());
        } else {
            // SAFETY:
            // We set the page table entry to read-write
            // And the idx will "& (PTE_PER_PAGE - 1)" so it will not out of range
            unsafe {
                l1e.set_entry(pt_lvl2_idx(ipa), Aarch64PageTableEntry::from_pa(pa | pte | PTE_BLOCK));
            }
        }
    }

    /// unmap a 2mb page of ipa
    pub fn unmap_2mb(&self, ipa: usize) {
        let directory = Aarch64PageTableEntry::from_pa(self.directory.pa());
        let l0e = if cfg!(feature = "lvl4") {
            let l0e = directory.entry(pt_lvl0_idx(ipa));
            if !l0e.valid() {
                return;
            }
            l0e
        } else {
            directory
        };
        let l1e = l0e.entry(pt_lvl1_idx(ipa));
        if l1e.valid() {
            let l2e = l1e.entry(pt_lvl2_idx(ipa));
            if l2e.valid() {
                // SAFETY:
                // We set the page table entry to read-write
                // And the idx will "& (PTE_PER_PAGE - 1)" so it will not out of range
                unsafe {
                    l1e.set_entry(pt_lvl2_idx(ipa), Aarch64PageTableEntry(0));
                }
                if empty_page(l1e.to_pa()) {
                    let l1e_pa = l1e.to_pa();
                    if cfg!(feature = "lvl4") {
                        // SAFETY:
                        // We set the page table entry to read-write
                        // And the idx will "& (PTE_PER_PAGE - 1)" so it will not out of range
                        unsafe {
                            l0e.set_entry(pt_lvl1_idx(ipa), Aarch64PageTableEntry(0));
                        }
                    } else {
                        // SAFETY:
                        // We set the page table entry to read-write
                        // And the idx will "& (PTE_PER_PAGE - 1)" so it will not out of range
                        unsafe {
                            directory.set_entry(pt_lvl1_idx(ipa), Aarch64PageTableEntry(0));
                        }
                    }
                    let mut pages = self.pages.lock();
                    pages.retain(|pf| pf.pa() != l1e_pa);
                }
            }
        }
    }

    /// map a 2 4kb page of ipa to a physical address
    pub fn map(&self, ipa: usize, pa: usize, pte: usize) {
        let directory = Aarch64PageTableEntry::from_pa(self.directory.pa());
        let l0e = if cfg!(feature = "lvl4") {
            let mut l0e = directory.entry(pt_lvl0_idx(ipa));
            if !l0e.valid() {
                if let Ok(frame) = mem_page_alloc() {
                    l0e = Aarch64PageTableEntry::make_table(frame.pa());
                    let mut pages = self.pages.lock();
                    pages.push(frame);
                    // SAFETY:
                    // We set the page table entry to read-write
                    // And the idx will "& (PTE_PER_PAGE - 1)" so it will not out of range
                    unsafe {
                        directory.set_entry(pt_lvl0_idx(ipa), l0e);
                    }
                } else {
                    error!("map lv0 page failed");
                    return;
                }
            }
            l0e
        } else {
            directory
        };
        let mut l1e = l0e.entry(pt_lvl1_idx(ipa));
        if !l1e.valid() {
            let result = mem_page_alloc();
            if let Ok(frame) = result {
                l1e = Aarch64PageTableEntry::make_table(frame.pa());
                let mut pages = self.pages.lock();
                pages.push(frame);
                if cfg!(feature = "lvl4") {
                    // SAFETY:
                    // We set the page table entry to read-write
                    // And the idx will "& (PTE_PER_PAGE - 1)" so it will not out of range
                    unsafe {
                        l0e.set_entry(pt_lvl1_idx(ipa), l1e);
                    }
                } else {
                    // SAFETY:
                    // We set the page table entry to read-write
                    // And the idx will "& (PTE_PER_PAGE - 1)" so it will not out of range
                    unsafe {
                        directory.set_entry(pt_lvl1_idx(ipa), l1e);
                    }
                }
            } else {
                error!("map lv1 page failed");
                return;
            }
        }

        let mut l2e = l1e.entry(pt_lvl2_idx(ipa));
        if !l2e.valid() {
            let result = mem_page_alloc();
            if let Ok(frame) = result {
                l2e = Aarch64PageTableEntry::make_table(frame.pa());
                let mut pages = self.pages.lock();
                pages.push(frame);
                // SAFETY:
                // We set the page table entry to read-write
                // And the idx will "& (PTE_PER_PAGE - 1)" so it will not out of range
                unsafe {
                    l1e.set_entry(pt_lvl2_idx(ipa), l2e);
                }
            } else {
                error!("map lv2 page failed {:#?}", result.err());
                return;
            }
        } else if l2e.to_pte() & 0b11 == PTE_BLOCK {
            debug!("map lvl 2 already mapped with 2mb 0x{:x}", l2e.to_pte());
        }
        let l3e = l2e.entry(pt_lvl3_idx(ipa));
        if l3e.valid() {
            debug!("map lvl 3 already mapped with 0x{:x}", l3e.to_pte());
        } else {
            // SAFETY:
            // We set the page table entry to read-write
            // And the idx will "& (PTE_PER_PAGE - 1)" so it will not out of range
            unsafe {
                l2e.set_entry(pt_lvl3_idx(ipa), Aarch64PageTableEntry::from_pa(pa | PTE_TABLE | pte));
            }
        }
    }

    /// unmap a 4kb page of ipa
    pub fn unmap(&self, ipa: usize) {
        let directory = Aarch64PageTableEntry::from_pa(self.directory.pa());
        let l0e = if cfg!(feature = "lvl4") {
            let l0e = directory.entry(pt_lvl0_idx(ipa));
            if !l0e.valid() {
                return;
            }
            l0e
        } else {
            directory
        };
        let l1e = l0e.entry(pt_lvl1_idx(ipa));
        if l1e.valid() {
            let l2e = l1e.entry(pt_lvl2_idx(ipa));
            if l2e.valid() {
                let l3e = l2e.entry(pt_lvl3_idx(ipa));
                if l3e.valid() {
                    // SAFETY:
                    // We set the page table entry to read-write
                    // And the idx will "& (PTE_PER_PAGE - 1)" so it will not out of range
                    unsafe {
                        l2e.set_entry(pt_lvl3_idx(ipa), Aarch64PageTableEntry::from_pa(0));
                    }
                    // check l2e
                    if empty_page(l2e.to_pa()) {
                        let l2e_pa = l2e.to_pa();
                        // SAFETY:
                        // We set the page table entry to read-write
                        // And the idx will "& (PTE_PER_PAGE - 1)" so it will not out of range
                        unsafe {
                            l1e.set_entry(pt_lvl2_idx(ipa), Aarch64PageTableEntry(0));
                        }
                        let mut pages = self.pages.lock();
                        pages.retain(|pf| pf.pa != l2e_pa);
                        // check l1e
                        if empty_page(l1e.to_pa()) {
                            let l1e_pa = l1e.to_pa();
                            if cfg!(feature = "lvl4") {
                                // SAFETY:
                                // We set the page table entry to read-write
                                // And the idx will "& (PTE_PER_PAGE - 1)" so it will not out of range
                                unsafe {
                                    l0e.set_entry(pt_lvl1_idx(ipa), Aarch64PageTableEntry(0));
                                }
                            } else {
                                // SAFETY:
                                // We set the page table entry to read-write
                                // And the idx will "& (PTE_PER_PAGE - 1)" so it will not out of range
                                unsafe {
                                    directory.set_entry(pt_lvl1_idx(ipa), Aarch64PageTableEntry(0));
                                }
                            }
                            pages.retain(|pf| pf.pa != l1e_pa);
                        }
                    }
                }
            }
        }
    }

    /// map a range of ipa to a range of physical space, which page size is 2mb
    pub fn map_range_2mb(&self, ipa: usize, len: usize, pa: usize, pte: usize) {
        let size_2mb = 1 << LVL2_SHIFT;
        let page_num = round_up(len, size_2mb) / size_2mb;

        for i in 0..page_num {
            self.map_2mb(ipa + i * size_2mb, pa + i * size_2mb, pte);
        }
    }

    /// unmap a range of ipa, which page size is 2mb
    pub fn unmap_range_2mb(&self, ipa: usize, len: usize) {
        let size_2mb = 1 << LVL2_SHIFT;
        let page_num = round_up(len, size_2mb) / size_2mb;

        for i in 0..page_num {
            self.unmap_2mb(ipa + i * size_2mb);
        }
    }

    /// map a range of ipa to a range of physical space, which page size is 4kb
    pub fn map_range(&self, ipa: usize, len: usize, pa: usize, pte: usize) {
        let page_num = round_up(len, PAGE_SIZE) / PAGE_SIZE;
        for i in 0..page_num {
            self.map(ipa + i * PAGE_SIZE, pa + i * PAGE_SIZE, pte);
        }
    }

    /// unmap a range of ipa, which page size is 4kb
    pub fn unmap_range(&self, ipa: usize, len: usize) {
        let page_num = round_up(len, PAGE_SIZE) / PAGE_SIZE;
        for i in 0..page_num {
            self.unmap(ipa + i * PAGE_SIZE);
        }
    }

    /// display page table to debug
    pub fn show_pt(&self, ipa: usize) {
        // println!("show_pt");
        let directory = Aarch64PageTableEntry::from_pa(self.directory.pa());
        debug!("root {:x}", directory.to_pte());
        let l1e = if cfg!(feature = "lvl4") {
            let l0e = directory.entry(pt_lvl0_idx(ipa));
            debug!("l0e {:x}", l0e.to_pte());
            l0e.entry(pt_lvl1_idx(ipa))
        } else {
            directory.entry(pt_lvl1_idx(ipa))
        };
        debug!("l1e {:x}", l1e.to_pte());
        let l2e = l1e.entry(pt_lvl2_idx(ipa));
        debug!("l2e {:x}", l2e.to_pte());
        if !l2e.valid() {
            error!("invalid ipa {:x} to l2 pte {:x}", ipa, l2e.to_pte());
        } else if l2e.to_pte() & 0b11 == PTE_BLOCK {
            debug!("l2 ipa {:x} to pa {:x}", ipa, l2e.to_pte());
        } else {
            let l3e = l2e.entry(pt_lvl3_idx(ipa));
            debug!("l3 ipa {:x} to pa {:x}", ipa, l3e.to_pte());
        }
    }

    /// map a range of ipa to a range of physical space, using 4kb or 2mb page depending on the alignment of ipa, len and pa
    pub fn pt_map_range(&self, ipa: usize, len: usize, pa: usize, pte: usize, map_block: bool) {
        let size_2mb = 1 << LVL2_SHIFT;
        if ipa % size_2mb == 0 && len % size_2mb == 0 && pa % size_2mb == 0 && map_block {
            self.map_range_2mb(ipa, len, pa, pte);
        } else {
            self.map_range(ipa, len, pa, pte);
        }
    }

    /// unmap a range of ipa, using 4kb or 2mb page depending on the alignment of ipa and len
    pub fn pt_unmap_range(&self, ipa: usize, len: usize, map_block: bool) {
        let size_2mb = 1 << LVL2_SHIFT;
        if ipa % size_2mb == 0 && len % size_2mb == 0 && map_block {
            self.unmap_range_2mb(ipa, len);
        } else {
            self.unmap_range(ipa, len);
        }
    }
}

/// check if a page is empty
pub fn empty_page(addr: usize) -> bool {
    for i in 0..(PAGE_SIZE / 8) {
        // SAFETY:
        // the read of any address is safe in EL2
        if unsafe { ((addr + i * 8) as *const usize).read_volatile() != 0 } {
            return false;
        }
    }
    true
}
