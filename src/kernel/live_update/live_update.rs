// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet, LinkedList};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use crate::arch::ContextFrameTrait;
use spin::{Mutex, RwLock};
use crate::kernel::cpu::CPU_LIST;
use crate::arch::{
    invalid_hypervisor_all, current_cpu_arch, PAGE_SIZE, PAGE_SHIFT, ContextFrame, set_current_cpu, Aarch64ContextFrame,
};

use crate::arch::{
    emu_intc_handler, emu_smmu_handler, GIC_LRS_NUM, gic_maintenance_handler, gicc_clear_current_irq, INTERRUPT_EN_SET,
    PageTable, partial_passthrough_intc_handler, SMMU_V2, SmmuV2, TIMER_FREQ, TIMER_SLICE, Vgic, LVL2_PAGE_TABLE,
    update_request_asm,
};
use crate::board::PLAT_DESC;
use crate::config::{
    DEF_VM_CONFIG_TABLE, vm_cfg_entry, VmConfigEntry, VmConfigTable, VmDtbDevConfig, VMDtbDevConfigList,
    VmEmulatedDeviceConfig, VmEmulatedDeviceConfigList, VmMemoryConfig, VmPassthroughDeviceConfig,
};
use crate::device::{
    BlkIov, EMU_DEVS_LIST, emu_virtio_mmio_handler, EmuDevEntry, EmuDeviceType, EmuDevs, MEDIATED_BLK_LIST,
    MediatedBlk, virtio_blk_notify_handler, virtio_console_notify_handler, virtio_mediated_blk_notify_handler,
    virtio_net_notify_handler, VirtioMmio,
};
use crate::kernel::{
    async_blk_io_req, ASYNC_EXE_STATUS, ASYNC_IO_TASK_LIST, async_ipi_req, ASYNC_IPI_TASK_LIST, ASYNC_USED_INFO_LIST,
    AsyncExeStatus, AsyncTask, AsyncTaskData, Cpu, CPU_IF_LIST, CpuIf, CpuState, current_cpu, FairQueue, HEAP_REGION,
    HeapRegion, INTERRUPT_GLB_BITMAP, INTERRUPT_HANDLERS, INTERRUPT_HYPER_BITMAP, InterruptHandler, IoAsyncMsg,
    ipi_irq_handler, ipi_send_msg, IpiInnerMsg, IpiMediatedMsg, IpiMessage, IpiType, SchedType, SchedulerUpdate,
    SHARE_MEM_LIST, timer_irq_handler, UsedInfo, Vcpu, VCPU_LIST, VcpuInner, vm, Vm, VM_IF_LIST, vm_ipa2pa, VM_LIST,
    VM_NUM_MAX, VM_REGION, VmInterface, VmRegion, logger_init, CPU_STACK_SIZE, clear_bss, find_vcpu_by_id,
    IPI_HANDLER_LIST, run_idle_thread,
};
use crate::utils::{BitAlloc256, BitMap, FlexBitmap, time_current_us, memcpy_safe};
use crate::mm::{heap_init, PageFrame};

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum FreshStatus {
    Start,
    FreshVM,
    FreshVCPU,
    Finish,
    None,
}

static FRESH_STATUS: RwLock<FreshStatus> = RwLock::new(FreshStatus::None);

pub static FRESH_LOGIC_LOCK: Mutex<()> = Mutex::new(());
pub static FRESH_IRQ_LOGIC_LOCK: Mutex<()> = Mutex::new(());
pub static RUST_SHYPER_UPDATE_LOCK: Mutex<()> = Mutex::new(());

fn set_fresh_status(status: FreshStatus) {
    *FRESH_STATUS.write() = status;
}

pub fn fresh_status() -> FreshStatus {
    *FRESH_STATUS.read()
}

#[cfg(not(feature = "update"))]
pub const UPDATE_IMG_BASE_ADDR: usize = 0x8a000000;
#[cfg(feature = "update")]
pub const UPDATE_IMG_BASE_ADDR: usize = 0x83000000;

#[repr(C)]
pub struct HypervisorAddr {
    cpu_id: usize,
    vm_list: usize,
    vm_config_table: usize,
    vcpu_list: usize,
    cpu: usize,
    emu_dev_list: usize,
    interrupt_hyper_bitmap: usize,
    interrupt_glb_bitmap: usize,
    interrupt_en_set: usize,
    interrupt_handlers: usize,
    vm_region: usize,
    heap_region: usize,
    vm_if_list: usize,
    gic_lrs_num: usize,
    // address for ipi
    cpu_if_list: usize,
    ipi_handler_list: usize,
    // arch time
    time_freq: usize,
    time_slice: usize,
    // mediated blk
    mediated_blk_list: usize,
    // async task
    async_exe_status: usize,
    async_ipi_task_list: usize,
    async_io_task_list: usize,
    async_used_info_list: usize,
    // shared mem
    shared_mem_list: usize,
    // smmu_v2
    smmu_v2: usize,
    //CPU lvl2 page dir for devices
    hva2pa_lvl2_base: usize,
}

pub fn update_request() {
    // VM_STATE_FLAG UNILIB_FS_LIST SYSTEM_FDT
    let vm_config_table = &DEF_VM_CONFIG_TABLE as *const _ as usize;
    let emu_dev_list = &EMU_DEVS_LIST as *const _ as usize;
    let interrupt_hyper_bitmap = &INTERRUPT_HYPER_BITMAP as *const _ as usize;
    let interrupt_glb_bitmap = &INTERRUPT_GLB_BITMAP as *const _ as usize;
    let interrupt_en_set = &INTERRUPT_EN_SET as *const _ as usize;
    let interrupt_handlers = &INTERRUPT_HANDLERS as *const _ as usize;
    let vm_region = &VM_REGION as *const _ as usize;
    let heap_region = &HEAP_REGION as *const _ as usize;
    let vm_list = &VM_LIST as *const _ as usize;
    let vm_if_list = &VM_IF_LIST as *const _ as usize;
    let vcpu_list = &VCPU_LIST as *const _ as usize;
    let cpu = current_cpu_arch() as usize;
    let cpu_if_list = &CPU_IF_LIST as *const _ as usize;
    let gic_lrs_num = &GIC_LRS_NUM as *const _ as usize;
    let ipi_handler_list = &IPI_HANDLER_LIST as *const _ as usize;
    let time_freq = &TIMER_FREQ as *const _ as usize;
    let time_slice = &TIMER_SLICE as *const _ as usize;
    let mediated_blk_list = &MEDIATED_BLK_LIST as *const _ as usize;
    let async_exe_status = &ASYNC_EXE_STATUS as *const _ as usize;
    let async_ipi_task_list = &ASYNC_IPI_TASK_LIST as *const _ as usize;
    let async_io_task_list = &ASYNC_IO_TASK_LIST as *const _ as usize;
    let async_used_info_list = &ASYNC_USED_INFO_LIST as *const _ as usize;
    let shared_mem_list = &SHARE_MEM_LIST as *const _ as usize;
    let smmu_v2 = &SMMU_V2 as *const _ as usize;
    let hva2pa_lvl2_base = unsafe { &LVL2_PAGE_TABLE } as *const _ as usize;

    let addr_list = HypervisorAddr {
        cpu_id: current_cpu().id,
        vm_config_table,
        emu_dev_list,
        interrupt_hyper_bitmap,
        interrupt_glb_bitmap,
        interrupt_en_set,
        interrupt_handlers,
        vm_region,
        heap_region,
        vm_list,
        vm_if_list,
        vcpu_list,
        cpu,
        cpu_if_list,
        gic_lrs_num,
        ipi_handler_list,
        time_freq,
        time_slice,
        mediated_blk_list,
        async_exe_status,
        async_ipi_task_list,
        async_io_task_list,
        async_used_info_list,
        shared_mem_list,
        smmu_v2,
        hva2pa_lvl2_base,
    };
    if current_cpu().id == 0 {
        unsafe {
            update_request_asm(&addr_list, true);
        }
        for cpu_id in 0..PLAT_DESC.cpu_desc.num {
            if cpu_id != current_cpu().id {
                ipi_send_msg(cpu_id, IpiType::IpiTHyperFresh, IpiInnerMsg::HyperFreshMsg());
            }
        }
    }
    unsafe {
        update_request_asm(&addr_list, false);
    }
}

#[no_mangle]
pub extern "C" fn rust_shyper_update(address_list: &HypervisorAddr, alloc: bool) {
    // TODO: vm0_dtb?

    if alloc {
        // cpu id is 0
        unsafe {
            clear_bss();
        }
        heap_init();
        // alloc and pre_copy
        unsafe {
            // // HEAP_REGION
            let heap_region = &*(address_list.heap_region as *const Mutex<HeapRegion>);
            heap_region_update(heap_region);

            // DEF_VM_CONFIG_TABLE
            let vm_config_table = &*(address_list.vm_config_table as *const Mutex<VmConfigTable>);
            vm_config_table_update(vm_config_table);

            // VM_LIST
            let vm_list = &*(address_list.vm_list as *const Mutex<Vec<Vm>>);
            vm_list_alloc(vm_list);

            // VCPU_LIST
            let vcpu_list = &*(address_list.vcpu_list as *const Mutex<Vec<Vcpu>>);
            vcpu_list_alloc(vcpu_list);

            // CPU_IF
            let cpu_if = &*(address_list.cpu_if_list as *const Mutex<Vec<CpuIf>>);
            cpu_if_alloc(cpu_if);

            // TIMER_FREQ & TIMER_SLICE
            let time_freq = &*(address_list.time_freq as *const Mutex<usize>);
            let time_slice = &*(address_list.time_slice as *const Mutex<usize>);
            arch_time_update(time_freq, time_slice);

            // INTERRUPT_HYPER_BITMAP, INTERRUPT_GLB_BITMAP, INTERRUPT_HANDLERS
            let interrupt_hyper_bitmap = &*(address_list.interrupt_hyper_bitmap as *const Mutex<BitMap<BitAlloc256>>);
            let interrupt_glb_bitmap = &*(address_list.interrupt_glb_bitmap as *const Mutex<BitMap<BitAlloc256>>);
            let interrutp_en_set = &*(address_list.interrupt_en_set as *const Mutex<BTreeSet<usize>>);
            let interrupt_handlers =
                &*(address_list.interrupt_handlers as *const Mutex<BTreeMap<usize, InterruptHandler>>);
            interrupt_update(
                interrupt_hyper_bitmap,
                interrupt_glb_bitmap,
                interrutp_en_set,
                interrupt_handlers,
            );

            // EMU_DEVS_LIST
            let emu_dev_list = &*(address_list.emu_dev_list as *const Mutex<Vec<EmuDevEntry>>);
            emu_dev_list_update(emu_dev_list);

            // GIC_LRS_NUM
            let gic_lrs_num = &*(address_list.gic_lrs_num as *const Mutex<usize>);
            gic_lrs_num_update(gic_lrs_num);
        }
        info!("Finish Alloc VM / VCPU / CPU_IF");
        return;
    }
    let lock = RUST_SHYPER_UPDATE_LOCK.lock();
    if address_list.cpu_id == 0 {
        let lock0 = FRESH_LOGIC_LOCK.lock();
        let lock1 = FRESH_IRQ_LOGIC_LOCK.lock();
        set_fresh_status(FreshStatus::Start);
        unsafe {
            // VM_LIST
            let time0 = time_current_us();
            let vm_list = &*(address_list.vm_list as *const Mutex<Vec<Vm>>);
            vm_list_update(vm_list);
            info!("Finish update vm list");
            set_fresh_status(FreshStatus::FreshVM);
            let time1 = time_current_us();

            // VCPU_LIST (add vgic)
            let vcpu_list = &*(address_list.vcpu_list as *const Mutex<Vec<Vcpu>>);
            vcpu_update(vcpu_list, vm_list);
            let time2 = time_current_us();
            drop(lock1);
            set_fresh_status(FreshStatus::FreshVCPU);
            let time3 = time_current_us();
            info!("Finish Update VCPU_LIST");
            // CPU: Must update after vcpu and vm
            let cpu = &*(address_list.cpu as *const Cpu);
            current_cpu_update(cpu, address_list.hva2pa_lvl2_base);
            info!("Update CPU[{}]", cpu.id);
            // VM_REGION
            let vm_region = &*(address_list.vm_region as *const Mutex<VmRegion>);
            vm_region_update(vm_region);
            info!("Update {} region for VM_REGION", VM_REGION.lock().region.len());
            // HEAP_REGION
            let heap_region = &*(address_list.heap_region as *const Mutex<HeapRegion>);
            heap_region_update(heap_region);
            info!("Update HEAP_REGION");
            // VM_IF_LIST
            let vm_if_list = &*(address_list.vm_if_list as *const [Mutex<VmInterface>; VM_NUM_MAX]);
            vm_if_list_update(vm_if_list);
            info!("Update VM_IF_LIST");
            // MEDIATED_BLK_LIST
            let mediated_blk_list = &*(address_list.mediated_blk_list as *const Mutex<Vec<MediatedBlk>>);
            mediated_blk_list_update(mediated_blk_list);
            info!("Update {} Mediated BLK", MEDIATED_BLK_LIST.lock().len());
            // SHARED_MEM_LIST
            let shared_mem_list = &*(address_list.shared_mem_list as *const Mutex<BTreeMap<usize, usize>>);
            shared_mem_list_update(shared_mem_list);
            info!("Update {} SHARE_MEM_LIST", SHARE_MEM_LIST.lock().len());
            // cpu_if_list
            let cpu_if = &*(address_list.cpu_if_list as *const Mutex<Vec<CpuIf>>);
            cpu_if_update(cpu_if);
            info!("Update CPU_IF_LIST");
            // ASYNC_EXE_STATUS、ASYNC_IPI_TASK_LIST、ASYNC_IO_TASK_LIST、ASYNC_USED_INFO_LIST
            let async_exe_status = &*(address_list.async_exe_status as *const Mutex<AsyncExeStatus>);
            let async_ipi_task_list = &*(address_list.async_ipi_task_list as *const Mutex<LinkedList<AsyncTask>>);
            let async_io_task_list = &*(address_list.async_io_task_list as *const Mutex<FairQueue<AsyncTask>>);
            let async_used_info_list =
                &*(address_list.async_used_info_list as *const Mutex<BTreeMap<usize, LinkedList<UsedInfo>>>);
            async_task_update(
                async_exe_status,
                async_ipi_task_list,
                async_io_task_list,
                async_used_info_list,
            );

            // SMMU_V2
            let smmu_v2 = &*(address_list.smmu_v2 as *const Mutex<SmmuV2>);
            smmu_update(smmu_v2);
            // LOGGER
            let _ = logger_init();
            set_fresh_status(FreshStatus::Finish);
            drop(lock0);
            info!(
                "handle VM {} us, handle VCPU {} us, free lock {} us",
                time1 - time0,
                time2 - time1,
                time3 - time2
            );
        }
    } else {
        let cpu = unsafe { &*(address_list.cpu as *const Cpu) };
        current_cpu_update(cpu, address_list.hva2pa_lvl2_base);
    }
    drop(lock);
    fresh_hyper();
}

pub fn fresh_hyper() {
    extern "C" {
        pub fn fresh_cpu();
        pub fn fresh_hyper_asm(ctx: usize);
        fn update_stack(dst_stack_base: usize, src_stack_base: usize);
    }
    let dst_cpu = unsafe { &mut CPU_LIST[current_cpu().id] };
    assert_eq!(dst_cpu.id, current_cpu().id);
    let new_ttbr0_el2 = &dst_cpu.cpu_pt.lvl1 as *const _ as u64;
    debug!("new new_ttbr0_el2 addr is {:x}", new_ttbr0_el2);
    use cortex_a::registers::*;
    let dst_stack_base = &dst_cpu.stack as *const _ as usize;
    dst_cpu.stack.clone_from(&current_cpu().stack);
    invalid_hypervisor_all();
    set_current_cpu(dst_cpu as *const _ as u64);
    TTBR0_EL2.set(new_ttbr0_el2);

    invalid_hypervisor_all();

    unsafe { crate::arch::cache_invalidate_d(dst_stack_base, CPU_STACK_SIZE) };
    let ctx = current_cpu().ctx as usize;
    if current_cpu().id == 0 {
        info!("CPU[{}] ctx {:x}", current_cpu().id, ctx);
        current_cpu().clear_ctx();
        unsafe { fresh_hyper_asm(ctx) };
    } else {
        match current_cpu().cpu_state {
            CpuState::CpuInv => {
                panic!("Core[{}] state {:#?}", current_cpu().id, CpuState::CpuInv);
            }
            CpuState::CpuIdle => {
                run_idle_thread();
                gicc_clear_current_irq(true);
                unsafe { fresh_hyper_asm(current_cpu().ctx as usize) };
            }
            CpuState::CpuRun => {
                info!("Core[{}] state {:#?}", current_cpu().id, CpuState::CpuRun);
                gicc_clear_current_irq(true);
                let ctx = current_cpu().ctx as usize;
                current_cpu().clear_ctx();
                unsafe { fresh_hyper_asm(ctx) };
            }
        }
    }
}

pub fn shared_mem_list_update(src_shared_mem_list: &Mutex<BTreeMap<usize, usize>>) {
    info!("share mem list addr {:x}", &SHARE_MEM_LIST as *const _ as usize);
    match SHARE_MEM_LIST.try_lock() {
        Some(mut shared_mem_list) => {
            info!("try src mem list addr {:x}", &src_shared_mem_list as *const _ as usize);
            for (key, val) in src_shared_mem_list.lock().iter() {
                shared_mem_list.insert(*key, *val);
            }
        }
        None => {
            panic!("get SHARE_MEM_LIST fail");
        }
    }
}

pub fn async_task_update(
    src_async_exe_status: &Mutex<AsyncExeStatus>,
    src_async_ipi_task_list: &Mutex<LinkedList<AsyncTask>>,
    src_async_io_task_list: &Mutex<FairQueue<AsyncTask>>,
    src_async_used_info_list: &Mutex<BTreeMap<usize, LinkedList<UsedInfo>>>,
) {
    let mut async_exe_status = ASYNC_EXE_STATUS.lock();
    let mut async_ipi_task_list = ASYNC_IPI_TASK_LIST.lock();
    let mut async_io_task_list = ASYNC_IO_TASK_LIST.lock();
    let mut async_used_info_list = ASYNC_USED_INFO_LIST.lock();
    assert_eq!(async_ipi_task_list.len(), 0);
    assert_eq!(async_io_task_list.len(), 0);
    assert_eq!(async_used_info_list.len(), 0);
    *async_exe_status = *src_async_exe_status.lock();
    for ipi_task in src_async_ipi_task_list.lock().iter() {
        let vm_id = ipi_task.src_vmid;
        let vm = vm(vm_id).unwrap();
        let task_data = match &ipi_task.task_data {
            AsyncTaskData::AsyncIpiTask(mediated_msg) => {
                assert_eq!(mediated_msg.src_id, vm_id);
                let mmio_id = mediated_msg.blk.id();
                let vq_idx = mediated_msg.vq.vq_indx();
                match vm.emu_dev(mmio_id) {
                    EmuDevs::VirtioBlk(blk) => {
                        let new_vq = blk.vq(vq_idx).clone().unwrap();
                        AsyncTaskData::AsyncIpiTask(IpiMediatedMsg {
                            src_id: vm_id,
                            vq: new_vq.clone(),
                            blk: blk.clone(),
                        })
                    }
                    _ => panic!("illegal mmio dev type in async_task_update"),
                }
            }
            AsyncTaskData::AsyncIoTask(_) => panic!("Find an IO Task in IPI task list"),
            AsyncTaskData::AsyncNoneTask(_) => panic!("Find an IO None Task in IPI task list"),
        };
        async_ipi_task_list.push_back(AsyncTask {
            task_data,
            src_vmid: vm_id,
            state: Arc::new(Mutex::new(*ipi_task.state.lock())),
            task: Arc::new(Mutex::new(Box::pin(async_ipi_req()))),
        })
    }
    while !src_async_io_task_list.lock().is_empty() {
        let io_task = src_async_io_task_list.lock().pop_front().unwrap();
        let vm_id = io_task.src_vmid;
        let vm = vm(vm_id).unwrap();
        let task_data = match &io_task.task_data {
            AsyncTaskData::AsyncIpiTask(_) => panic!("Find an IPI Task in IO task list"),
            AsyncTaskData::AsyncIoTask(io_msg) => {
                assert_eq!(vm_id, io_msg.src_vmid);
                let vq_idx = io_msg.vq.vq_indx();
                match vm.emu_blk_dev() {
                    EmuDevs::VirtioBlk(blk) => {
                        let new_vq = blk.vq(vq_idx).clone().unwrap();
                        AsyncTaskData::AsyncIoTask(IoAsyncMsg {
                            src_vmid: vm_id,
                            vq: new_vq.clone(),
                            dev: blk.clone(),
                            io_type: io_msg.io_type,
                            blk_id: io_msg.blk_id,
                            sector: io_msg.sector,
                            count: io_msg.count,
                            cache: io_msg.cache,
                            iov_list: Arc::new({
                                let mut list = vec![];
                                for iov in io_msg.iov_list.iter() {
                                    list.push(BlkIov {
                                        data_bg: iov.data_bg,
                                        len: iov.len,
                                    });
                                }
                                list
                            }),
                        })
                    }
                    _ => panic!("illegal mmio dev type in async_task_update"),
                }
            }
            _ => {
                todo!()
            }
        };
        async_io_task_list.push_back(AsyncTask {
            task_data,
            src_vmid: vm_id,
            state: Arc::new(Mutex::new(*io_task.state.lock())),
            task: Arc::new(Mutex::new(Box::pin(async_blk_io_req()))),
        });
    }
    for (key, used_info) in src_async_used_info_list.lock().iter() {
        let mut new_used_info = LinkedList::new();
        for info in used_info.iter() {
            new_used_info.push_back(UsedInfo {
                desc_chain_head_idx: info.desc_chain_head_idx,
                used_len: info.used_len,
            });
        }
        async_used_info_list.insert(*key, new_used_info);
    }
}

pub fn mediated_blk_list_update(src_mediated_blk_list: &Mutex<Vec<MediatedBlk>>) {
    let mut mediated_blk_list = MEDIATED_BLK_LIST.lock();
    assert_eq!(mediated_blk_list.len(), 0);
    mediated_blk_list.clear();
    for blk in src_mediated_blk_list.lock().iter() {
        mediated_blk_list.push(MediatedBlk {
            base_addr: blk.base_addr,
            avail: blk.avail,
        });
    }
}

pub fn arch_time_update(src_time_freq: &Mutex<usize>, src_time_slice: &Mutex<usize>) {
    *TIMER_FREQ.lock() = *src_time_freq.lock();
    *TIMER_SLICE.lock() = *src_time_slice.lock();
}

pub fn cpu_if_alloc(src_cpu_if: &Mutex<Vec<CpuIf>>) {
    let mut cpu_if_list = CPU_IF_LIST.lock();
    for _ in 0..src_cpu_if.lock().len() {
        cpu_if_list.push(CpuIf::default());
    }
}

pub fn cpu_if_update(src_cpu_if: &Mutex<Vec<CpuIf>>) {
    let mut cpu_if_list = CPU_IF_LIST.lock();
    assert_eq!(cpu_if_list.len(), src_cpu_if.lock().len());
    for (idx, cpu_if) in src_cpu_if.lock().iter().enumerate() {
        for (msg_idx, msg) in cpu_if.msg_queue.iter().enumerate() {
            // Copy ipi msg
            let new_ipi_msg = match msg.ipi_message.clone() {
                IpiInnerMsg::Initc(initc) => IpiInnerMsg::Initc(initc),
                IpiInnerMsg::Power(power) => IpiInnerMsg::Power(power),
                IpiInnerMsg::EnternetMsg(eth_msg) => IpiInnerMsg::EnternetMsg(eth_msg),
                IpiInnerMsg::VmmMsg(vmm_msg) => IpiInnerMsg::VmmMsg(vmm_msg),
                IpiInnerMsg::VcpuMsg(vcpu_msg) => IpiInnerMsg::VcpuMsg(vcpu_msg),
                IpiInnerMsg::MediatedMsg(mediated_msg) => {
                    let mmio_id = mediated_msg.blk.id();
                    let vm_id = mediated_msg.src_id;
                    let vq_idx = mediated_msg.vq.vq_indx();

                    let vm = vm(vm_id).unwrap();
                    match vm.emu_dev(mmio_id) {
                        EmuDevs::VirtioBlk(blk) => {
                            let new_vq = blk.vq(vq_idx).clone().unwrap();
                            IpiInnerMsg::MediatedMsg(IpiMediatedMsg {
                                src_id: vm_id,
                                vq: new_vq.clone(),
                                blk: blk.clone(),
                            })
                        }
                        _ => {
                            panic!("illegal mmio dev type in cpu_if_update");
                        }
                    }
                }
                IpiInnerMsg::MediatedNotifyMsg(notify_msg) => IpiInnerMsg::MediatedNotifyMsg(notify_msg),
                IpiInnerMsg::HvcMsg(hvc_msg) => IpiInnerMsg::HvcMsg(hvc_msg),
                IpiInnerMsg::IntInjectMsg(inject_msg) => IpiInnerMsg::IntInjectMsg(inject_msg),
                IpiInnerMsg::HyperFreshMsg() => IpiInnerMsg::HyperFreshMsg(),
                IpiInnerMsg::None => IpiInnerMsg::None,
            };
            cpu_if_list[idx].msg_queue.insert(
                msg_idx,
                IpiMessage {
                    ipi_type: msg.ipi_type,
                    ipi_message: new_ipi_msg,
                },
            );
        }
    }
}

pub fn vm_if_list_update(src_vm_if_list: &[Mutex<VmInterface>; VM_NUM_MAX]) {
    for (idx, vm_if_lock) in src_vm_if_list.iter().enumerate() {
        let vm_if = vm_if_lock.lock();
        let mut cur_vm_if = VM_IF_LIST[idx].lock();
        cur_vm_if.master_cpu_id = vm_if.master_cpu_id;
        cur_vm_if.state = vm_if.state;
        cur_vm_if.vm_type = vm_if.vm_type;
        cur_vm_if.mac = vm_if.mac;
        cur_vm_if.ivc_arg = vm_if.ivc_arg;
        cur_vm_if.ivc_arg_ptr = vm_if.ivc_arg_ptr;
        cur_vm_if.mem_map = vm_if.mem_map.as_ref().map(|mem_map| FlexBitmap {
            len: mem_map.len,
            map: {
                let mut map = vec![];
                for v in mem_map.map.iter() {
                    map.push(*v);
                }
                map
            },
        });
        cur_vm_if.mem_map_cache = vm_if
            .mem_map_cache
            .as_ref()
            .map(|cache| Arc::new(PageFrame::new(cache.pa, cache.page_num)));
    }
}

fn current_cpu_update(src_cpu: &Cpu, hva2pa_lvl2_base: usize) {
    // only need to alloc a new VcpuPool from heap, other props all map at 0x400000000
    let dst_cpu = unsafe { &mut CPU_LIST[src_cpu.id] };
    let lvl2_page_table = unsafe { &LVL2_PAGE_TABLE } as *const _ as usize;
    match &src_cpu.sched {
        SchedType::SchedRR(rr) => {
            dst_cpu.sched = SchedType::SchedRR(rr.update());
        }
        SchedType::None => {
            dst_cpu.sched = SchedType::None;
        }
    }
    dst_cpu.id = src_cpu.id;
    dst_cpu.cpu_state = src_cpu.cpu_state;
    dst_cpu.active_vcpu = match &src_cpu.active_vcpu {
        Some(active_vcpu) => find_vcpu_by_id(active_vcpu.id()),
        None => None,
    };
    let dst_stack_base = dst_cpu.stack.as_ptr() as usize;
    let src_stack_base = src_cpu.stack.as_ptr() as usize;
    dst_cpu.ctx = (src_cpu.ctx as usize + dst_stack_base - src_stack_base) as *mut Aarch64ContextFrame;
    for vcpu in src_cpu.vcpu_array.iter().flatten() {
        dst_cpu.vcpu_array.append_vcpu(find_vcpu_by_id(vcpu.id()).unwrap());
    }
    dst_cpu.current_irq = src_cpu.current_irq;

    dst_cpu.cpu_pt.clone_from(&src_cpu.cpu_pt);

    memcpy_safe(lvl2_page_table as *mut u8, hva2pa_lvl2_base as *mut u8, PAGE_SIZE);
    dst_cpu.cpu_pt.lvl1[0] &= (1 << PAGE_SHIFT) - 1;
    dst_cpu.cpu_pt.lvl1[0] |= lvl2_page_table >> PAGE_SHIFT << PAGE_SHIFT;
    let sp = unsafe { &*(src_cpu.ctx as *mut ContextFrame) }.stack_pointer();
    let new_sp = sp + dst_stack_base - src_stack_base;
    unsafe { &mut *(dst_cpu.ctx as *mut ContextFrame) }.set_stack_pointer(new_sp);

    info!("Update CPU[{}]", dst_cpu.id);
}

fn gic_lrs_num_update(src_gic_lrs_num: &Mutex<usize>) {
    let gic_lrs_num = *src_gic_lrs_num.lock();
    *GIC_LRS_NUM.lock() = gic_lrs_num;
    info!("Update GIC_LRS_NUM");
}

// alloc vm_list
fn vm_list_alloc(src_vm_list: &Mutex<Vec<Vm>>) {
    let mut vm_list = VM_LIST.lock();
    for vm in src_vm_list.lock().iter() {
        let new_vm = Vm::new(vm.id());
        vm_list.push(new_vm.clone());
        let mut dst_inner = new_vm.inner.lock();
        let src_inner = vm.inner.lock();
        let pt = match &src_inner.pt {
            None => None,
            Some(page_table) => {
                let new_page_table = PageTable {
                    directory: Arc::new(PageFrame::new(page_table.directory.pa, page_table.directory.page_num)),
                    pages: Arc::new(Mutex::new(vec![])),
                };
                for page in page_table.pages.lock().iter() {
                    new_page_table.pages.lock().push(PageFrame::new(page.pa, page.page_num));
                }
                Some(new_page_table)
            }
        };
        dst_inner.ready = src_inner.ready;
        dst_inner.config = vm_cfg_entry(src_inner.id);
        dst_inner.pt = pt;
        dst_inner.mem_region_num = src_inner.mem_region_num;
        dst_inner.pa_region = {
            let mut pa_region = vec![];
            for region in src_inner.pa_region.iter() {
                pa_region.push(*region);
            }
            pa_region
        };
        dst_inner.entry_point = src_inner.entry_point;
        dst_inner.has_master = src_inner.has_master;
        dst_inner.cpu_num = src_inner.cpu_num;
        dst_inner.ncpu = src_inner.ncpu;
        dst_inner.intc_dev_id = src_inner.intc_dev_id;
        dst_inner.int_bitmap = src_inner.int_bitmap;
        dst_inner.share_mem_base = src_inner.share_mem_base;
        dst_inner.migrate_save_pf = {
            let mut pf = vec![];
            for page in src_inner.migrate_save_pf.iter() {
                pf.push(PageFrame::new(page.pa, page.page_num));
            }
            pf
        };
        dst_inner.migrate_restore_pf = {
            let mut pf = vec![];
            for page in src_inner.migrate_restore_pf.iter() {
                pf.push(PageFrame::new(page.pa, page.page_num));
            }
            pf
        };
        dst_inner.med_blk_id = src_inner.med_blk_id;
    }
    assert_eq!(vm_list.len(), src_vm_list.lock().len());
    info!("Alloc {} VM in VM_LIST", vm_list.len());
}

// Set vm.vcpu_list in vcpu_update
fn vm_list_update(src_vm_list: &Mutex<Vec<Vm>>) {
    // let mut vm_list = VM_LIST.lock();
    assert_eq!(VM_LIST.lock().len(), src_vm_list.lock().len());
    for (idx, vm) in src_vm_list.lock().iter().enumerate() {
        let emu_devs = {
            let mut emu_devs = vec![];
            let old_emu_devs = vm.inner.lock().emu_devs.clone();
            for dev in old_emu_devs.iter() {
                // TODO: wip
                let new_dev = match dev {
                    EmuDevs::Vgic(_) => {
                        // set vgic after vcpu update
                        EmuDevs::None
                    }
                    EmuDevs::VirtioBlk(blk) => {
                        let mmio = VirtioMmio::new(0);
                        assert_eq!(
                            (blk.vq(0).unwrap().desc_table()),
                            vm_ipa2pa(vm.clone(), blk.vq(0).unwrap().desc_table_addr())
                        );
                        assert_eq!(
                            (blk.vq(0).unwrap().used()),
                            vm_ipa2pa(vm.clone(), blk.vq(0).unwrap().used_addr())
                        );
                        assert_eq!(
                            (blk.vq(0).unwrap().avail()),
                            vm_ipa2pa(vm.clone(), blk.vq(0).unwrap().avail_addr())
                        );
                        mmio.save_mmio(
                            blk.clone(),
                            if blk.dev().mediated() {
                                Some(virtio_mediated_blk_notify_handler)
                            } else {
                                Some(virtio_blk_notify_handler)
                            },
                        );
                        EmuDevs::VirtioBlk(mmio)
                    }
                    EmuDevs::VirtioNet(net) => {
                        let mmio = VirtioMmio::new(0);
                        assert_eq!(
                            (net.vq(0).unwrap().desc_table()),
                            vm_ipa2pa(vm.clone(), net.vq(0).unwrap().desc_table_addr())
                        );
                        assert_eq!(
                            (net.vq(0).unwrap().used()),
                            vm_ipa2pa(vm.clone(), net.vq(0).unwrap().used_addr())
                        );
                        assert_eq!(
                            (net.vq(0).unwrap().avail()),
                            vm_ipa2pa(vm.clone(), net.vq(0).unwrap().avail_addr())
                        );
                        info!("VirtioNet save handler {:x}", unsafe {
                            *(&virtio_net_notify_handler as *const _ as *const usize)
                        });
                        mmio.save_mmio(net.clone(), Some(virtio_net_notify_handler));
                        EmuDevs::VirtioNet(mmio)
                    }
                    EmuDevs::VirtioConsole(console) => {
                        let mmio = VirtioMmio::new(0);
                        assert_eq!(
                            (console.vq(0).unwrap().desc_table()),
                            vm_ipa2pa(vm.clone(), console.vq(0).unwrap().desc_table_addr())
                        );
                        assert_eq!(
                            (console.vq(0).unwrap().used()),
                            vm_ipa2pa(vm.clone(), console.vq(0).unwrap().used_addr())
                        );
                        assert_eq!(
                            (console.vq(0).unwrap().avail()),
                            vm_ipa2pa(vm.clone(), console.vq(0).unwrap().avail_addr())
                        );
                        info!("VirtioConsole save handler {:x}", unsafe {
                            *(&virtio_console_notify_handler as *const _ as *const usize)
                        });
                        mmio.save_mmio(console.clone(), Some(virtio_console_notify_handler));
                        EmuDevs::VirtioConsole(mmio)
                    }
                    EmuDevs::None => EmuDevs::None,
                };
                emu_devs.push(new_dev);
            }
            emu_devs
        };
        let dst_vm = &(VM_LIST.lock()[idx]);
        let mut dst_inner = dst_vm.inner.lock();
        let src_inner = vm.inner.lock();
        assert_eq!(dst_inner.id, src_inner.id);
        dst_inner.emu_devs = emu_devs;
    }
}

fn heap_region_update(src_heap_region: &Mutex<HeapRegion>) {
    let mut heap_region = HEAP_REGION.lock();
    let src_region = src_heap_region.lock();
    heap_region.map = src_region.map;
    heap_region.region = src_region.region;
    assert_eq!(heap_region.region, src_region.region);
}

fn vm_region_update(src_vm_region: &Mutex<VmRegion>) {
    let mut vm_region = VM_REGION.lock();
    assert_eq!(vm_region.region.len(), 0);
    vm_region.region.clear();
    for mem_region in src_vm_region.lock().region.iter() {
        vm_region.region.push(*mem_region);
    }
    assert_eq!(vm_region.region, src_vm_region.lock().region);
}

fn interrupt_update(
    src_hyper_bitmap: &Mutex<BitMap<BitAlloc256>>,
    src_glb_bitmap: &Mutex<BitMap<BitAlloc256>>,
    src_en_set: &Mutex<BTreeSet<usize>>,
    src_handlers: &Mutex<BTreeMap<usize, InterruptHandler>>,
) {
    let mut hyper_bitmap = INTERRUPT_HYPER_BITMAP.lock();
    *hyper_bitmap = *src_hyper_bitmap.lock();
    let mut glb_bitmap = INTERRUPT_GLB_BITMAP.lock();
    *glb_bitmap = *src_glb_bitmap.lock();
    let mut handlers = INTERRUPT_HANDLERS.lock();
    for (int_id, handler) in src_handlers.lock().iter() {
        match handler {
            InterruptHandler::IpiIrqHandler(_) => {
                handlers.insert(*int_id, InterruptHandler::IpiIrqHandler(ipi_irq_handler));
            }
            InterruptHandler::GicMaintenanceHandler(_) => {
                handlers.insert(
                    *int_id,
                    InterruptHandler::GicMaintenanceHandler(gic_maintenance_handler),
                );
            }
            InterruptHandler::TimeIrqHandler(_) => {
                handlers.insert(*int_id, InterruptHandler::TimeIrqHandler(timer_irq_handler));
            }
            InterruptHandler::None => {
                handlers.insert(*int_id, InterruptHandler::None);
            }
        }
    }
    let mut en_set = INTERRUPT_EN_SET.lock();
    (*en_set).extend(&*src_en_set.lock());
    info!("Update INTERRUPT_GLB_BITMAP / INTERRUPT_HYPER_BITMAP / INTERRUPT_EN_SET / INTERRUPT_HANDLERS");
}

fn emu_dev_list_update(src_emu_dev_list: &Mutex<Vec<EmuDevEntry>>) {
    let mut emu_dev_list = EMU_DEVS_LIST.lock();
    assert_eq!(emu_dev_list.len(), 0);
    emu_dev_list.clear();
    for emu_dev_entry in src_emu_dev_list.lock().iter() {
        let emu_handler = match emu_dev_entry.emu_type {
            EmuDeviceType::EmuDeviceTGicd => emu_intc_handler,
            EmuDeviceType::EmuDeviceTGPPT => partial_passthrough_intc_handler,
            EmuDeviceType::EmuDeviceTVirtioBlk => emu_virtio_mmio_handler,
            EmuDeviceType::EmuDeviceTVirtioNet => emu_virtio_mmio_handler,
            EmuDeviceType::EmuDeviceTVirtioConsole => emu_virtio_mmio_handler,
            EmuDeviceType::EmuDeviceTIOMMU => emu_smmu_handler,
            _ => {
                panic!("not support emu dev entry type {}", emu_dev_entry.emu_type);
            }
        };
        emu_dev_list.push(EmuDevEntry {
            emu_type: emu_dev_entry.emu_type,
            vm_id: emu_dev_entry.vm_id,
            id: emu_dev_entry.id,
            ipa: emu_dev_entry.ipa,
            size: emu_dev_entry.size,
            handler: emu_handler,
        });
    }
    info!("Update {} emu dev for EMU_DEVS_LIST", emu_dev_list.len());
}

fn vm_config_table_update(src_vm_config_table: &Mutex<VmConfigTable>) {
    let mut vm_config_table = DEF_VM_CONFIG_TABLE.lock();
    let src_config_table = src_vm_config_table.lock();
    vm_config_table.name = src_config_table.name;
    vm_config_table.vm_bitmap = src_config_table.vm_bitmap;
    vm_config_table.vm_num = src_config_table.vm_num;
    assert_eq!(vm_config_table.entries.len(), 0);
    vm_config_table.entries.clear();
    for entry in src_config_table.entries.iter() {
        let image = *entry.image.lock();
        let memory = VmMemoryConfig {
            region: {
                let mut region = vec![];
                for mem in entry.memory.lock().region.iter() {
                    region.push(*mem);
                }
                assert_eq!(region, entry.memory.lock().region);
                region
            },
        };
        let cpu = *entry.cpu.lock();
        // emu dev config
        let mut vm_emu_dev_confg = VmEmulatedDeviceConfigList { emu_dev_list: vec![] };
        let src_emu_dev_confg_list = entry.vm_emu_dev_confg.lock();
        for emu_config in &src_emu_dev_confg_list.emu_dev_list {
            vm_emu_dev_confg.emu_dev_list.push(VmEmulatedDeviceConfig {
                name: emu_config.name.clone(),
                base_ipa: emu_config.base_ipa,
                length: emu_config.length,
                irq_id: emu_config.irq_id,
                cfg_list: {
                    let mut cfg_list = vec![];
                    for cfg in emu_config.cfg_list.iter() {
                        cfg_list.push(*cfg);
                    }
                    assert_eq!(cfg_list, emu_config.cfg_list);
                    cfg_list
                },
                emu_type: emu_config.emu_type,
                mediated: emu_config.mediated,
            })
        }
        // passthrough dev config
        let src_pt = entry.vm_pt_dev_confg.lock();
        let mut vm_pt_dev_confg = VmPassthroughDeviceConfig {
            regions: vec![],
            irqs: vec![],
            streams_ids: vec![],
        };
        for region in src_pt.regions.iter() {
            vm_pt_dev_confg.regions.push(*region);
        }
        for irq in src_pt.irqs.iter() {
            vm_pt_dev_confg.irqs.push(*irq);
        }
        for streams_id in src_pt.streams_ids.iter() {
            vm_pt_dev_confg.streams_ids.push(*streams_id);
        }
        assert_eq!(vm_pt_dev_confg.regions, src_pt.regions);
        assert_eq!(vm_pt_dev_confg.irqs, src_pt.irqs);
        assert_eq!(vm_pt_dev_confg.streams_ids, src_pt.streams_ids);

        // dtb config
        let mut vm_dtb_devs = VMDtbDevConfigList {
            dtb_device_list: vec![],
        };
        let src_dtb_confg_list = entry.vm_dtb_devs.lock();
        for dtb_config in src_dtb_confg_list.dtb_device_list.iter() {
            vm_dtb_devs.dtb_device_list.push(VmDtbDevConfig {
                name: String::from(&dtb_config.name),
                dev_type: dtb_config.dev_type,
                irqs: {
                    let mut irqs = vec![];
                    for irq in dtb_config.irqs.iter() {
                        irqs.push(*irq);
                    }
                    assert_eq!(irqs, dtb_config.irqs);
                    irqs
                },
                addr_region: dtb_config.addr_region,
            });
        }
        let fdt_overlay = entry.fdt_overlay.lock();
        vm_config_table.entries.push(VmConfigEntry {
            id: entry.id,
            name: entry.name.clone(),
            os_type: entry.os_type,
            cmdline: String::from(&entry.cmdline),
            image: Arc::new(Mutex::new(image)),
            memory: Arc::new(Mutex::new(memory)),
            cpu: Arc::new(Mutex::new(cpu)),
            vm_emu_dev_confg: Arc::new(Mutex::new(vm_emu_dev_confg)),
            vm_pt_dev_confg: Arc::new(Mutex::new(vm_pt_dev_confg)),
            vm_dtb_devs: Arc::new(Mutex::new(vm_dtb_devs)),
            fdt_overlay: Arc::new(Mutex::new(fdt_overlay.clone())),
        });
    }
    assert_eq!(vm_config_table.entries.len(), src_config_table.entries.len());
    assert_eq!(vm_config_table.vm_num, src_config_table.vm_num);
    assert_eq!(vm_config_table.vm_bitmap, src_config_table.vm_bitmap);
    assert_eq!(vm_config_table.name, src_config_table.name);
    info!("Update {} VM to DEF_VM_CONFIG_TABLE", vm_config_table.vm_num);
}

fn vcpu_list_alloc(src_vcpu_list: &Mutex<Vec<Vcpu>>) {
    let mut vcpu_list = VCPU_LIST.lock();
    for vcpu in src_vcpu_list.lock().iter() {
        let src_inner = vcpu.inner.lock();
        let src_vm_option = src_inner.vm.clone();
        let vm = match src_vm_option {
            None => None,
            Some(src_vm) => {
                let vm_id = src_vm.id();
                vm(vm_id)
            }
        };
        let mut vcpu_inner = VcpuInner::default();
        vcpu_inner.vm = vm.clone();
        vcpu_inner.id = src_inner.id;
        vcpu_inner.phys_id = src_inner.phys_id;
        let vcpu = Vcpu {
            inner: Arc::new(Mutex::new(vcpu_inner)),
        };
        vm.unwrap().push_vcpu(vcpu.clone());
        vcpu_list.push(vcpu);
    }
    assert_eq!(vcpu_list.len(), src_vcpu_list.lock().len());
    info!("Alloc {} VCPU to VCPU_LIST", vcpu_list.len());
}

fn vcpu_update(src_vcpu_list: &Mutex<Vec<Vcpu>>, src_vm_list: &Mutex<Vec<Vm>>) {
    let vcpu_list = VCPU_LIST.lock();
    // assert_eq!(vcpu_list.len(), src_vcpu_list.lock().len());
    for (idx, vcpu) in src_vcpu_list.lock().iter().enumerate() {
        let src_inner = vcpu.inner.lock();
        let mut dst_inner = vcpu_list[idx].inner.lock();

        // assert_eq!(dst_inner.id, src_inner.id);
        // assert_eq!(dst_inner.phys_id, src_inner.phys_id);
        dst_inner.state = src_inner.state;
        dst_inner.int_list = {
            let mut int_list = vec![];
            for int in src_inner.int_list.iter() {
                int_list.push(*int);
            }
            int_list
        };
        dst_inner.vcpu_ctx = src_inner.vcpu_ctx;
        dst_inner.vm_ctx = src_inner.vm_ctx;
        // assert_eq!(dst_inner.int_list, src_inner.int_list);
    }

    // Add vgic emu dev for vm
    for src_vm in src_vm_list.lock().iter() {
        let src_vgic = src_vm.vgic();
        let new_vgic = Vgic::default();
        new_vgic.save_vgic(src_vgic.clone());

        let vm = vm(src_vm.id()).unwrap();
        if let EmuDevs::None = vm.emu_dev(vm.intc_dev_id()) {
            vm.set_emu_devs(vm.intc_dev_id(), EmuDevs::Vgic(Arc::new(new_vgic)));
        } else {
            panic!("illegal vgic emu dev idx in vm.emu_devs");
        }
    }
}

fn smmu_update(src_smmu_v2: &Mutex<SmmuV2>) {
    let mut smmu_v2 = SMMU_V2.lock();
    let src_smmu = src_smmu_v2.lock();
    smmu_v2.glb_rs0 = src_smmu.glb_rs0;
    smmu_v2.glb_rs1 = src_smmu.glb_rs1;
    smmu_v2.context_s2_idx = src_smmu.context_s2_idx;
    for ctx_bank in src_smmu.context_bank.iter() {
        smmu_v2.context_bank.push(*ctx_bank);
    }
    smmu_v2.context_alloc_bitmap = match &src_smmu.context_alloc_bitmap {
        Some(ctx_bitmap) => {
            let mut bitmap = FlexBitmap::new(ctx_bitmap.len);
            for v in ctx_bitmap.map.iter() {
                bitmap.map.push(*v);
            }
            Some(bitmap)
        }
        None => None,
    };

    smmu_v2.smr_num = src_smmu.smr_num;
    smmu_v2.smr_alloc_bitmap = match &src_smmu.smr_alloc_bitmap {
        Some(smr_bitmap) => {
            let mut bitmap = FlexBitmap::new(smr_bitmap.len);
            for v in smr_bitmap.map.iter() {
                bitmap.map.push(*v);
            }
            Some(bitmap)
        }
        None => None,
    };
    smmu_v2.group_alloc_bitmap = match &src_smmu.group_alloc_bitmap {
        Some(group_bitmap) => {
            let mut bitmap = FlexBitmap::new(group_bitmap.len);
            for v in group_bitmap.map.iter() {
                bitmap.map.push(*v);
            }
            Some(bitmap)
        }
        None => None,
    };
}
