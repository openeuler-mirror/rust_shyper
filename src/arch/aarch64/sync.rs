// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use crate::arch::{
    exception_data_abort_access_is_sign_ext, exception_data_abort_access_is_write, exception_data_abort_access_reg,
    exception_data_abort_access_reg_width, exception_data_abort_access_width, exception_data_abort_handleable,
    exception_data_abort_is_permission_fault, exception_data_abort_is_translate_fault, exception_iss,
};
use crate::arch::{exception_esr, exception_fault_addr};
use crate::arch::exception_next_instruction_step;
use crate::arch::smc_guest_handler;
use crate::device::{emu_handler, EmuContext};
use crate::kernel::{active_vm, current_cpu, hvc_guest_handler, migrate_data_abort_handler};

pub const HVC_RETURN_REG: usize = 0;

pub fn data_abort_handler() {
    // let time0 = time_current_us();
    let emu_ctx = EmuContext {
        address: exception_fault_addr(),
        width: exception_data_abort_access_width(),
        write: exception_data_abort_access_is_write(),
        sign_ext: exception_data_abort_access_is_sign_ext(),
        reg: exception_data_abort_access_reg(),
        reg_width: exception_data_abort_access_reg_width(),
    };
    let elr = current_cpu().get_elr();

    if !exception_data_abort_handleable() {
        panic!(
            "Core {} data abort not handleable 0x{:x}, esr 0x{:x}",
            current_cpu().id,
            exception_fault_addr(),
            exception_esr()
        );
    }

    if !exception_data_abort_is_translate_fault() {
        if exception_data_abort_is_permission_fault() {
            // println!(
            //     "write {}, width {}, reg width {}, addr {:x}, iss {:x}, reg idx {}, reg val 0x{:x}, esr 0x{:x}",
            //     exception_data_abort_access_is_write(),
            //     emu_ctx.width,
            //     emu_ctx.reg_width,
            //     emu_ctx.address,
            //     exception_iss(),
            //     emu_ctx.reg,
            //     current_cpu().get_gpr(emu_ctx.reg),
            //     exception_esr()
            // );
            migrate_data_abort_handler(&emu_ctx);
            // no need to rewrite elr

            // let time1 = time_current_us();
            // println!("migrate_data_abort_handler: {}us", time1 - time0);
            return;
        } else {
            panic!(
                "Core {} data abort is not translate fault 0x{:x}",
                current_cpu().id,
                exception_fault_addr(),
            );
        }
    }
    // if current_cpu().id == 1 {
    // println!("emu_handler address:{:#x}", emu_ctx.address);
    // }
    if !emu_handler(&emu_ctx) {
        active_vm().unwrap().show_pagetable(emu_ctx.address);
        println!(
            "write {}, width {}, reg width {}, addr {:x}, iss {:x}, reg idx {}, reg val 0x{:x}, esr 0x{:x}",
            exception_data_abort_access_is_write(),
            emu_ctx.width,
            emu_ctx.reg_width,
            emu_ctx.address,
            exception_iss(),
            emu_ctx.reg,
            current_cpu().get_gpr(emu_ctx.reg),
            exception_esr()
        );
        panic!(
            "data_abort_handler: Failed to handler emul device request, ipa 0x{:x} elr 0x{:x}",
            emu_ctx.address, elr
        );
    }
    let val = elr + exception_next_instruction_step();
    current_cpu().set_elr(val);
}

pub fn smc_handler() {
    let fid = current_cpu().get_gpr(0);
    let x1 = current_cpu().get_gpr(1);
    let x2 = current_cpu().get_gpr(2);
    let x3 = current_cpu().get_gpr(3);

    if !smc_guest_handler(fid, x1, x2, x3) {
        warn!("smc_handler: unknown fid 0x{:x}", fid);
        current_cpu().set_gpr(0, 0);
    }

    let elr = current_cpu().get_elr();
    let val = elr + exception_next_instruction_step();
    current_cpu().set_elr(val);
}

pub fn hvc_handler() {
    // let time_start = timer_arch_get_counter();
    let x0 = current_cpu().get_gpr(0);
    let x1 = current_cpu().get_gpr(1);
    let x2 = current_cpu().get_gpr(2);
    let x3 = current_cpu().get_gpr(3);
    let x4 = current_cpu().get_gpr(4);
    let x5 = current_cpu().get_gpr(5);
    let x6 = current_cpu().get_gpr(6);
    let mode = current_cpu().get_gpr(7);

    let hvc_type = (mode >> 8) & 0xff;
    let event = mode & 0xff;

    match hvc_guest_handler(hvc_type, event, x0, x1, x2, x3, x4, x5, x6) {
        Ok(val) => {
            current_cpu().set_gpr(HVC_RETURN_REG, val);
        }
        Err(_) => {
            warn!("Failed to handle hvc request fid 0x{:x} event 0x{:x}", hvc_type, event);
            current_cpu().set_gpr(HVC_RETURN_REG, usize::MAX);
        }
    }
    // let time_end = timer_arch_get_counter();
    // println!(
    //     "hvc fid 0x{:x} event 0x{:x} counter {}, freq {:x}",
    //     hvc_type,
    //     event,
    //     time_end - time_start,
    //     timer_arch_get_frequency()
    // );
}
