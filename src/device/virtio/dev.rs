// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use spin::Mutex;

use crate::config::VmEmulatedDeviceConfig;
// use crate::device::add_mediated_dev;
use crate::device::{net_features, blk_features, NetDesc};
use crate::device::{console_features, ConsoleDesc};
use crate::device::{BlkDesc, VirtioBlkReq};

/// Represents the type of a Virtio device.
#[derive(Copy, Clone, Debug)]
pub enum VirtioDeviceType {
    None = 0,
    Net = 1,
    Block = 2,
    Console = 3,
}

pub enum DevDesc {
    BlkDesc(BlkDesc),
    NetDesc(NetDesc),
    ConsoleDesc(ConsoleDesc),
    None,
}

pub struct VirtDev {
    dev_type: VirtioDeviceType,
    int_id: usize,
    desc: DevDesc,
    features: usize,
    req: Option<VirtioBlkReq>,
    inner: Mutex<VirtDevInner>,
}

impl VirtDev {
    /// Creates a new `VirtDev` with default inner values.
    pub fn new(dev_type: VirtioDeviceType, config: &VmEmulatedDeviceConfig) -> Self {
        let (desc, features, req) = match dev_type {
            VirtioDeviceType::Block => {
                let desc = DevDesc::BlkDesc(BlkDesc::new(config.cfg_list[1]));
                let features = blk_features();
                let mut blk_req = VirtioBlkReq::default();
                blk_req.set_start(config.cfg_list[0]);
                blk_req.set_mediated(config.mediated);
                blk_req.set_size(config.cfg_list[1]);
                (desc, features, Some(blk_req))
            }
            VirtioDeviceType::Net => {
                let desc = DevDesc::NetDesc(NetDesc::new(&config.cfg_list));
                let features = net_features();
                (desc, features, None)
            }
            VirtioDeviceType::Console => {
                let desc = DevDesc::ConsoleDesc(ConsoleDesc::new(config.cfg_list[0] as u16, config.cfg_list[1] as u64));
                let features = console_features();
                (desc, features, None)
            }
            _ => {
                panic!("ERROR: Wrong virtio device type");
            }
        };
        Self {
            dev_type,
            int_id: config.irq_id,
            desc,
            features,
            req,
            inner: Mutex::new(VirtDevInner::default()),
        }
    }

    /// Retrieves the features supported by the Virtio device.
    pub fn features(&self) -> usize {
        self.features
    }

    /// Retrieves the generation of the Virtio device.
    pub fn generation(&self) -> usize {
        let inner = self.inner.lock();
        inner.generation
    }

    /// Retrieves the device description associated with the Virtio device.
    pub fn desc(&self) -> &DevDesc {
        &self.desc
    }

    /// Retrieves the device request associated with the Virtio device.
    pub fn req(&self) -> &Option<VirtioBlkReq> {
        &self.req
    }

    /// Retrieves the interrupt ID associated with the Virtio device.
    pub fn int_id(&self) -> usize {
        self.int_id
    }

    /// Checks if the Virtio device is activated.
    pub fn activated(&self) -> bool {
        let inner = self.inner.lock();
        inner.activated
    }

    /// Sets the activation status of the Virtio device.
    pub fn set_activated(&self, activated: bool) {
        let mut inner = self.inner.lock();
        inner.activated = activated;
    }

    /// Checks if the Virtio device is mediated.
    pub fn mediated(&self) -> bool {
        match self.req() {
            Some(req) => req.mediated(),
            None => false,
        }
    }
}

/// Represents the inner data structure for `VirtDev`.
pub struct VirtDevInner {
    activated: bool,
    generation: usize,
}

impl VirtDevInner {
    /// Creates a new `VirtDevInner` with default values.
    pub fn default() -> VirtDevInner {
        VirtDevInner {
            activated: false,
            generation: 0,
        }
    }
}
