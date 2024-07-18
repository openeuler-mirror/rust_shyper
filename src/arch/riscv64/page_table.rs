use super::interface::PAGE_SIZE;
use crate::arch::ArchPageTableEntryTrait;
use crate::mm::PageFrame;
use super::interface::WORD_SIZE;
use super::interface::PTE_PER_PAGE;
use crate::kernel::mem_page_alloc;
use crate::utils::round_up;
use alloc::sync::Arc;
use spin::Mutex;
use alloc::vec::Vec;

// TODO:
// Consider using Svnapot to use larger page entries, merging consecutive page entries
// Svnapot: supports 64K large pages

#[repr(transparent)]
#[derive(Copy, Clone, Debug)]
pub struct Riscv64PTEntry(usize);

// page_table const
pub const LVL0_SHIFT: usize = 30;
pub const LVL1_SHIFT: usize = 21;
pub const LVL2_SHIFT: usize = 12;

/// PageTable Entry bits
pub const PTE_V: usize = 1 << 0;
pub const PTE_R: usize = 1 << 1;
pub const PTE_W: usize = 1 << 2;
pub const PTE_X: usize = 1 << 3;
pub const PTE_U: usize = 1 << 4;
pub const PTE_G: usize = 1 << 5;
pub const PTE_A: usize = 1 << 6;
pub const PTE_D: usize = 1 << 7;
pub const PTE_RSW: usize = 0b11 << 8; // 2 bits

// Note: riscv doesn't support marking device memory
// Basic Stage-2 Page Table Entry
pub const PTE_S2_NORMAL: usize = PTE_V | PTE_R | PTE_W | PTE_X | PTE_U | PTE_A | PTE_D;
pub const PTE_S2_DEVICE: usize = PTE_V | PTE_R | PTE_W | PTE_X | PTE_U | PTE_A | PTE_D;
pub const PTE_S2_NORMALNOCACHE: usize = PTE_V | PTE_R | PTE_W | PTE_X | PTE_U | PTE_A | PTE_D;
pub const PTE_S2_RO: usize = PTE_V | PTE_R | PTE_U | PTE_A | PTE_D;
pub const PTE_S2_FIELD_AP_RO: usize = PTE_R;

/// page_table functions
/// L0 level pagetable's size is 16KB
pub fn pt_lvl0_idx(va: usize) -> usize {
    (va >> LVL0_SHIFT) & (PTE_PER_PAGE * 4 - 1)
}

pub fn pt_lvl1_idx(va: usize) -> usize {
    (va >> LVL1_SHIFT) & (PTE_PER_PAGE - 1)
}

pub fn pt_lvl2_idx(va: usize) -> usize {
    (va >> LVL2_SHIFT) & (PTE_PER_PAGE - 1)
}

/// Page Table Entry Implementation
/// Currently, We use Sv39x4 PageTable
/// Similar to supervisor mode page table entry
// Defines specifications for RISCV page entries: RISCV page entries are pa shifted 2 bits to the right, with perm fields occupying the lower 10 bits
impl ArchPageTableEntryTrait for Riscv64PTEntry {
    // Pass a page table entry，with permission bit
    fn from_pte(value: usize) -> Self {
        Riscv64PTEntry(value)
    }

    fn from_pa(pa: usize) -> Self {
        // 56 bit PA
        Riscv64PTEntry((pa & 0x003F_FFFF_FFFF_F000) >> 2)
    }

    fn to_pte(&self) -> usize {
        self.0
    }

    fn to_pa(&self) -> usize {
        (self.0 & 0xFFFF_FFFF_FFFF_FC00) << 2
    }

    fn valid(&self) -> bool {
        (self.0 & PTE_V) != 0
    }

    /// read an item in the page table
    fn entry(&self, index: usize) -> Self {
        let addr = self.to_pa() + index * WORD_SIZE;
        // SAFETY: 'addr' is a valid address of page table item
        unsafe { Riscv64PTEntry((addr as *const usize).read_volatile()) }
    }

    /// Write the entry in the page table.
    /// # Safety:
    /// 1. The 'index' must be a valid index, within the range of the page table, usually 0..512
    /// 2. The 'value' must be a valid page table entry, with legal permission bits
    unsafe fn set_entry(&self, index: usize, value: Self) {
        let addr = self.to_pa() + index * WORD_SIZE;
        (addr as *mut usize).write_volatile(value.to_pte());
    }

    // frame_pa is 4KB aligned
    fn make_table(frame_pa: usize) -> Self {
        Riscv64PTEntry::from_pte((frame_pa >> 2) | PTE_V)
    }
}

#[derive(Clone)]
pub struct PageTable {
    pub directory: Arc<PageFrame>,
    pub pages: Arc<Mutex<Vec<PageFrame>>>,
}

/// RISCV PageTable Features:
/// No PTE_BLOCK, all page are 4KB
/// All leaf PTE are in the 3-depth position
impl PageTable {
    pub fn new(directory: PageFrame) -> PageTable {
        PageTable {
            directory: Arc::new(directory),
            pages: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn base_pa(&self) -> usize {
        self.directory.pa()
    }

    /// TODO: change a range of va's access permission
    /// only change access_permission, not others(PTE_V, PTE_U, PTE_A, PTE_D)
    #[allow(unused_variables)]
    pub fn access_permission(&self, start_ipa: usize, len: usize, ap: usize) -> (usize, usize) {
        todo!()
    }

    /// map ipa to pa
    /// pa and ipa should be 4KB aligned
    /// pte should be pte entry bits
    pub fn map(&self, ipa: usize, pa: usize, pte: usize) {
        let directory = Riscv64PTEntry::from_pa(self.directory.pa());
        let mut l0e = directory.entry(pt_lvl0_idx(ipa));
        if !l0e.valid() {
            if let Ok(frame) = mem_page_alloc() {
                l0e = Riscv64PTEntry::make_table(frame.pa());
                // Set directory's entry
                // SAFETY: idx is an index within the permitted range, and l0e is a valid entry
                unsafe { directory.set_entry(pt_lvl0_idx(ipa), l0e) }
                let mut pages = self.pages.lock();
                pages.push(frame);
            } else {
                panic!("map lv0 page failed");
            }
        }

        let mut l1e = l0e.entry(pt_lvl1_idx(ipa));
        if !l1e.valid() {
            if let Ok(frame) = mem_page_alloc() {
                l1e = Riscv64PTEntry::make_table(frame.pa());
                // Note: Set the entry for the level 1 page table
                // SAFETY: idx is an index within the permitted range, and l1e is a valid entry
                unsafe { l0e.set_entry(pt_lvl1_idx(ipa), l1e) }
                let mut pages = self.pages.lock();
                pages.push(frame);
            } else {
                error!("map lv1 page failed");
                return;
            }
        }

        let l2e = l1e.entry(pt_lvl2_idx(ipa));
        if l2e.valid() {
            debug!("map lvl 2 already mapped with 0x{:x}", l2e.to_pte());
        } else {
            // SAFETY:
            // idx is an index within the permitted range, and value is a valid entry
            // (containing a valid PA and permission bits)
            unsafe { l1e.set_entry(pt_lvl2_idx(ipa), Riscv64PTEntry::from_pte((pa >> 2) | pte)) };
        }
    }

    pub fn unmap(&self, ipa: usize) {
        let directory = Riscv64PTEntry::from_pa(self.directory.pa());
        let l0e = directory.entry(pt_lvl0_idx(ipa));
        if !l0e.valid() {
            return;
        }

        let l1e = l0e.entry(pt_lvl1_idx(ipa));
        if !l1e.valid() {
            return;
        }

        let l2e = l1e.entry(pt_lvl2_idx(ipa));
        if !l2e.valid() {
            return;
        }

        // check and release l1 page table
        // SAFETY: idx is an index within the permitted range, and 0 is an allowed entry value
        unsafe { l1e.set_entry(pt_lvl1_idx(ipa), Riscv64PTEntry(0)) };
        if !empty_page(l1e.to_pa()) {
            return;
        }
        let l1e_pa = l1e.to_pa();
        let mut pages = self.pages.lock();
        pages.retain(|pf| pf.pa != l1e_pa); // remove l1 table pageframe from pages

        // Check and release l0 page table
        // SAFETY: idx is an index within the permitted range, and 0 is an allowed entry value
        unsafe { l0e.set_entry(pt_lvl0_idx(ipa), Riscv64PTEntry(0)) };
        if !empty_page(l0e.to_pa()) {
            return;
        }
        let l0e_pa = l0e.to_pa();
        pages.retain(|pf| pf.pa != l0e_pa);

        // SAFETY: idx is an index within the permitted range, and 0 is an allowed entry value
        unsafe { directory.set_entry(pt_lvl0_idx(ipa), Riscv64PTEntry(0)) };
    }

    pub fn map_range(&self, ipa: usize, len: usize, pa: usize, pte: usize) {
        let page_num = round_up(len, PAGE_SIZE) / PAGE_SIZE;
        trace!(
            "map range ipa {:x} len {:x}, to pa {:x} pte {:x}. page_num = {}",
            ipa,
            len,
            pa,
            pte,
            page_num
        );
        for i in 0..page_num {
            self.map(ipa + i * PAGE_SIZE, pa + i * PAGE_SIZE, pte);
            if i % 0x20000 == 0 {
                debug!("map ipa {:x} to pa {:x}", ipa + i * PAGE_SIZE, pa + i * PAGE_SIZE);
            }
        }
        trace!("finish map_range ipa {:x}!", ipa);
    }

    pub fn unmap_range(&self, ipa: usize, len: usize) {
        let page_num = round_up(len, PAGE_SIZE) / PAGE_SIZE;
        for i in 0..page_num {
            self.unmap(ipa + i * PAGE_SIZE);
        }
    }

    pub fn show_pt(&self, ipa: usize) {
        let directory = Riscv64PTEntry::from_pa(self.directory.pa());
        debug!("root {:x}", directory.to_pte());
        let l0e = directory.entry(pt_lvl0_idx(ipa));
        if !l0e.valid() {
            error!("invalid ipa {:x} to l0 pte {:x}", ipa, l0e.to_pte());
            return;
        } else {
            debug!("l0e {:x}", l0e.to_pte());
        }

        let l1e = l0e.entry(pt_lvl1_idx(ipa));
        if !l1e.valid() {
            error!("invalid ipa {:x} to l1 pte {:x}", ipa, l1e.to_pte());
            return;
        } else {
            debug!("l1e {:x}", l1e.to_pte());
        }

        let l2e = l1e.entry(pt_lvl2_idx(ipa));
        debug!("l1 ipa {:x} to pa {:x}", ipa, l2e.to_pte());
    }

    #[allow(unused_variables)]
    pub fn pt_map_range(&self, ipa: usize, len: usize, pa: usize, pte: usize, map_block: bool) {
        self.map_range(ipa, len, pa, pte)
    }

    #[allow(unused_variables)]
    pub fn pt_unmap_range(&self, ipa: usize, len: usize, map_block: bool) {
        self.unmap_range(ipa, len);
    }
}

/// TODO: suggest to delete and use reference counter to manage pages
/// judge whether a page is all 0
pub fn empty_page(addr: usize) -> bool {
    for i in 0..(PAGE_SIZE / 8) {
        // SAFETY:
        // 1. addr is a page aligned address
        // 2. The memory is readable
        if unsafe { ((addr + i * 8) as *const usize).read_volatile() != 0 } {
            return false;
        }
    }
    true
}
