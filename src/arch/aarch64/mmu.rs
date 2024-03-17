// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use tock_registers::*;
use tock_registers::interfaces::*;

use crate::utils::{memset, bit_extract};
use crate::arch::{at, isb};

use super::interface::*;

register_bitfields! {u64,
    pub TableDescriptor [
        NEXT_LEVEL_TABLE_PPN OFFSET(12) NUMBITS(36) [], // [47:12]
        TYPE  OFFSET(1) NUMBITS(1) [
            Block = 0,
            Table = 1
        ],
        VALID OFFSET(0) NUMBITS(1) [
            False = 0,
            True = 1
        ]
    ]
}

register_bitfields! {u64,
    pub PageDescriptorS1 [
        UXN      OFFSET(54) NUMBITS(1) [
            False = 0,
            True = 1
        ],
        PXN      OFFSET(53) NUMBITS(1) [
            False = 0,
            True = 1
        ],
        OUTPUT_PPN OFFSET(12) NUMBITS(36) [], // [47:12]
        AF       OFFSET(10) NUMBITS(1) [
            False = 0,
            True = 1
        ],
        SH       OFFSET(8) NUMBITS(2) [
            OuterShareable = 0b10,
            InnerShareable = 0b11
        ],
        AP       OFFSET(6) NUMBITS(2) [
            RW_ELx = 0b00,
            RW_ELx_EL0 = 0b01,
            RO_ELx = 0b10,
            RO_ELx_EL0 = 0b11
        ],
        AttrIndx OFFSET(2) NUMBITS(3) [
            Attr0 = 0b000,
            Attr1 = 0b001,
            Attr2 = 0b010
        ],
        TYPE     OFFSET(1) NUMBITS(1) [
            Block = 0,
            Table = 1
        ],
        VALID    OFFSET(0) NUMBITS(1) [
            False = 0,
            True = 1
        ]
    ]
}

enum MemoryType {
    Normal,
    Device,
}

#[derive(Copy, Clone)]
#[repr(transparent)]
struct BlockDescriptor(u64);

impl BlockDescriptor {
    fn new(output_addr: usize, mem_type: MemoryType) -> BlockDescriptor {
        BlockDescriptor(
            (PageDescriptorS1::OUTPUT_PPN.val((output_addr >> PAGE_SHIFT) as u64)
                + PageDescriptorS1::AF::True
                + PageDescriptorS1::AP::RW_ELx
                + PageDescriptorS1::TYPE::Block
                + PageDescriptorS1::VALID::True
                + match mem_type {
                    MemoryType::Device => PageDescriptorS1::AttrIndx::Attr0 + PageDescriptorS1::SH::OuterShareable,
                    MemoryType::Normal => PageDescriptorS1::AttrIndx::Attr1 + PageDescriptorS1::SH::InnerShareable,
                })
            .value,
        )
    }

    fn table(output_addr: usize) -> BlockDescriptor {
        BlockDescriptor(
            (PageDescriptorS1::OUTPUT_PPN.val((output_addr >> PAGE_SHIFT) as u64)
                + PageDescriptorS1::VALID::True
                + PageDescriptorS1::TYPE::Table)
                .value,
        )
    }
    const fn invalid() -> BlockDescriptor {
        BlockDescriptor(0)
    }
}

#[repr(C)]
#[repr(align(4096))]
/// PageTable struct
pub struct PageTables {
    entry: [BlockDescriptor; ENTRY_PER_PAGE],
}

/// level1 page table
pub static LVL1_PAGE_TABLE: PageTables = PageTables {
    entry: [BlockDescriptor(0); ENTRY_PER_PAGE],
};

/// level2 page table
pub static LVL2_PAGE_TABLE: PageTables = PageTables {
    entry: [BlockDescriptor(0); ENTRY_PER_PAGE],
};

const PLATFORM_PHYSICAL_LIMIT_GB: usize = 16;

#[no_mangle]
// #[link_section = ".text.boot"]
/// populate page table
pub extern "C" fn pt_populate(lvl1_pt: &mut PageTables, lvl2_pt: &mut PageTables) {
    let lvl1_base: usize = lvl1_pt as *const _ as usize;
    let lvl2_base = lvl2_pt as *const _ as usize;
    // SAFETY:
    // The lvl1_pt and lvl2_pt are writable for the Hypervisor in EL2.
    // c is a valid value of type u8 without overflow.
    unsafe {
        memset(lvl1_base as *mut u8, 0, PAGE_SIZE);
        memset(lvl2_base as *mut u8, 0, PAGE_SIZE);
    }

    #[cfg(feature = "tx2")]
    {
        use crate::arch::pt_lvl2_idx;
        use crate::board::PLAT_DESC;
        for i in 0..PLATFORM_PHYSICAL_LIMIT_GB {
            use crate::arch::LVL1_SHIFT;
            let output_addr = i << LVL1_SHIFT;
            lvl1_pt.entry[i] = if output_addr >= PLAT_DESC.mem_desc.base {
                BlockDescriptor::new(output_addr, MemoryType::Normal)
            } else {
                BlockDescriptor::invalid()
            }
        }

        lvl1_pt.entry[0] = BlockDescriptor::table(lvl2_base);
        // 0x200000 ~ 2MB
        // UART0 ~ 0x3000000 - 0x3200000 (0x3100000)
        // UART1 ~ 0xc200000 - 0xc400000 (0xc280000)
        // EMMC ~ 0x3400000 - 0x3600000 (0x3460000)
        // GIC  ~ 0x3800000 - 0x3a00000 (0x3881000)
        // SMMU ~ 0x12000000 - 0x13000000
        lvl2_pt.entry[pt_lvl2_idx(0x3000000)] = BlockDescriptor::new(0x3000000, MemoryType::Device);
        lvl2_pt.entry[pt_lvl2_idx(0xc200000)] = BlockDescriptor::new(0xc200000, MemoryType::Device);
        // lvl2_pt.entry[pt_lvl2_idx(0x3400000)] = BlockDescriptor::new(0x3400000, MemoryType::Device);
        lvl2_pt.entry[pt_lvl2_idx(0x3800000)] = BlockDescriptor::new(0x3800000, MemoryType::Device);
        for i in 0..(0x100_0000 / 0x200000) {
            let addr = 0x12000000 + i * 0x200000;
            lvl2_pt.entry[pt_lvl2_idx(addr)] = BlockDescriptor::new(addr, MemoryType::Device);
        }
    }
    #[cfg(feature = "pi4")]
    {
        use crate::arch::LVL2_SHIFT;
        // crate::driver::putc('o' as u8);
        // crate::driver::putc('r' as u8);
        // crate::driver::putc('e' as u8);
        // println!("pt");
        // 0x0_0000_0000 ~ 0x0_c000_0000 --> normal memory (3GB)
        lvl1_pt.entry[0] = BlockDescriptor::new(0, MemoryType::Normal);
        lvl1_pt.entry[1] = BlockDescriptor::new(0x40000000, MemoryType::Normal);
        lvl1_pt.entry[2] = BlockDescriptor::new(0x80000000, MemoryType::Normal);
        lvl1_pt.entry[3] = BlockDescriptor::table(lvl2_base);
        // 0x0_c000_0000 ~ 0x0_fc00_0000 --> normal memory (960MB)
        for i in 0..480 {
            lvl2_pt.entry[i] = BlockDescriptor::new(0x0c0000000 + (i << LVL2_SHIFT), MemoryType::Normal);
        }
        // 0x0_fc00_0000 ~ 0x1_0000_0000 --> device memory (64MB)
        for i in 480..512 {
            lvl2_pt.entry[i] = BlockDescriptor::new(0x0c0000000 + (i << LVL2_SHIFT), MemoryType::Device);
        }
        // 0x1_0000_0000 ~ 0x2_0000_0000 --> normal memory (4GB)
        lvl1_pt.entry[4] = BlockDescriptor::new(0x100000000, MemoryType::Normal);
        lvl1_pt.entry[5] = BlockDescriptor::new(0x140000000, MemoryType::Normal);
        lvl1_pt.entry[6] = BlockDescriptor::new(0x180000000, MemoryType::Normal);
        lvl1_pt.entry[7] = BlockDescriptor::new(0x1c0000000, MemoryType::Normal);
        for i in 8..512 {
            lvl1_pt.entry[i] = BlockDescriptor::invalid();
        }
    }
    #[cfg(feature = "qemu")]
    {
        use crate::arch::LVL2_SHIFT;
        use crate::board::PLAT_DESC;
        for index in 0..PLATFORM_PHYSICAL_LIMIT_GB {
            use crate::arch::LVL1_SHIFT;
            let pa = index << LVL1_SHIFT;
            if pa < PLAT_DESC.mem_desc.base {
                lvl1_pt.entry[index] = BlockDescriptor::new(pa, MemoryType::Device);
            } else {
                lvl1_pt.entry[index] = BlockDescriptor::new(pa, MemoryType::Normal);
            }
        }
        lvl1_pt.entry[32] = BlockDescriptor::table(lvl2_base);
        for (index, pa) in (0..PLAT_DESC.mem_desc.base).step_by(1 << LVL2_SHIFT).enumerate() {
            if index >= 512 {
                break;
            }
            lvl2_pt.entry[index] = BlockDescriptor::new(pa, MemoryType::Device);
        }
    }
    #[cfg(feature = "rk3588")]
    {
        use crate::arch::LVL2_SHIFT;
        // 0x0020_0000 ~ 0xc000_0000 --> normal memory (3GB)
        lvl1_pt.entry[0] = BlockDescriptor::new(0, MemoryType::Normal);
        lvl1_pt.entry[1] = BlockDescriptor::new(0x40000000, MemoryType::Normal);
        lvl1_pt.entry[2] = BlockDescriptor::new(0x80000000, MemoryType::Normal);
        lvl1_pt.entry[3] = BlockDescriptor::table(lvl2_base);
        // 0xc000_0000 ~ 0xf000_0000 --> normal memory (768MB)
        const DEVICE_BOUND: usize = (0xf000_0000 - 0xc000_0000) / (1 << LVL2_SHIFT);
        for i in 0..DEVICE_BOUND {
            lvl2_pt.entry[i] = BlockDescriptor::new(0x0c0000000 + (i << LVL2_SHIFT), MemoryType::Normal);
        }
        // 0x0_f000_0000 ~ 0x1_0000_0000 --> device memory (256MB)
        for i in DEVICE_BOUND..512 {
            lvl2_pt.entry[i] = BlockDescriptor::new(0x0c0000000 + (i << LVL2_SHIFT), MemoryType::Device);
        }
        // 0x1_0000_0000 ~ 0x2_0000_0000 --> normal memory (4GB)
        lvl1_pt.entry[4] = BlockDescriptor::new(0x100000000, MemoryType::Normal);
        lvl1_pt.entry[5] = BlockDescriptor::new(0x140000000, MemoryType::Normal);
        lvl1_pt.entry[6] = BlockDescriptor::new(0x180000000, MemoryType::Normal);
        lvl1_pt.entry[7] = BlockDescriptor::new(0x1c0000000, MemoryType::Normal);
        for i in 8..512 {
            lvl1_pt.entry[i] = BlockDescriptor::invalid();
        }
        // 0x200000 ~ 2MB
        // UART0 ~ 0xfea00000 - 0xfec00000 (0xfeb50000)
        // UART1 ~ 0xfea00000 - 0xfec00000 (0xfebc0000)
        // EMMC ~ 0xfe200000 - 0xfe400000 (0xfe2e0000)
        // GIC  ~ 0xfe600000 - 0xfe800000 (0xfe600000)
        // SMMU1 ~ 0xfc800000 - 0xfce00000 (0xfc900000  size:0x200000;0xfcb00000  size:0x200000)
    }
}

#[no_mangle]
// #[link_section = ".text.boot"]
/// init mmu, set mmu-related registers
pub extern "C" fn mmu_init(pt: &PageTables) {
    use cortex_a::registers::*;
    MAIR_EL2.write(
        MAIR_EL2::Attr0_Device::nonGathering_nonReordering_noEarlyWriteAck
            + MAIR_EL2::Attr1_Normal_Outer::WriteBack_NonTransient_ReadWriteAlloc
            + MAIR_EL2::Attr1_Normal_Inner::WriteBack_NonTransient_ReadWriteAlloc
            + MAIR_EL2::Attr2_Normal_Outer::NonCacheable
            + MAIR_EL2::Attr2_Normal_Inner::NonCacheable,
    );
    TTBR0_EL2.set(&pt.entry as *const _ as u64);

    TCR_EL2.write(
        TCR_EL2::PS::Bits_48
            + TCR_EL2::SH0::Inner
            + TCR_EL2::TG0::KiB_4
            + TCR_EL2::ORGN0::WriteBack_ReadAlloc_WriteAlloc_Cacheable
            + TCR_EL2::IRGN0::WriteBack_ReadAlloc_WriteAlloc_Cacheable
            + TCR_EL2::T0SZ.val(64 - 39),
    );
}

const PAR_EL1_OFF: usize = 12;
const PAR_EL1_LEN: usize = 36;

/// translate gva to ipa
pub fn gva2ipa(gva: usize) -> Result<usize, ()> {
    use cortex_a::registers::PAR_EL1;

    let par = PAR_EL1.get();
    at::s1e1r(gva);
    isb();
    let tmp = PAR_EL1.get();
    PAR_EL1.set(par);

    if (tmp & PAR_EL1::F::TranslationAborted.value) != 0 {
        Err(())
    } else {
        let par_pa = bit_extract(tmp as usize, PAR_EL1_OFF, PAR_EL1_LEN);
        let pa = (par_pa << PAR_EL1_OFF) | (gva & (PAGE_SIZE - 1));
        Ok(pa)
    }
}
