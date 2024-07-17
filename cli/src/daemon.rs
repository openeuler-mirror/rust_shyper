// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use std::{
    fs::{self, File},
    io::Read,
    mem, process,
};
use libc::{c_char, c_int, c_ulong, c_ulonglong, close, ioctl, open, uintptr_t, O_RDWR, SIGTERM, SIGUSR1};
use serde::{Serialize, Deserialize};
use log::{debug, error, info, warn};
use signal_hook::iterator::Signals;

use crate::{
    blk::{mediated_blk_add, mediated_blk_init, mediated_blk_read, mediated_blk_write, MediatedBlkCfg, MED_BLK_LIST},
    config::copy_img_file_to_memory,
    ioctl_arg::{IOCTL_SYS, IOCTL_SYS_GET_KERNEL_IMG_NAME},
    util::cstr_arr_to_string,
    vmm::vmm_boot,
};

#[derive(Serialize, Deserialize, Debug)]
struct DaemonConfig {
    mediated: Vec<String>,
}

#[repr(C)]
struct HvcType {
    hvc_fid: u64,
    hvc_event: u64,
}

#[repr(C)]
struct BlkArg {
    hvc_fid: u64,
    hvc_event: u64,
    blk_id: u16,
    r#type: u32,
    sector: u64,
    count: u64,
}

#[repr(C)]
struct CfgArg {
    hvc_fid: u64,
    hvc_event: u64,
    vm_id: u64,
}

// hvc_fid
pub const HVC_SYS: usize = 0;
pub const HVC_VMM: usize = 1;
pub const HVC_IVC: usize = 2;
pub const HVC_MEDIATED: usize = 3;
pub const HVC_CONFIG: usize = 0x11;
#[cfg(feature = "unilib")]
pub const HVC_UNILIB: usize = 0x12;

// hvc_sys_event
pub const HVC_SYS_REBOOT: usize = 0;
pub const HVC_SYS_SHUTDOWN: usize = 1;
pub const HVC_SYS_UPDATE: usize = 3;
pub const HVC_SYS_TEST: usize = 4;
pub const HVC_SYS_UPDATE_MEM_MAP: usize = 5;

// hvc_vmm_event
pub const HVC_VMM_LIST_VM: usize = 0;
pub const HVC_VMM_GET_VM_STATE: usize = 1;
pub const HVC_VMM_BOOT_VM: usize = 2;
pub const HVC_VMM_SHUTDOWN_VM: usize = 3;
pub const HVC_VMM_REBOOT_VM: usize = 4;
pub const HVC_VMM_GET_VM_DEF_CFG: usize = 5;
pub const HVC_VMM_GET_VM_CFG: usize = 6;
pub const HVC_VMM_SET_VM_CFG: usize = 7;
pub const HVC_VMM_GET_VM_ID: usize = 8;
pub const HVC_VMM_TRACE_VMEXIT: usize = 9;
// for src vm: send msg to MVM to ask for migrating
pub const HVC_VMM_MIGRATE_START: usize = 10;
pub const HVC_VMM_MIGRATE_READY: usize = 11;
// for sender: copy dirty memory to receiver
pub const HVC_VMM_MIGRATE_MEMCPY: usize = 12;
pub const HVC_VMM_MIGRATE_FINISH: usize = 13;
// for receiver: init new vm but not boot
pub const HVC_VMM_MIGRATE_INIT_VM: usize = 14;
pub const HVC_VMM_MIGRATE_VM_BOOT: usize = 15;
pub const HVC_VMM_VM_REMOVE: usize = 16;

// hvc_ivc_event
pub const HVC_IVC_UPDATE_MQ: usize = 0;
pub const HVC_IVC_SEND_MSG: usize = 1;
pub const HVC_IVC_BROADCAST_MSG: usize = 2;
pub const HVC_IVC_INIT_KEEP_ALIVE: usize = 3;
pub const HVC_IVC_KEEP_ALIVE: usize = 4;
pub const HVC_IVC_ACK: usize = 5;
pub const HVC_IVC_GET_TIME: usize = 6;
pub const HVC_IVC_SHARE_MEM: usize = 7;
pub const HVC_IVC_SEND_SHAREMEM: usize = 0x10;
//shared mem communication
pub const HVC_IVC_GET_SHARED_MEM_IPA: usize = 0x11;
pub const HVC_IVC_SEND_SHAREMEM_TEST_SPEED: usize = 0x12;

// hvc_mediated_event
pub const HVC_MEDIATED_DEV_APPEND: usize = 0x30;
pub const HVC_MEDIATED_DEV_NOTIFY: usize = 0x31;
pub const HVC_MEDIATED_DRV_NOTIFY: usize = 0x32;
pub const HVC_MEDIATED_USER_NOTIFY: usize = 0x20;
// hvc_config_event
pub const HVC_CONFIG_ADD_VM: usize = 0;
pub const HVC_CONFIG_DELETE_VM: usize = 1;
pub const HVC_CONFIG_CPU: usize = 2;
pub const HVC_CONFIG_MEMORY_REGION: usize = 3;
pub const HVC_CONFIG_EMULATED_DEVICE: usize = 4;
pub const HVC_CONFIG_PASSTHROUGH_DEVICE_REGION: usize = 5;
pub const HVC_CONFIG_PASSTHROUGH_DEVICE_IRQS: usize = 6;
pub const HVC_CONFIG_PASSTHROUGH_DEVICE_STREAMS_IDS: usize = 7;
pub const HVC_CONFIG_DTB_DEVICE: usize = 8;
pub const HVC_CONFIG_UPLOAD_KERNEL_IMAGE: usize = 9;
pub const HVC_CONFIG_MEMORY_COLOR_BUDGET: usize = 10;
pub const HVC_CONFIG_UPLOAD_DEVICE_TREE: usize = 11;

pub fn generate_hvc_mode(fid: usize, event: usize) -> usize {
    ((fid << 8) | event) & 0xffff
}

// Execute the signal processing function only once
fn sig_handle_event(signal: i32) {
    // info!("Receive signal {}", signal);

    let mut file = File::open("/dev/shyper").unwrap();
    const HVC_TYPE_SIZE: usize = mem::size_of::<HvcType>();
    const BLK_ARG_SIZE: usize = mem::size_of::<BlkArg>();
    const CONFIG_ARG_SIZE: usize = mem::size_of::<CfgArg>();
    let mut buf: [u8; 256] = [0; 256];

    let n = file.read(&mut buf).unwrap();
    drop(file);

    if n == 0 {
        warn!("Lost signal {}!", signal);
    }
    let hvc_type: HvcType;

    unsafe {
        // try_into cast &[u8] to &[u8; HVC_TYPE_SIZE]
        hvc_type = mem::transmute::<[u8; HVC_TYPE_SIZE], HvcType>(buf[0..HVC_TYPE_SIZE].try_into().unwrap());
    }

    match hvc_type.hvc_fid as usize {
        HVC_MEDIATED => match hvc_type.hvc_event as usize {
            HVC_MEDIATED_USER_NOTIFY => {
                let blk_arg;
                unsafe {
                    blk_arg = mem::transmute::<[u8; BLK_ARG_SIZE], BlkArg>(buf[0..BLK_ARG_SIZE].try_into().unwrap());
                }
                if blk_arg.r#type == 0 {
                    mediated_blk_read(blk_arg.blk_id, blk_arg.sector, blk_arg.count);
                } else if blk_arg.r#type == 1 {
                    mediated_blk_write(blk_arg.blk_id, blk_arg.sector, blk_arg.count);
                } else {
                    warn!("[sig_handle_event] unknown blk req type {}", blk_arg.r#type);
                }
                return;
            }
            _ => return,
        },
        HVC_CONFIG => match hvc_type.hvc_event as usize {
            HVC_CONFIG_UPLOAD_KERNEL_IMAGE => {
                let cfg_arg;
                unsafe {
                    cfg_arg =
                        mem::transmute::<[u8; CONFIG_ARG_SIZE], CfgArg>(buf[0..CONFIG_ARG_SIZE].try_into().unwrap());
                }
                let fd_event = generate_hvc_mode(IOCTL_SYS, IOCTL_SYS_GET_KERNEL_IMG_NAME);

                #[repr(C)]
                struct NameArg {
                    vm_id: u64,
                    name_addr: *mut c_char,
                }

                let filename: [u8; 64] = [0; 64];
                let mut name_arg: NameArg = NameArg {
                    vm_id: cfg_arg.vm_id,
                    name_addr: filename.as_ptr() as *mut c_char,
                };

                unsafe {
                    let fd = open("/dev/shyper\0".as_ptr() as *const u8, O_RDWR);
                    if ioctl(fd, fd_event as c_ulonglong, &mut name_arg as *mut NameArg as uintptr_t) != 0 {
                        warn!("sig_handle_event: failed to get VM[{}] name\n", cfg_arg.vm_id);
                        close(fd);
                        return;
                    }

                    let img_name = cstr_arr_to_string(filename.as_slice());
                    if let Err(err) = copy_img_file_to_memory(cfg_arg.vm_id, img_name, fd as u32) {
                        warn!("sig_handle_event: failed to copy img file to memory: {}", err);
                        return;
                    }
                    vmm_boot(cfg_arg.vm_id as u32);
                    close(fd);
                    return;
                }
            }
            _ => return,
        },
        _ => return,
    }
}

pub fn init_daemon() {
    // IGNORE: get semaphore and file_lock
    // IGNORE: create migrate fifo file
    // IGNORE: IVC Init
    let pid = process::id();
    let mut vmid: c_int = 0;
    unsafe {
        let fd = open("/dev/shyper\0".as_ptr() as *const u8, O_RDWR);
        if fd < 0 {
            error!("open /dev/shyper failed: errcode = {}", *libc::__errno_location());
            return;
        }
        // Set process id, in case kernel module can send signal to cli
        if ioctl(fd, 0x1002, c_ulong::from(pid)) < 0 {
            error!("ioctl set pid failed: errcode = {}", *libc::__errno_location());
            close(fd);
            return;
        }
        // Get vmid
        if ioctl(fd, 0x1004, &mut vmid as *mut c_int) < 0 {
            error!("ioctl get vmid failed: errcode = {}", *libc::__errno_location());
            close(fd);
            return;
        }
        close(fd);
    }

    // Init mediated block partition
    if vmid == 0 {
        info!("VM[{}] start to init blk service\n", vmid);
        mediated_blk_init();
    }
    info!("VM[{}] daemon process init success\n", vmid);

    // TODO: The signal processing at this time is not real-time, but the signal is first written to the channel in the custom handler set in signal_hook,
    // and then captured in the user mode polling
    // Consider using signal_hook's real-time signal processing to enhance real-time performance
    let mut signals = Signals::new(&[SIGUSR1]).unwrap();
    for signal in &mut signals {
        sig_handle_event(signal);
    }
}

pub fn config_daemon(path: String) -> Result<(), String> {
    info!("Start Shyper-cli daemon configure");
    let json_str =
        fs::read_to_string(path.clone()).map_err(|err| format!("Open json file {} err: {}", path.clone(), err))?;
    let config: DaemonConfig = serde_json::from_str(&json_str).map_err(|err| format!("Parse json err: {}", err))?;
    debug!("config is {:?}", config);

    let mut disk_cnt = 0;
    let mut disks: Vec<MediatedBlkCfg> = Vec::new();
    for disk in config.mediated {
        // Add disk, and if error happens, skip it
        let result = mediated_blk_add(disk_cnt, disk.clone());
        if result.is_ok() {
            disk_cnt += 1;
            disks.push(result.unwrap());
        } else {
            warn!("Add mediated disk {} failed: {}", disk, result.err().unwrap());
        }
    }
    MED_BLK_LIST.set(disks).unwrap();

    info!("daemon configure {} mediated disk(s)", disk_cnt);
    info!("daemon configuration finished");
    Ok(())
}
