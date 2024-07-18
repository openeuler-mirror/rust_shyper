use riscv;

/// TODO: Current RISCV ISA doesn't support this feature.
/// Invalidate the data cache for the given address range.
/// # Safety:
/// The 'start' and 'len' must be valid address and length.
pub unsafe fn cache_invalidate_d(_start: usize, _len: usize) {}

/// TODO: Current RISCV ISA doesn't support this feature.
pub fn cache_clean_invalidate_d(_start: usize, _len: usize) {}

pub fn isb() {
    // SAFETY:
    // Fence_I only flushes the instruction cache, which doesn't have effect on data.
    unsafe { riscv::asm::fence_i() };
}

pub fn fence() {
    // SAFETY:
    // Fence allows all the previous load/store instructions to complete
    // before any subsequent load/store instructions are executed.
    unsafe { riscv::asm::fence() };
}
