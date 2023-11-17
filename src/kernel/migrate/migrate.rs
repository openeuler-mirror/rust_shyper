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
    GIC_LIST_REGS_NUM, GIC_PRIVINT_NUM, GIC_SGIS_NUM, GIC_SPI_MAX, IrqState, PAGE_SIZE, PTE_S2_FIELD_AP_RW,
    PTE_S2_NORMAL, PTE_S2_RO, Sgis,
};
use crate::arch::tlb_invalidate_guest_all;
use crate::device::{EmuContext, VirtioDeviceType, VirtMmioRegs, VirtDevData, DevDescData, VirtioMmioData, VirtqData};
use crate::kernel::{
    active_vm, get_share_mem, hvc_send_msg_to_vm, HVC_VMM, HVC_VMM_MIGRATE_START, HvcGuestMsg, HvcMigrateMsg,
    mem_pages_alloc, MIGRATE_BITMAP, MIGRATE_COPY, MIGRATE_FINISH, MIGRATE_SEND, vm, Vm, vm_if_copy_mem_map,
    vm_if_mem_map_cache, vm_if_mem_map_page_num, vm_if_set_mem_map, vm_if_set_mem_map_cache, VM_CONTEXT_RECEIVE,
    HVC_VMM_MIGRATE_VM_BOOT, send_hvc_ipi, VMData, VM_STATE_FLAG, VM_CONTEXT_SEND, vm_if_dirty_mem_map,
    vm_if_mem_map_dirty_sum, DIRTY_MEM_THRESHOLD, HVC_VMM_MIGRATE_FINISH, MIGRATE_RECEIVE,
};
use crate::utils::{set_barrier_num, round_up};
use core::mem::size_of;
use crate::vmm::vmm_remove_vm;

// virtio vgic migration data
pub struct VgicMigData {
    pub vgicd: VgicdData,
    pub cpu_priv_num: usize,
    pub cpu_priv: [VgicCpuPrivData; 4],
}

impl VgicMigData {
    pub fn default() -> VgicMigData {
        VgicMigData {
            vgicd: VgicdData::default(),
            cpu_priv_num: 0,
            cpu_priv: [VgicCpuPrivData::default(); 4], // TODO: 4 is hardcode for vm cpu num max
        }
    }
}

pub struct VgicdData {
    pub ctlr: u32,
    pub typer: u32,
    pub iidr: u32,
    pub interrupts: [VgicIntData; GIC_SPI_MAX],
}

impl VgicdData {
    pub fn default() -> VgicdData {
        VgicdData {
            ctlr: 0,
            typer: 0,
            iidr: 0,
            interrupts: [VgicIntData::default(); GIC_SPI_MAX],
        }
    }
}

#[derive(Copy, Clone)]
pub struct VgicCpuPrivData {
    pub curr_lrs: [u16; GIC_LIST_REGS_NUM],
    pub sgis: [Sgis; GIC_SGIS_NUM],
    pub interrupts: [VgicIntData; GIC_PRIVINT_NUM],
    pub pend_num: usize,
    pub pend_list: [usize; 16],
    // TODO: 16 is hard code
    pub act_num: usize,
    pub act_list: [usize; 16],
}

impl VgicCpuPrivData {
    pub fn default() -> VgicCpuPrivData {
        VgicCpuPrivData {
            curr_lrs: [0; GIC_LIST_REGS_NUM],
            sgis: [Sgis { pend: 0, act: 0 }; GIC_SGIS_NUM],
            interrupts: [VgicIntData::default(); GIC_PRIVINT_NUM],
            pend_num: 0,
            pend_list: [0; 16],
            act_num: 0,
            act_list: [0; 16],
        }
    }
}

#[derive(Copy, Clone)]
pub struct VgicIntData {
    pub owner: Option<usize>,
    // vcpu_id
    pub id: u16,
    pub hw: bool,
    pub in_lr: bool,
    pub lr: u16,
    pub enabled: bool,
    pub state: IrqState,
    pub prio: u8,
    pub targets: u8,
    pub cfg: u8,

    pub in_pend: bool,
    pub in_act: bool,
}

impl VgicIntData {
    pub fn default() -> VgicIntData {
        VgicIntData {
            owner: None,
            id: 0,
            hw: false,
            in_lr: false,
            lr: 0,
            enabled: false,
            state: IrqState::IrqSInactive,
            prio: 0,
            targets: 0,
            cfg: 0,
            in_pend: false,
            in_act: false,
        }
    }
}

// virtio mmio migration data

impl VirtioMmioData {
    pub fn default() -> VirtioMmioData {
        VirtioMmioData {
            id: 0,
            driver_features: 0,
            driver_status: 0,
            regs: VirtMmioRegs::default(),
            dev: VirtDevData::default(),
            oppo_dev: VirtDevData::default(),
            vq: [VirtqData::default(); 4],
        }
    }
}

impl VirtqData {
    pub fn default() -> VirtqData {
        VirtqData {
            ready: 0,
            vq_index: 0,
            num: 0,
            last_avail_idx: 0,
            last_used_idx: 0,
            used_flags: 0,
            desc_table_ipa: 0,
            avail_ipa: 0,
            used_ipa: 0,
        }
    }
}

impl VirtDevData {
    pub fn default() -> VirtDevData {
        VirtDevData {
            activated: false,
            dev_type: VirtioDeviceType::None,
            features: 0,
            generation: 0,
            int_id: 0,
            desc: DevDescData::None,
        }
    }
}

pub fn migrate_ready(vmid: usize) {
    if vm_if_mem_map_cache(vmid).is_none() {
        let trgt_vm = vm(vmid).unwrap();
        map_migrate_vm_mem(trgt_vm, get_share_mem(MIGRATE_SEND));
        match mem_pages_alloc(vm_if_mem_map_page_num(vmid)) {
            Ok(pf) => {
                active_vm().unwrap().pt_map_range(
                    get_share_mem(MIGRATE_BITMAP),
                    PAGE_SIZE * vm_if_mem_map_page_num(vmid),
                    pf.pa(),
                    PTE_S2_RO,
                    true,
                );
                vm_if_set_mem_map_cache(vmid, pf);
            }
            Err(_) => {
                panic!("migrate_ready: mem_pages_alloc failed");
            }
        }
    }
}

pub fn send_migrate_memcpy_msg(vmid: usize) {
    // copy trgt_vm dirty mem map to kernel module
    // println!("migrate_memcpy, vm_id {}", vmid);
    hvc_send_msg_to_vm(
        0,
        &HvcGuestMsg::Migrate(HvcMigrateMsg {
            fid: HVC_VMM,
            event: HVC_VMM_MIGRATE_START,
            vm_id: vmid,
            oper: MIGRATE_COPY,
            page_num: vm_if_mem_map_page_num(vmid),
        }),
    );
}

pub fn map_migrate_vm_mem(vm: Vm, ipa_start: usize) {
    let mut len = 0;
    for i in 0..vm.region_num() {
        active_vm()
            .unwrap()
            .pt_map_range(ipa_start + len, vm.pa_length(i), vm.pa_start(i), PTE_S2_NORMAL, true);
        len += vm.pa_length(i);
    }
}

pub fn unmap_migrate_vm_mem(vm: Vm, ipa_start: usize) {
    let mut len = 0;
    for i in 0..vm.region_num() {
        // println!("unmap_migrate_vm_mem, ipa_start {:x}, len {:x}", ipa_start, vm.pa_length(i));
        active_vm()
            .unwrap()
            .pt_unmap_range(ipa_start + len, vm.pa_length(i), true);
        len += vm.pa_length(i);
    }
}

pub fn migrate_finish_ipi_handler(vm_id: usize) {
    // println!("Core 0 handle VM[{}] finish ipi", vm_id);
    // let vm = vm(vm_id).unwrap();
    // copy trgt_vm dirty mem map to kernel module
    // let vm = vm(vm_id).unwrap();
    // for i in 0..vm.mem_region_num() {
    //     unsafe {
    //         cache_invalidate_d(vm.pa_start(i), vm.pa_length(i));
    //     }
    // }
    // tlb_invalidate_guest_all();
    vm_if_copy_mem_map(vm_id);

    hvc_send_msg_to_vm(
        0,
        &HvcGuestMsg::Migrate(HvcMigrateMsg {
            fid: HVC_VMM,
            event: HVC_VMM_MIGRATE_START,
            vm_id,
            oper: MIGRATE_FINISH,
            page_num: vm_if_mem_map_page_num(vm_id),
        }),
    );
}

pub fn migrate_data_abort_handler(emu_ctx: &EmuContext) {
    if emu_ctx.write {
        // ptr_read_write(emu_ctx.address, emu_ctx.width, val, false);
        let vm = active_vm().unwrap();
        // vm.show_pagetable(emu_ctx.address);
        let vm_id = vm.id();

        let (pa, len) = vm.pt_set_access_permission(emu_ctx.address, PTE_S2_FIELD_AP_RW);
        // println!(
        //     "migrate_data_abort_handler: emu_ctx addr 0x{:x}, write pa {:x}, len 0x{:x}",
        //     emu_ctx.address, pa, len
        // );
        let mut bit = 0;
        for i in 0..vm.region_num() {
            let start = vm.pa_start(i);
            let end = start + vm.pa_length(i);
            if pa >= start && pa < end {
                bit += (pa - active_vm().unwrap().pa_start(i)) / PAGE_SIZE;
                vm_if_set_mem_map(vm_id, bit, len / PAGE_SIZE);
                break;
            }
            bit += vm.pa_length(i) / PAGE_SIZE;
            if i + 1 == vm.region_num() {
                panic!(
                    "migrate_data_abort_handler: can not found addr 0x{:x} in vm{} pa region",
                    pa, vm_id
                );
            }
        }
        // flush tlb for updating page table
        tlb_invalidate_guest_all();
    } else {
        panic!("migrate_data_abort_handler: permission should be read only");
    }
}

fn mvm_migrate_memory(trgt_vmid: usize) {
    let vm = vm(trgt_vmid);
    vm.as_ref().unwrap().pt_read_only();
    // tlb_invalidate_guest_all();
    vm_if_copy_mem_map(trgt_vmid);
    send_migrate_memcpy_msg(trgt_vmid);
}

pub fn vmm_migrate_init_vm_hvc_handler(vm_id: usize) {
    info!("migrate init vm {}", vm_id);
    // vmm_init_gvm(x0);
    let vm = vm(vm_id).unwrap();
    map_migrate_vm_mem(vm.clone(), get_share_mem(MIGRATE_RECEIVE));
    vm.context_vm_migrate_init();
}

pub fn vmm_migrate_vm_boot_hvc_handler(vm_id: usize) {
    let mvm = vm(0).unwrap();
    let vm = vm(vm_id).unwrap();

    let size = size_of::<VMData>();
    mvm.pt_unmap_range(get_share_mem(VM_CONTEXT_RECEIVE), round_up(size, PAGE_SIZE), true);
    unmap_migrate_vm_mem(vm.clone(), get_share_mem(MIGRATE_RECEIVE));

    vm.context_vm_migrate_restore();
    for vcpu_id in 0..vm.cpu_num() {
        let cpu_trgt = vm.vcpuid_to_pcpuid(vcpu_id).unwrap();
        // send ipi to target vcpu, copy data and boot vm (in ipi copy gic data)
        send_hvc_ipi(0, vm_id, HVC_VMM, HVC_VMM_MIGRATE_VM_BOOT, cpu_trgt);
    }
}

pub fn vmm_migrate_finish_hvc_handler(vm_id: usize) {
    let mvm = vm(0).unwrap();
    let trgt_vm = vm(vm_id).unwrap();
    let size = size_of::<VMData>();
    mvm.pt_unmap_range(get_share_mem(VM_CONTEXT_SEND), round_up(size, PAGE_SIZE), true);
    mvm.pt_unmap_range(
        get_share_mem(MIGRATE_BITMAP),
        PAGE_SIZE * vm_if_mem_map_page_num(vm_id),
        true,
    );
    unmap_migrate_vm_mem(trgt_vm, get_share_mem(MIGRATE_SEND));
    vmm_remove_vm(vm_id);
    *VM_STATE_FLAG.lock() = 0;
}

pub fn vmm_migrate_ready_hvc_handler(vm_id: usize) {
    // init gvm dirty memory bitmap
    // let cpu_trgt = vm_if_get_cpu_id(x0);
    // println!(
    //     "core {} HVC_VMM_MIGRATE_READY, cpu trgt {}, vmid {}",
    //     current_cpu().id,
    //     cpu_trgt,
    //     x0
    // );
    migrate_ready(vm_id);
    mvm_migrate_memory(vm_id);
    vm_if_dirty_mem_map(vm_id);

    // send_hvc_ipi(0, x0, HVC_VMM, HVC_VMM_MIGRATE_READY, cpu_trgt);
}

pub fn vmm_migrate_memcpy_hvc_handler(vm_id: usize) {
    let dirty_mem_num = vm_if_mem_map_dirty_sum(vm_id);
    // let cpu_trgt = vm_if_get_cpu_id(vm_id);
    if dirty_mem_num < DIRTY_MEM_THRESHOLD {
        // Idle live vm, copy dirty mem and vm register struct
        let trgt_vm = vm(vm_id).unwrap();
        set_barrier_num(trgt_vm.cpu_num());
        for vcpu_id in 0..trgt_vm.cpu_num() {
            let pcpu_id = trgt_vm.vcpuid_to_pcpuid(vcpu_id).unwrap();
            send_hvc_ipi(0, vm_id, HVC_VMM, HVC_VMM_MIGRATE_FINISH, pcpu_id);
        }
    } else {
        mvm_migrate_memory(vm_id);
        // send_hvc_ipi(0, vm_id, HVC_VMM, HVC_VMM_MIGRATE_MEMCPY, cpu_trgt);
    }
}
