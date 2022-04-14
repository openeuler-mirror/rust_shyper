use crate::arch::gicc_clear_current_irq;
use crate::arch::power_arch_vm_shutdown_secondary_cores;
use crate::board::PLATFORM_CPU_NUM_MAX;
use crate::config::{
    init_tmp_config_for_bma1, init_tmp_config_for_bma2, init_tmp_config_for_ramdisk_vm1, init_tmp_config_for_vm1,
    init_tmp_config_for_vm2,
};
use crate::config::vm_cfg_entry;
use crate::device::create_fdt;
use crate::kernel::{
    active_vcpu_id, active_vm, current_cpu, vcpu_run, vm, Vm, vm_if_set_ivc_arg, vm_if_set_ivc_arg_ptr, vm_ipa2pa,
};
use crate::kernel::{active_vm_id, vm_if_get_cpu_id};
use crate::kernel::{ipi_send_msg, IpiInnerMsg, IpiMessage, IpiType, IpiVmmMsg};
use crate::vmm::{vmm_add_vm, vmm_assign_vcpu, vmm_boot, vmm_init_image, vmm_setup_config, vmm_setup_fdt};

#[derive(Copy, Clone)]
pub enum VmmEvent {
    VmmBoot,
    VmmReboot,
    VmmShutdown,
    VmmAssignCpu,
}

pub fn vmm_shutdown_secondary_vm() {
    println!("Shutting down all VMs...");
}

/* Set up VM structure and finish cpu assignment before set up VM config.
 * Only VM0 will go through this function.
 *
 * @param[in] vm_id: new added VM id.
 */
pub fn vmm_set_up_vm(vm_id: usize) {
    println!("vmm_set_up_vm: set up vm {} on cpu {}", vm_id, current_cpu().id);
    vmm_add_vm(vm_id);

    let mut vm = vm(vm_id).unwrap();

    let mut cpu_allocate_bitmap = vm.config().cpu_allocated_bitmap();
    let mut target_cpu_id = 0;
    let mut cpu_num = 0;
    while cpu_allocate_bitmap != 0 && target_cpu_id < PLATFORM_CPU_NUM_MAX {
        if cpu_allocate_bitmap & 1 != 0 {
            println!("vmm_set_up_vm: vm {} physical cpu id {}", vm_id, target_cpu_id);
            cpu_num += 1;

            let m = IpiVmmMsg {
                vmid: vm_id,
                event: VmmEvent::VmmAssignCpu,
            };
            if target_cpu_id != current_cpu().id {
                if !ipi_send_msg(target_cpu_id, IpiType::IpiTVMM, IpiInnerMsg::VmmMsg(m)) {
                    println!("vmm_set_up_vm: failed to send ipi to Core {}", target_cpu_id);
                }
            } else {
                vmm_assign_vcpu(vm_id);
            }
        }
        cpu_allocate_bitmap >>= 1;
        target_cpu_id += 1;
    }
    println!(
        "vmm_set_up_vm: vm {} total physical cpu num {} bitmap {:#b}",
        vm_id,
        cpu_num,
        vm.config().cpu_allocated_bitmap()
    );
}

/* Init VM before boot.
 * Only VM0 will go through this function.
 *
 * @param[in] vm_id: target VM id to boot.
 */
pub fn vmm_init_vm(vm_id: usize) {
    // Before boot, we need to set up the VM config.
    if current_cpu().id == 0 {
        if vm_id == 0 {
            panic!("not support boot for vm0");
        } else {
            let vm_cfg = match vm_cfg_entry(vm_id) {
                Some(vm_cfg) => {
                    println!("vmm_init_vm: VM {} config exists", vm_id);
                }
                None => {
                    // Hard Code for Guest VM config, not recommended.
                    if vm_id == 1 {
                        println!("vmm_init_vm: init_tmp_config_for_vm1");
                        init_tmp_config_for_vm1();
                    } else if vm_id == 2 {
                        println!("vmm_init_vm: init_tmp_config_for_vm2");
                        init_tmp_config_for_vm2();
                    }
                }
            };
        }

        vmm_set_up_vm(vm_id);
        loop {
            println!(
                "vmm_boot_vm: on core {},waiting vm[{}] to be set up",
                current_cpu().id,
                vm_id
            );
            let vm = match vm(vm_id) {
                None => {
                    panic!(
                        "vmm_boot_vm: on core {}, vm[{}] is not added yet",
                        current_cpu().id,
                        vm_id
                    );
                    // continue;
                }
                Some(vm) => vm,
            };
            if vm.ready() {
                break;
            }
        }
        vmm_setup_config(vm_id);
    } else {
        println!("Core {} should not init vm {}", current_cpu().id, vm_id);
    }
}

pub fn vmm_boot_vm(vm_id: usize) {
    let phys_id = vm_if_get_cpu_id(vm_id);
    // println!(
    //     "vmm_boot_vm: current_cpu {} target vm {} get phys_id {}",
    //     current_cpu().id,
    //     vm_id,
    //     phys_id
    // );
    if current_cpu().active_vcpu.clone().is_some() && vm_id == active_vm_id() {
        gicc_clear_current_irq(true);
        vmm_boot();
    } else {
        match current_cpu().vcpu_pool().pop_vcpuidx_through_vmid(vm_id) {
            None => {
                let m = IpiVmmMsg {
                    vmid: vm_id,
                    event: VmmEvent::VmmBoot,
                };
                if !ipi_send_msg(phys_id, IpiType::IpiTVMM, IpiInnerMsg::VmmMsg(m)) {
                    println!("vmm_boot_vm: failed to send ipi to Core {}", phys_id);
                }
            }
            Some(vcpu_idx) => {
                gicc_clear_current_irq(true);
                current_cpu().vcpu_pool().yield_vcpu(vcpu_idx);
                vmm_boot();
            }
        };
    }
}

pub fn vmm_reboot_vm(vm: Vm) {
    if vm.id() == 0 {
        vmm_shutdown_secondary_vm();
        crate::board::platform_sys_reboot();
    }
    let vcpu = current_cpu().active_vcpu.clone().unwrap();
    println!("VM {} reset...", vm.id());
    power_arch_vm_shutdown_secondary_cores(vm.clone());
    println!(
        "Core {} (vm {} vcpu {}) shutdown ok",
        current_cpu().id,
        vm.id(),
        active_vcpu_id()
    );

    let config = match vm_cfg_entry(vm.id()) {
        Some(_config) => _config,
        None => {
            panic!("vmm_setup_config vm id {} config doesn't exist", vm.id());
        }
    };
    if !vmm_init_image(vm.clone()) {
        panic!("vmm_reboot_vm: vmm_init_image failed");
    }
    if vm.id() != 0 {
        // init vm1 dtb
        match create_fdt(config.clone()) {
            Ok(dtb) => {
                let offset = config.device_tree_load_ipa() - vm.config().memory_region()[0].ipa_start;
                println!("dtb size {}", dtb.len());
                println!("pa 0x{:x}", vm.pa_start(0) + offset);
                crate::lib::memcpy_safe((vm.pa_start(0) + offset) as *const u8, dtb.as_ptr(), dtb.len());
            }
            _ => {
                panic!("vmm_setup_config: create fdt for vm{} fail", vm.id());
            }
        }
    } else {
        unsafe {
            vmm_setup_fdt(vm.clone());
        }
    }
    vm_if_set_ivc_arg(vm.id(), 0);
    vm_if_set_ivc_arg_ptr(vm.id(), 0);

    crate::arch::interrupt_arch_clear();
    crate::arch::vcpu_arch_init(vm.clone(), vm.vcpu(0).unwrap());
    vcpu.reset_context();
    vcpu_run();
}

pub fn get_vm_id(id_ipa: usize) -> bool {
    let vm = active_vm().unwrap();
    let id_pa = vm_ipa2pa(vm.clone(), id_ipa);
    if id_pa == 0 {
        println!("illegal id_pa {:x}", id_pa);
        return false;
    }
    unsafe {
        *(id_pa as *mut usize) = vm.id();
    }
    true
}

pub fn vmm_ipi_handler(msg: &IpiMessage) {
    match msg.ipi_message {
        IpiInnerMsg::VmmMsg(vmm) => match vmm.event {
            VmmEvent::VmmBoot => {
                vmm_boot_vm(vmm.vmid);
            }
            VmmEvent::VmmAssignCpu => {
                println!(
                    "vmm_ipi_handler: core {} receive assign vcpu request for vm[{}]",
                    current_cpu().id,
                    vmm.vmid
                );
                vmm_assign_vcpu(vmm.vmid);
            }
            _ => {
                todo!();
            }
        },
        _ => {
            println!("vmm_ipi_handler: illegal ipi type");
            return;
        }
    }
}
