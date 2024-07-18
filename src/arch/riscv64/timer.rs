use sbi;
use riscv;
use spin::Mutex;

use crate::kernel::TIMER_INTERVAL;

pub static TIMER_FREQ: Mutex<usize> = Mutex::new(0);
// cycles per ms
pub static TIMER_SLICE: Mutex<usize> = Mutex::new(0);

pub fn timer_arch_disable_irq() {
    // SAFETY: Disable stie bit in sie register, to disable timer interrupt
    unsafe {
        riscv::register::sie::clear_stimer();
    };
}

pub fn timer_arch_enable_irq() {
    // SAFETY: Enable stie bit in sie register, to enable timer interrupt
    unsafe {
        riscv::register::sie::set_stimer();
    };
}

#[inline]
pub fn timer_arch_get_counter() -> usize {
    riscv::register::time::read()
}

/// riscv timer freq depends on board
pub fn timer_arch_get_frequency() -> usize {
    // TODO: get frequency by device tree
    1000_0000
}

/// set next timer's time, i.e. after num ms, trigger time intr
pub fn timer_arch_set(num_ms: usize) {
    let slice_lock = TIMER_SLICE.lock();
    let val = *slice_lock * num_ms;
    drop(slice_lock);
    let _ = sbi::timer::set_timer((timer_arch_get_counter() + val) as u64);
}

pub fn timer_arch_init() {
    timer_arch_enable_irq();
    let mut freq_lock = TIMER_FREQ.lock();
    let mut slice_lock = TIMER_SLICE.lock();
    *freq_lock = timer_arch_get_frequency();
    *slice_lock = (*freq_lock) / 1000;

    drop(freq_lock);
    drop(slice_lock);

    timer_arch_set(TIMER_INTERVAL);
}
