// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

// The following is the structure of the flattened device tree (FDT)
// ---------------------------------
// |  Ftd_header                   |
// ---------------------------------
// |  (free space)                 |
// ---------------------------------
// |  Memory reservation block     |
// ---------------------------------
// |  (free space)                 |
// ---------------------------------
// |  Structure Block              |
// ---------------------------------
// |  (free space)                 |
// ---------------------------------
// |  Strings Block                |
// ---------------------------------
// |  (free space)                 |
// ---------------------------------

use core::cmp::Ordering;
use core::fmt::{Display, Error, Formatter};

// Flatteened Device Tree header offsets
mod offset {
    // Offset of the magic number
    pub const MAGIC: usize = 0x0;
    // Offset of total size of the device tree
    pub const TOTALSIZE: usize = 0x4;
    // Offset of the structure block
    pub const OFF_DT_STRUCT: usize = 0x8;
    // Offset of the strings block
    pub const OFF_DT_STRINGS: usize = 0xc;
    // Offset of the memory reservation block
    pub const OFF_MEM_RSVS: usize = 0x10;
    // Offset of the version
    pub const VERSION: usize = 0x14;
    // Offset of the last compatible version
    pub const LAST_COMP_VERSION: usize = 0x18;
    // Offset of the boot cpuid
    pub const BOOT_CPUID_PHYS: usize = 0x1c;
    // Offset of the size of the strings block
    pub const SIZE_DT_STRINGS: usize = 0x20;
    // Offset of the size of the structure block
    pub const SIZE_DT_STRUCT: usize = 0x24;
}

// Get the beginning of the u32 structure block
fn get_be32(buf: &[u8], offset: usize) -> Result<u32, ()> {
    let bytes = buf.get(offset..offset + 4).ok_or(())?;
    Ok(u32::from_be_bytes(bytes.try_into().unwrap()))
}

// Set the beginning of the u32 structure block
fn set_be32(buf: &mut [u8], offset: usize, val: u32) -> Result<(), ()> {
    let bytes = buf.get_mut(offset..offset + 4).ok_or(())?;

    bytes.copy_from_slice(&u32::to_be_bytes(val));
    Ok(())
}

// Get the beginning of the u64 structure block
fn get_be64(buf: &[u8], offset: usize) -> Result<u64, ()> {
    let bytes = buf.get(offset..offset + 8).ok_or(())?;
    Ok(u64::from_be_bytes(bytes.try_into().unwrap()))
}

// Set the beginning of the u64 structure block
fn set_be64(buf: &mut [u8], offset: usize, val: u64) -> Result<(), ()> {
    let bytes = buf.get_mut(offset..offset + 8).ok_or(())?;

    bytes.copy_from_slice(&u64::to_be_bytes(val));
    Ok(())
}

trait Section {
    // Offset of the section reference above
    const OFF_OFFSET: usize;

    // Get the size of the section
    fn get_size(dts: &[u8]) -> Option<u32>;

    // Do shift the section by the given "shift"
    fn shift(dts: &mut [u8], shift: isize) -> Result<(), ()> {
        let total_size = dts.len();
        let size = Self::get_size(dts).ok_or(())?;
        let src = get_be32(dts, Self::OFF_OFFSET)?; // Get the offset of the section
        let dst: u32 = match (src as usize).overflowing_add_signed(shift) {
            // Calculate the new offset of the section
            (dst, false) => dst.try_into().map_err(|_| ())?,
            (_, true) => return Err(()),
        };
        let src_end = src.checked_add(size).ok_or(())? as usize; // Calculate the end of the section
        let dst_end = dst.checked_add(size).ok_or(())? as usize; // Calculate the new end of the section

        if src_end > total_size || dst_end > total_size {
            return Err(()); // Check if the section is out of the device tree
        }

        set_be32(dts, Self::OFF_OFFSET, dst)?; // Set the new offset of the section
        dts.copy_within(src as usize..src_end, dst as usize); // Copy the section to the new offset
        Ok(())
    }
}

trait SizeSection: Section {
    // The offset of the section
    const OFF_OFFSET: usize;

    // The offset of the section's size
    const SIZE_OFFSET: usize;
}

impl<T: SizeSection> Section for T {
    const OFF_OFFSET: usize = <T as SizeSection>::OFF_OFFSET;

    fn get_size(dts: &[u8]) -> Option<u32> {
        let bytes = dts.get(T::SIZE_OFFSET..T::SIZE_OFFSET + 4)?;
        Some(u32::from_be_bytes(bytes.try_into().unwrap()))
    }
}

struct StructSection;

impl SizeSection for StructSection {
    const OFF_OFFSET: usize = offset::OFF_DT_STRUCT;
    const SIZE_OFFSET: usize = offset::SIZE_DT_STRUCT;
}

struct StringSection;

impl SizeSection for StringSection {
    const OFF_OFFSET: usize = offset::OFF_DT_STRINGS;
    const SIZE_OFFSET: usize = offset::SIZE_DT_STRINGS;
}

/// Error type for memory reservation block
pub struct MemRsvError();

impl Display for MemRsvError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "Memory reservation block error")
    }
}

/// Result type for memory reservation block
pub type MemRsvResult = Result<(), MemRsvError>;

/// Resize the memory reservation block
fn mem_rsvmap_resize(dts: &mut [u8], adj: isize) -> Result<(), ()> {
    // Add/Delete a fdt_reserve_entry need resize 2 u64 of the value of "address" and "size"
    let byteadj = adj * 2 * 8;

    match byteadj.cmp(&0) {
        // Geater than 0 means add a fdt_reserve_entry
        // StringSection is after StructSection, so the StringSection should be shifted first
        Ordering::Greater => {
            StringSection::shift(dts, byteadj)?;
            StructSection::shift(dts, byteadj)?;
        }
        // Less than 0 means delete a fdt_reserve_entry
        // StringSection is after StructSection, so the StructSection should be shifted first
        Ordering::Less => {
            StructSection::shift(dts, byteadj)?;
            StringSection::shift(dts, byteadj)?;
        }
        Ordering::Equal => (),
    }

    Ok(())
}

/// Append a memory reservation block to the device tree
///
/// This function appends given memory ranges, as pairs of (start, len) to stands for the memory reservation block.
pub fn mem_reserve(dts: &mut [u8], ranges: &[(u64, u64)]) -> MemRsvResult {
    // Resize the strcut and string section first
    mem_rsvmap_resize(dts, ranges.len() as isize).map_err(|()| MemRsvError())?;

    // Get the offset of the memory reservation block
    let offset = get_be32(dts, offset::OFF_MEM_RSVS).map_err(|()| MemRsvError())?;
    // Delete the data fronted the memory reservation block
    let rsvmap = &mut dts[offset as usize..];
    let mut it = rsvmap.chunks_exact_mut(16). // The size of a fdt_reserve_entry is 16 bytes
    skip_while(|chunk| get_be64(chunk, 0).unwrap() != 0 || get_be64(chunk, 8).unwrap() != 0); // Skip the empty fdt_reserve_entry

    for (base, size) in ranges {
        let chunk = it.next().ok_or(MemRsvError())?;

        set_be64(chunk, 0, *base).map_err(|()| MemRsvError())?;
        set_be64(chunk, 8, *size).map_err(|()| MemRsvError())?;
    }

    // Don't forget to set the end of the memory reservation block to 0
    let chunk = it.next().ok_or(MemRsvError())?;

    set_be64(chunk, 0, 0).map_err(|()| MemRsvError())?;
    set_be64(chunk, 8, 0).map_err(|()| MemRsvError())?;
    Ok(())
}
