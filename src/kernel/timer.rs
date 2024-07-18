// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use crate::arch::traits::InterruptController;
use crate::arch::{is_boot_core, IntCtrl};
use crate::kernel::{current_cpu, Scheduler};

// #[derive(Copy, Clone)]
// struct Timer(bool);

// impl Timer {
//     const fn default() -> Timer {
//         Timer(false)
//     }

//     fn set(&mut self, val: bool) {
//         self.0 = val;
//     }
// }

// static TIMER_LIST: Mutex<[Timer; PLATFORM_CPU_NUM_MAX]> =
//     Mutex::new([Timer::default(); PLATFORM_CPU_NUM_MAX]);

/// initialize timer on current cpu (This function needs to be executed on each cpu)
pub fn timer_init() {
    crate::arch::timer_arch_init();
    timer_enable(false);

    #[cfg(not(feature = "secondary_start"))]
    crate::utils::barrier();

    if is_boot_core(current_cpu().id) {
        crate::kernel::interrupt_reserve_int(IntCtrl::IRQ_HYPERVISOR_TIMER, timer_irq_handler);
        info!("Timer frequency: {}Hz", crate::arch::timer_arch_get_frequency());
        info!("Timer init ok");
    }
}

/// enable timer on current cpu
pub fn timer_enable(val: bool) {
    // println!(
    //     "Core {} {} EL2 timer",
    //     current_cpu().id,
    //     if val { "enable" } else { "disable" }
    // );
    super::interrupt::interrupt_cpu_enable(IntCtrl::IRQ_HYPERVISOR_TIMER, val);
}

/// trigger timer interrupt after X ms
fn timer_notify_after(ms: usize) {
    use crate::arch::{timer_arch_enable_irq, timer_arch_set};
    if ms == 0 {
        return;
    }

    timer_arch_set(ms);
    timer_arch_enable_irq();
}

pub const TIMER_INTERVAL: usize = 10;

pub fn timer_irq_handler() {
    use crate::arch::timer_arch_disable_irq;

    timer_arch_disable_irq();
    current_cpu().scheduler().do_schedule();

    #[cfg(target_arch = "riscv64")]
    {
        let vcpu_option = current_cpu().active_vcpu.as_ref();
        if let Some(vcpu) = vcpu_option {
            let next_timer = vcpu.inner.inner_mut.lock().vm_ctx.next_timer_intr;
            // next_timer == 0 indicates the initialization condition.
            // In this case, no clock interrupt is triggered
            if next_timer <= crate::arch::timer_arch_get_counter() as u64 && next_timer != 0 {
                // Timer is expired, inject interrupt!
                crate::arch::IntCtrl::vm_inject(vcpu.vm().as_ref().unwrap(), vcpu, crate::arch::IRQ_GUEST_TIMER);
            }
        }
    }

    timer_notify_after(TIMER_INTERVAL);
}
