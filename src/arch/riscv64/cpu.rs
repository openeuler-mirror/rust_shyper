use crate::kernel::current_cpu;

/// Mask (disable) interrupt from perspective of CPU
#[inline(always)]
pub fn cpu_interrupt_mask() {
    // SAFETY: Disable the interrupt for current hart.
    unsafe {
        riscv::register::sstatus::clear_sie();
    }
}

/// Unmask (enable) interrupt from perspective of CPU
#[inline(always)]
pub fn cpu_interrupt_unmask() {
    // SAFETY: Enable the interrupt for current hart.
    unsafe {
        riscv::register::sstatus::set_sie();
    }
}

#[inline(always)]
pub fn current_cpu_arch() -> u64 {
    let addr: u64;
    // SAFETY: The 'tp' register is used to store the current CPU pointer.
    unsafe {
        core::arch::asm!("mv {}, tp",
            out(reg) addr);
    }
    addr
}

pub fn get_current_cpuid() -> usize {
    current_cpu().id as usize
}

/// Set the current CPU pointer.
/// # Safety:
/// 1. The 'cpu_addr' must be a valid address.
/// 2. The memory pointed by 'cpu_addr' must have enough space to store the `Cpu` struct.
/// 3. The 'cpu_addr' must be aligned.
pub unsafe fn set_current_cpu(cpu_addr: u64) {
    core::arch::asm!("mv tp, {}",
        in(reg) cpu_addr);
}
