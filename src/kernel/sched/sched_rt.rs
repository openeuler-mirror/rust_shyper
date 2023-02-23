// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use crate::kernel::{Vcpu, Scheduler};

pub struct SchedulerRT {}

impl Scheduler for SchedulerRT {
    fn init(&mut self) {
        todo!()
    }

    fn next(&mut self) -> Option<Vcpu> {
        todo!()
    }

    fn do_schedule(&mut self) {
        todo!()
    }

    fn sleep(&mut self, vcpu: Vcpu) {
        todo!()
    }

    fn wakeup(&mut self, vcpu: Vcpu) {
        todo!()
    }

    fn yield_to(&mut self, vcpu: Vcpu) {
        todo!()
    }
}
