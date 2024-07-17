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
    fs::{File, OpenOptions},
    io::BufReader,
    os::fd::AsRawFd,
    process,
};

use libc::{
    c_void, close, ioctl, lseek, mmap, munmap, open, MAP_ANONYMOUS, MAP_FAILED, MAP_HUGETLB, MAP_PRIVATE, O_RDONLY,
    O_RDWR, PROT_READ, PROT_WRITE, SEEK_SET,
};
use log::{error, info, warn};
use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer, Serialize,
};
use serde_json::Value;

use crate::{
    config_arg::{
        VmAddConfigArg, VmAddDtbDeviceConfigArg, VmAddEmulatedDeviceConfigArg, VmAddMemoryRegionConfigArg,
        VmAddPassthroughDeviceIrqsConfigArg, VmAddPassthroughDeviceRegionConfigArg,
        VmAddPassthroughDeviceStreamsIdsConfigArg, VmKernelImageInfo, VmLoadKernelImgFileArg,
        VmMemoryColorBudgetConfigArg, VmSetCpuConfigArg,
    },
    daemon::{
        generate_hvc_mode, HVC_CONFIG, HVC_CONFIG_ADD_VM, HVC_CONFIG_CPU, HVC_CONFIG_DELETE_VM, HVC_CONFIG_DTB_DEVICE,
        HVC_CONFIG_EMULATED_DEVICE, HVC_CONFIG_MEMORY_COLOR_BUDGET, HVC_CONFIG_MEMORY_REGION,
        HVC_CONFIG_PASSTHROUGH_DEVICE_IRQS, HVC_CONFIG_PASSTHROUGH_DEVICE_REGION,
        HVC_CONFIG_PASSTHROUGH_DEVICE_STREAMS_IDS, HVC_CONFIG_UPLOAD_DEVICE_TREE, HVC_CONFIG_UPLOAD_KERNEL_IMAGE,
    },
    ioctl_arg::{IOCTL_SYS, IOCTL_SYS_SET_KERNEL_IMG_NAME},
    util::{check_cache_address, file_size, string_to_u64, virt_to_phys_user},
};

const CACHE_MAX: usize = 2 * 1024 * 1024;
const CMDLINE_MAX_LEN: usize = 1024;

const PASSTHROUGH_DEV_MAX_NUM: usize = 128;
const EMULATED_DEV_MAX_NUM: usize = 16;
const DTB_DEV_MAX_NUM: usize = 16;
const DEV_MAX_NUM: usize = PASSTHROUGH_DEV_MAX_NUM + EMULATED_DEV_MAX_NUM + DTB_DEV_MAX_NUM;
const CFG_MAX_NUM: usize = 0x10;
const IRQ_MAX_NUM: usize = 0x40;

#[allow(non_camel_case_types)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum VmType {
    VM_T_LINUX,
    VM_T_BARE_MATEL_APP,
    VM_T_FREERTOS,
}

#[allow(non_camel_case_types)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum EmuDeviceType {
    EMU_DEVICE_T_CONSOLE,
    EMU_DEVICE_T_GICD,
    EMU_DEVICE_T_GPPT,
    EMU_DEVICE_T_VIRTIO_BLK,
    EMU_DEVICE_T_VIRTIO_NET,
    EMU_DEVICE_T_VIRTIO_CONSOLE,
    EMU_DEVICE_T_SHYPER,
    EMU_DEVICE_T_VIRTIO_BLK_MEDIATED,
    EMU_DEVICE_T_IOMMU,
    EMU_DEVICE_T_SRE,
    EMU_DEVICE_T_SGIR,
    EMU_DEVICE_T_GICR,
    EMU_DEVICE_T_META,
    EMU_DEVICE_T_PLIC,
}

#[allow(non_camel_case_types)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DtbDeviceType {
    DTB_DEVICE_T_CONSOLE,
    DTB_DEVICE_T_GICD,
    DTB_DEVICE_T_GICC,
    DTB_DEVICE_T_GICR,
    DTB_DEVICE_T_PLIC,
}

// parse hex string to u64, like: "0x8000" -> 32768
fn deserialize_hex_string<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct HexStringVisitor;

    impl<'de> Visitor<'de> for HexStringVisitor {
        type Value = u64;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a hex string")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let value = value.trim();
            if value == "0" {
                Ok(0)
            } else if value.len() <= 2 {
                Err(de::Error::custom("value is not long enough for hex string"))
            } else {
                // Remove the "0x" prefix and parse the remaining string as u64
                u64::from_str_radix(&value[2..], 16).map_err(de::Error::custom)
            }
        }
    }

    deserializer.deserialize_str(HexStringVisitor)
}

fn deserialize_binary_string<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct BinStringVisitor;

    impl<'de> Visitor<'de> for BinStringVisitor {
        type Value = u32;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a binary string, like 0b0111")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            // Remove the "0b" prefix and parse the remaining string as u64
            u32::from_str_radix(&value[2..], 2).map_err(de::Error::custom)
        }
    }

    deserializer.deserialize_str(BinStringVisitor)
}

// parse colors, like "0-13,14,15,32-63", to num array
fn deserialize_memory_colors_str_to_vec<'de, D>(deserializer: D) -> Result<Option<Vec<u64>>, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct MemColorsStrVisitor;

    impl<'de> Visitor<'de> for MemColorsStrVisitor {
        type Value = Option<Vec<u64>>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a colors description string, like \"0-13,14,15,32-63\"")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let mut vec: Vec<u64> = Vec::new();
            for color_slice in value.split(',') {
                if !color_slice.contains("-") {
                    let color = u64::from_str_radix(color_slice, 10).map_err(de::Error::custom)?;
                    vec.push(color);
                    continue;
                }

                let pos = color_slice.find("-").unwrap();
                let len = color_slice.len();
                let start = u64::from_str_radix(&color_slice[0..pos], 10).map_err(de::Error::custom)?;
                let end = u64::from_str_radix(&color_slice[pos + 1..len], 10).map_err(de::Error::custom)?;

                for i in start..end + 1 {
                    vec.push(i);
                }
            }
            Ok(Some(vec))
        }
    }

    deserializer.deserialize_str(MemColorsStrVisitor)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VmImageConfig {
    pub kernel_filename: String,
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub kernel_load_ipa: u64,
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub kernel_entry_point: u64,
    pub device_tree_filename: String,
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub device_tree_load_ipa: u64,
    pub ramdisk_filename: String,
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub ramdisk_load_ipa: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemoryRegion {
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub ipa_start: u64,
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub length: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VmMemoryConfig {
    pub region: Vec<MemoryRegion>,
    // Add default attr, in case when colors field is missing then colors will be filled with None value
    #[serde(deserialize_with = "deserialize_memory_colors_str_to_vec", default)]
    pub colors: Option<Vec<u64>>,
    pub budget: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VmCpuConfig {
    pub num: u32,
    #[serde(deserialize_with = "deserialize_binary_string")]
    pub allocate_bitmap: u32,
    pub master: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VmEmulatedDeviceConfig {
    pub emulated_device_list: Vec<EmulatedDevice>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EmulatedDevice {
    pub name: String,
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub base_ipa: u64,
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub length: u64,
    pub irq_id: usize,
    #[serde(default)]
    pub cfg_num: usize,
    #[serde(deserialize_with = "deserialize_cfg_list", default)]
    pub cfg_list: Vec<u64>,
    pub r#type: EmuDeviceType,
}

fn deserialize_cfg_list<'de, D>(deserializer: D) -> Result<Vec<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    // 解析为serdes_json定义的可变Value，以应对Value是不同值的情况
    let value: Value = Deserialize::deserialize(deserializer)?;
    let mut vec: Vec<u64> = Vec::new();

    match value {
        Value::Array(arr) => {
            for item in arr {
                if let Value::Number(n) = item {
                    if let Some(n) = n.as_u64() {
                        vec.push(n);
                    } else {
                        return Err(de::Error::custom(format!("Can't cast {} to u64", n)));
                    }
                } else if let Value::String(s) = item {
                    match string_to_u64(s) {
                        Ok(n) => vec.push(n),
                        Err(err) => {
                            return Err(de::Error::custom(err));
                        }
                    }
                } else {
                    return Err(de::Error::custom(format!("Not in num/string format: {}", item)));
                }
            }
            Ok(vec)
        }
        _ => Err(de::Error::custom("cfg_list is not array!")),
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VmPassthroughDeviceConfig {
    pub passthrough_device_list: Vec<PassthroughDevice>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PassthroughDevice {
    pub name: String,
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub base_pa: u64,
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub base_ipa: u64,
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub length: u64,
    pub smmu_id: Option<usize>,
    pub irq_num: usize,
    pub irq_list: Vec<usize>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VmDtbDeviceConfig {
    pub dtb_device_list: Vec<DtbDevice>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DtbDevice {
    pub name: String,
    pub r#type: DtbDeviceType,
    pub irq_num: usize,
    pub irq_list: Vec<usize>,
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub addr_region_ipa: u64,
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub addr_region_length: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VmConfigEntry {
    pub name: String,
    pub r#type: VmType,
    pub cmdline: String,
    pub image: VmImageConfig,
    pub memory: VmMemoryConfig,
    pub cpu: VmCpuConfig,
    pub emulated_device: VmEmulatedDeviceConfig,
    pub passthrough_device: VmPassthroughDeviceConfig,
    pub dtb_device: VmDtbDeviceConfig,
}

pub fn parse_vm_entry(json_file: String) -> Result<VmConfigEntry, String> {
    // Open the file in read-only mode with buffer.
    let file = File::open(json_file).map_err(|err| err.to_string())?;
    let reader = BufReader::new(file);

    let entry: VmConfigEntry = serde_json::from_reader(reader).map_err(|err| err.to_string())?;
    Ok(entry)
}

pub fn config_delete_vm(vm_id: u64) {
    let fd_event = generate_hvc_mode(HVC_CONFIG, HVC_CONFIG_DELETE_VM) as u64;
    let file = OpenOptions::new().read(true).write(true).open("/dev/shyper").unwrap();
    let fd = file.as_raw_fd();

    let result = unsafe { libc::ioctl(fd, fd_event, vm_id) };

    if result != 0 {
        error!("Failed to delete VM[{}] config", vm_id);
    } else {
        info!("DELETE VM [{}] config successfully", vm_id);
    }
}

fn ioctl_send_config(fd: i32, fd_event: usize, arg: *const c_void) -> Result<(), String> {
    let result = unsafe { ioctl(fd, fd_event as u64, arg) };
    if result != 0 {
        return Err(String::from("ioctl failed"));
    }
    Ok(())
}

pub fn config_vm_info(vm_cfg: VmConfigEntry, vm_id: u64, fd: i32) -> Result<(), String> {
    // 2. Add VM memory region
    let fd_event = generate_hvc_mode(HVC_CONFIG, HVC_CONFIG_MEMORY_REGION);
    for region in vm_cfg.memory.region {
        let mem_cfg_arg = VmAddMemoryRegionConfigArg {
            vmid: vm_id,
            ipa_start: region.ipa_start,
            length: region.length,
        };
        ioctl_send_config(fd, fd_event, &mem_cfg_arg as *const _ as *const c_void)
            .map_err(|_| String::from("failed to send vm_add_memory_region_config_arg"))?;
    }

    // 3. Add VM memory color and budget information
    let has_color = vm_cfg.memory.colors.is_some();
    let has_budget = vm_cfg.memory.budget.is_some();
    if has_color || has_budget {
        let fd_event = generate_hvc_mode(HVC_CONFIG, HVC_CONFIG_MEMORY_COLOR_BUDGET);
        let cfg = VmMemoryColorBudgetConfigArg {
            vmid: vm_id,
            color_num: if has_color {
                vm_cfg.memory.colors.as_ref().unwrap().len() as u64
            } else {
                0
            },
            color_array_addr: if has_color {
                vm_cfg.memory.colors.unwrap().as_ptr() as *const u64 as u64
            } else {
                0
            },
            budget: if has_budget { vm_cfg.memory.budget.unwrap() } else { 0 },
        };
        ioctl_send_config(fd, fd_event, &cfg as *const _ as *const c_void)
            .map_err(|_| String::from("failed to send vm_memory_color_budget_config_arg"))?;
    }

    // 4. Set VM CPU config
    let fd_event = generate_hvc_mode(HVC_CONFIG, HVC_CONFIG_CPU);
    let cpu_cfg_arg = VmSetCpuConfigArg {
        vmid: vm_id,
        num: vm_cfg.cpu.num,
        allocate_bitmap: vm_cfg.cpu.allocate_bitmap,
        master: vm_cfg.cpu.master,
    };
    ioctl_send_config(fd, fd_event, &cpu_cfg_arg as *const _ as *const c_void)
        .map_err(|_| String::from("failed to send vm_cpu_config_arg"))?;

    // 5. Add VM emulated device config
    let fd_event = generate_hvc_mode(HVC_CONFIG, HVC_CONFIG_EMULATED_DEVICE);
    for device in vm_cfg.emulated_device.emulated_device_list {
        let emu_cfg_arg = VmAddEmulatedDeviceConfigArg {
            vmid: vm_id,
            dev_name_addr: device.name.as_ptr() as u64,
            dev_name_length: device.name.len() as u64,
            base_ipa: device.base_ipa,
            length: device.length,
            irq_id: device.irq_id as u64,
            cfg_list_addr: device.cfg_list.as_ptr() as *const u64 as u64,
            cfg_list_length: device.cfg_list.len() as u64,
            emu_type: device.r#type as u64,
        };
        ioctl_send_config(fd, fd_event, &emu_cfg_arg as *const _ as *const c_void)
            .map_err(|_| String::from("failed to send vm_emulated_device_config_arg"))?;
    }

    // record passthrough device irqs and stream ids
    let mut passthrough_irqs: Vec<u64> = Vec::new();
    let mut passthrough_stream_ids: Vec<u64> = Vec::new();

    // 6. Add VM passthrough device region config.
    let fd_event = generate_hvc_mode(HVC_CONFIG, HVC_CONFIG_PASSTHROUGH_DEVICE_REGION);
    for device in vm_cfg.passthrough_device.passthrough_device_list {
        let passthrough_cfg_arg = VmAddPassthroughDeviceRegionConfigArg {
            vmid: vm_id,
            base_ipa: device.base_ipa,
            base_pa: device.base_pa,
            length: device.length,
        };
        ioctl_send_config(fd, fd_event, &passthrough_cfg_arg as *const _ as *const c_void)
            .map_err(|_| String::from("failed to send vm_passthrough_device_region_config_arg"))?;

        passthrough_irqs.append(&mut device.irq_list.clone().into_iter().map(|x| x as u64).collect());
        if device.smmu_id.is_some() {
            passthrough_stream_ids.push(device.smmu_id.unwrap() as u64);
        }
    }

    // 7. Add VM passthrough device irqs.
    if !passthrough_irqs.is_empty() {
        let fd_event = generate_hvc_mode(HVC_CONFIG, HVC_CONFIG_PASSTHROUGH_DEVICE_IRQS);
        let passthrough_irqs_cfg_arg = VmAddPassthroughDeviceIrqsConfigArg {
            vmid: vm_id,
            irqs_addr: passthrough_irqs.as_ptr() as *const u64 as u64,
            irqs_length: passthrough_irqs.len() as u64,
        };
        ioctl_send_config(fd, fd_event, &passthrough_irqs_cfg_arg as *const _ as *const c_void)
            .map_err(|_| String::from("failed to send vm_passthrough_device_irqs_config_arg"))?;
    }

    // 8. Add VM passthrough device streams ids
    if !passthrough_stream_ids.is_empty() {
        let fd_event = generate_hvc_mode(HVC_CONFIG, HVC_CONFIG_PASSTHROUGH_DEVICE_STREAMS_IDS);
        let passthrough_stream_ids_cfg_arg = VmAddPassthroughDeviceStreamsIdsConfigArg {
            vmid: vm_id,
            streams_ids_addr: passthrough_stream_ids.as_ptr() as *const u64 as u64,
            streams_ids_length: passthrough_stream_ids.len() as u64,
        };
        ioctl_send_config(
            fd,
            fd_event,
            &passthrough_stream_ids_cfg_arg as *const _ as *const c_void,
        )
        .map_err(|_| String::from("failed to send vm_passthrough_device_streams_ids_config_arg"))?;
    }

    // 9. Add VM dtb device config
    let fd_event = generate_hvc_mode(HVC_CONFIG, HVC_CONFIG_DTB_DEVICE);
    for dtb_device in vm_cfg.dtb_device.dtb_device_list {
        let dtb_cfg_arg = VmAddDtbDeviceConfigArg {
            vmid: vm_id,
            dev_name_addr: dtb_device.name.as_ptr() as u64,
            dev_name_length: dtb_device.name.len() as u64,
            dev_type: dtb_device.r#type as u64,
            irq_list_addr: dtb_device.irq_list.as_ptr() as *const u64 as u64,
            irq_list_length: dtb_device.irq_list.len() as u64,
            addr_region_ipa: dtb_device.addr_region_ipa,
        };
        ioctl_send_config(fd, fd_event, &dtb_cfg_arg as *const _ as *const c_void)
            .map_err(|_| String::from("failed to send vm_dtb_device_config_arg"))?;
    }

    // 10. Copy dtb_file and img_file to memory
    if !vm_cfg.image.device_tree_filename.is_empty() {
        copy_device_tree_to_memory(vm_id, vm_cfg.image.device_tree_filename.clone(), fd as u32).map_err(|_| {
            format!(
                "failed to copy device tree {} to memory",
                vm_cfg.image.device_tree_filename
            )
        })?;
    }
    if !copy_img_file_to_memory(vm_id, vm_cfg.image.kernel_filename.clone(), fd as u32).is_ok() {
        return Err(format!(
            "failed to copy kernel image {} to memory",
            vm_cfg.image.kernel_filename
        ));
    }

    // 10. Store kernel image file name in kernel module.
    let fd_event = generate_hvc_mode(IOCTL_SYS, IOCTL_SYS_SET_KERNEL_IMG_NAME);
    let mut img_arg = VmKernelImageInfo {
        vm_id,
        image_name: [0; 32],
    };
    img_arg.image_name[..vm_cfg.image.kernel_filename.len()].copy_from_slice(vm_cfg.image.kernel_filename.as_bytes());
    img_arg.image_name[vm_cfg.image.kernel_filename.len()] = 0;

    let result = unsafe { ioctl(fd, fd_event as u64, &img_arg as *const _ as *const c_void) };
    if result != 0 {
        return Err(String::from("Failed to set kernel image name"));
    }

    Ok(())
}

pub fn config_add_vm(config_json: String) -> Result<u64, String> {
    let vm_cfg = parse_vm_entry(config_json)?;
    let vm_cfg_2 = vm_cfg.clone();
    println!("Parse VM config successfully, VM name [{}]", vm_cfg.name);

    let fd = unsafe { open("/dev/shyper\0".as_ptr() as *const u8, O_RDWR) };

    if fd < 0 {
        return Err(format!("Failed to open /dev/shyper: {}", fd));
    }

    // 1. Add VM to Hypervisor
    let vm_id: u64 = 0;
    let vm_add_req = VmAddConfigArg {
        vm_name_addr: vm_cfg.name.as_ptr() as u64,
        vm_name_length: vm_cfg.name.len() as u64,
        vm_type: vm_cfg.r#type as u64,
        cmd_line_addr: vm_cfg.cmdline.as_ptr() as u64,
        cmd_line_length: vm_cfg.cmdline.len() as u64,
        kernel_load_ipa: vm_cfg.image.kernel_load_ipa,
        device_tree_load_ipa: vm_cfg.image.device_tree_load_ipa,
        ramdisk_load_ipa: vm_cfg.image.ramdisk_load_ipa,
        vm_id_addr: &vm_id as *const u64 as u64,
    };
    let fd_event = generate_hvc_mode(HVC_CONFIG, HVC_CONFIG_ADD_VM);
    ioctl_send_config(fd, fd_event, &vm_add_req as *const _ as *const c_void)
        .map_err(|_| String::from("config_add_vm failed"))?;
    info!("Send VM [{}] config successfully", vm_id);

    // deal with failure condition
    match config_vm_info(vm_cfg_2, vm_id, fd) {
        Ok(_) => {
            info!("Config VM [{}] successfully", vm_id);
            unsafe { close(fd) };
            Ok(vm_id)
        }
        Err(err) => {
            error!("Config VM [{}] failed: {}", vm_id, err);
            unsafe { close(fd) };
            config_delete_vm(vm_id);
            Err(err)
        }
    }
}

fn copy_file_to_hypervisor(vmid: u64, filename: String, shyper_fd: u32, upload_mode: usize) -> Result<(), String> {
    // Create a Cache_buffer, copy it in batches to the buffer, and then use ioctl to copy the data to the hypervisor
    let file_size = file_size(&filename)?;
    let mut copied_size: u64 = 0;
    let mut coping_size: u64;
    let cache_va: *mut c_void;
    let file_fd: i32;

    unsafe {
        file_fd = open(filename.as_ptr() as *const u8, O_RDONLY);
        cache_va = mmap(
            0 as *mut c_void,
            CACHE_MAX,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS | MAP_HUGETLB,
            0,
            0,
        );
        if cache_va == MAP_FAILED {
            close(file_fd);
            return Err(String::from("Allocate cache memory error!"));
        }
    }

    // check whether cache address is valid
    if let Err(err) = check_cache_address(cache_va, CACHE_MAX as u64) {
        warn!("Cache address is invalid");
        unsafe {
            close(file_fd);
            munmap(cache_va, CACHE_MAX);
        }
        return Err(err);
    }

    let cache_ipa = virt_to_phys_user(process::id(), cache_va as u64).map_err(|err| {
        warn!("Failed to get cache pa\n");
        unsafe {
            close(file_fd);
            munmap(cache_va, CACHE_MAX);
        }
        err
    })?;

    while copied_size < file_size {
        // set read start is previous read
        unsafe {
            if lseek(file_fd, copied_size as i64, SEEK_SET) < 0 {
                warn!("seek file {} pos {} failed\n", filename, copied_size);
                close(file_fd);
                munmap(cache_va, CACHE_MAX);
                return Err(String::from("lseek err"));
            }

            coping_size = if file_size - copied_size > CACHE_MAX as u64 {
                CACHE_MAX as u64
            } else {
                file_size
            };

            if libc::read(file_fd, cache_va, coping_size as usize) == -1 {
                warn!(
                    "read kernel image {} pos {} size {} failed\n",
                    filename, copied_size, coping_size
                );
                close(file_fd);
                munmap(cache_va, CACHE_MAX);
                return Err(format!("read file {} err", filename));
            }

            // Use HVC to copy to Hypervisor memory
            let arg = VmLoadKernelImgFileArg {
                vmid,
                img_size: file_size,
                cache_ipa,
                load_offset: copied_size,
                load_size: coping_size,
            };
            let fd_event = generate_hvc_mode(HVC_CONFIG, upload_mode);

            let ret = ioctl(shyper_fd as i32, fd_event as u64, &arg as *const _ as *const u8);
            if ret != 0 {
                warn!("Copy_file_to_hypervisor: ioctl failed");
            }
            copied_size += coping_size;
        }
    }

    unsafe {
        close(file_fd);
        if munmap(cache_va, CACHE_MAX) != 0 {
            warn!("Failed to unmap cache va {:#x}", cache_va as u64);
        }
    }

    Ok(())
}

pub fn copy_img_file_to_memory(vmid: u64, filename: String, shyper_fd: u32) -> Result<(), String> {
    copy_file_to_hypervisor(vmid, filename, shyper_fd, HVC_CONFIG_UPLOAD_KERNEL_IMAGE)
}

pub fn copy_device_tree_to_memory(vmid: u64, filename: String, shyper_fd: u32) -> Result<(), String> {
    copy_file_to_hypervisor(vmid, filename, shyper_fd, HVC_CONFIG_UPLOAD_DEVICE_TREE)
}
