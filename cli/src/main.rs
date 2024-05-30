// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

mod blk;
mod config;
mod config_arg;
mod daemon;
mod ioctl_arg;
mod sys;
mod util;
mod vmm;

use std::path::Path;

use clap::{Parser, Subcommand, ValueEnum};
use config::parse_vm_entry;
use daemon::{config_daemon, init_daemon};
use log::{error, info, warn};
use sys::{sys_reboot, sys_shutdown, sys_test, sys_update};
use vmm::{vmm_boot, vmm_getvmid, vmm_list_vm_info, vmm_reboot, vmm_remove};

use crate::config::config_add_vm;

#[derive(Parser)]
#[command(
    version,
    author,
    about,
    long_about = "CommandLine Interface for Shyper/Rust-Shyper Hypervisor"
)]
struct CLI {
    #[command(subcommand)]
    subcmd: CLISubCmd,
}

#[derive(Subcommand)]
enum CLISubCmd {
    /// system subcommand
    System {
        #[command(subcommand)]
        subcmd: SystemSubCmd,
    },
    Vm {
        #[command(subcommand)]
        subcmd: VmSubCmd,
    },
}

#[derive(Subcommand)]
enum SystemSubCmd {
    Reboot {
        /// A force flag to set.
        #[arg(short, long)]
        force: bool,
    },
    Shutdown {
        /// A force flag to set.
        #[arg(short, long)]
        force: bool,
    },
    Update {
        /// new hypervisor image
        image: String,
    },
    Test {},
    Daemon {
        /// daemon config, specifically mediated disk config
        #[arg(default_value = "cli-config.json")]
        config: String,
    },
}

#[derive(Subcommand)]
enum VmSubCmd {
    /// list the info of the vm
    List {},
    Boot {
        vmid: u32,
        /// Choose display method, currently only supported SDL2.
        #[arg(long)]
        display: Option<DisplayMode>,
    },
    Reboot {
        vmid: u32,
        /// A force flag to set.
        #[arg(short, long)]
        force: bool,
    },
    Remove {
        vmid: u32,
    },
    Getdefconfig {
        vmid: u32,
    },
    Config {
        /// vm config file, in json format
        #[arg(value_parser = parse_file)]
        config: String,
    },
    Delconfig {
        vmid: u32,
        /// A force flag to set.
        #[arg(short, long)]
        force: bool,
    },
    Getvmid {},
}

fn parse_file(file: &str) -> Result<String, String> {
    if file.is_empty() {
        return Err(String::from("CONFIG can't be empty!"));
    }

    let file_path = Path::new(file);
    if !file_path.exists() {
        // judge whether the file exists or not
        return Err(String::from(format!("File {} not exists!", file)));
    }

    Ok(String::from(file))
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum DisplayMode {
    SDL,
}

fn exec_system_cmd(subcmd: SystemSubCmd) {
    match subcmd {
        SystemSubCmd::Reboot { force } => sys_reboot(force),
        SystemSubCmd::Shutdown { force } => sys_shutdown(force),
        SystemSubCmd::Update { image } => sys_update(image),
        SystemSubCmd::Test {} => sys_test(),
        SystemSubCmd::Daemon { config } => {
            config_daemon(config).unwrap();
            init_daemon();
        }
    }
}

fn exec_vm_cmd(subcmd: VmSubCmd) {
    match subcmd {
        VmSubCmd::List {} => vmm_list_vm_info(),
        VmSubCmd::Boot { vmid, display } => vmm_boot(vmid),
        VmSubCmd::Reboot { vmid, force } => vmm_reboot(force, vmid),
        VmSubCmd::Remove { vmid } => vmm_remove(vmid),
        VmSubCmd::Getdefconfig { vmid } => todo!(),
        VmSubCmd::Config { config } => {
            if let Err(err) = config_add_vm(config) {
                error!("Add vm failed: {}", err);
            }
        }
        VmSubCmd::Delconfig { vmid, force } => todo!(),
        VmSubCmd::Getvmid {} => vmm_getvmid(),
    }
}

fn main() {
    // configure logger and set log level
    env_logger::Builder::new().filter_level(log::LevelFilter::Info).init();

    let cli = CLI::parse();
    match cli.subcmd {
        CLISubCmd::System { subcmd } => {
            exec_system_cmd(subcmd);
        }
        CLISubCmd::Vm { subcmd } => {
            exec_vm_cmd(subcmd);
        }
    }
}
