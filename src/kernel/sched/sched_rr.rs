// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use alloc::vec::Vec;
use crate::kernel::{Vcpu, Scheduler, SchedulerUpdate, current_cpu, VcpuState, timer_enable, vm, run_idle_thread};

#[derive(Default)]
/// Round-Robin Scheduler struct
pub struct SchedulerRR {
    queue: Vec<Vcpu>,
    active_idx: usize,
    base_slice: usize,
}

impl SchedulerRR {
    pub fn new(slice: usize) -> Self {
        Self {
            queue: Vec::new(),
            active_idx: 0,
            base_slice: slice,
        }
    }
}

impl Scheduler for SchedulerRR {
    fn init(&mut self) {}

    /// Select the next vcpu object in the round-robin queue
    fn next(&mut self) -> Option<Vcpu> {
        let queue = &self.queue;
        let len = queue.len();
        for i in 1..=len {
            let idx = (self.active_idx + i) % len;
            match queue.get(idx) {
                Some(vcpu) => match vcpu.state() {
                    VcpuState::Invalid => {}
                    _ => {
                        self.active_idx = idx;
                        return Some(vcpu.clone());
                    }
                },
                None => panic!("len != 0 but front is None"),
            }
        }
        None
    }

    /// Schedule to the next vcpu object
    fn do_schedule(&mut self) {
        if let Some(next_vcpu) = self.next() {
            current_cpu().schedule_to(next_vcpu);
        } else {
            match current_cpu().ctx_ptr() {
                None => {
                    error!("run_idle_thread: cpu{} ctx is NULL", current_cpu().id);
                }
                Some(_ctx) => {
                    run_idle_thread();
                }
            }
        }
    }

    /// put vcpu into sleep, and remove it from scheduler
    fn sleep(&mut self, vcpu: Vcpu) {
        let mut need_schedule = false;
        {
            let queue = &mut self.queue;
            if let Some(idx) = queue.iter().position(|x| x.vm_id() == vcpu.vm_id()) {
                queue.remove(idx);
                match idx.cmp(&self.active_idx) {
                    core::cmp::Ordering::Less => {
                        self.active_idx -= 1;
                    }
                    core::cmp::Ordering::Equal => {
                        // cpu.active_vcpu need remove
                        current_cpu().set_active_vcpu(None);
                        need_schedule = true;
                    }
                    _ => {}
                }
            }
        }
        vcpu.set_state(VcpuState::Sleep);
        if self.queue.len() <= 1 {
            timer_enable(false);
        }
        if need_schedule {
            self.do_schedule();
        }
    }

    /// wakeup a vcpu, meaning that the vcpu is ready to be scheduled
    fn wakeup(&mut self, vcpu: Vcpu) {
        let queue = &mut self.queue;
        vcpu.set_state(VcpuState::Ready);
        queue.push(vcpu);
        if queue.len() > 1 {
            timer_enable(true);
        }
        if queue.len() == 1 {
            self.do_schedule();
        }
    }

    /// yield to another cpu, only used when vcpu is new added and want to be excuted immediately
    fn yield_to(&mut self, vcpu: Vcpu) {
        let queue: &mut Vec<Vcpu> = &mut self.queue;
        queue.push(vcpu.clone());
        self.active_idx = queue.len() - 1;
        current_cpu().schedule_to(vcpu);
        if queue.len() > 1 {
            timer_enable(true);
        }
    }
}

impl SchedulerUpdate for SchedulerRR {
    fn update(&self) -> Self {
        let src_rr = self;
        let mut new_rr = SchedulerRR::default();
        for vcpu in src_rr.queue.iter() {
            let vm_id = vcpu.vm_id();
            let vcpu_id = vcpu.id();
            let vm = vm(vm_id).unwrap();
            new_rr.queue.push(vm.vcpu(vcpu_id).unwrap().clone());
        }
        new_rr.active_idx = src_rr.active_idx;
        new_rr.base_slice = src_rr.base_slice;

        let active_vcpu = if src_rr.active_idx < src_rr.queue.len() {
            Some(new_rr.queue[src_rr.active_idx].clone())
        } else {
            debug!("Core[{}] is none", current_cpu().id);
            None
        };
        current_cpu().set_active_vcpu(active_vcpu);
        new_rr
    }
}
