// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use alloc::sync::Arc;

use spin::Mutex;

use crate::config::VmEmulatedDeviceConfig;
// use crate::device::add_mediated_dev;
use crate::device::{net_features, NetDesc, NetDescData};
use crate::device::{console_features, ConsoleDesc, ConsoleDescData};
use crate::device::{BlkDesc, BLOCKIF_IOV_MAX, VirtioBlkReq};
use crate::device::{VIRTIO_BLK_F_SEG_MAX, VIRTIO_BLK_F_SIZE_MAX, VIRTIO_F_VERSION_1};
use crate::device::{BlkStat, NicStat};
use crate::kernel::mem_pages_alloc;
use crate::mm::PageFrame;

/// Represents the type of a Virtio device.
#[derive(Copy, Clone, Debug)]
pub enum VirtioDeviceType {
    None = 0,
    Net = 1,
    Block = 2,
    Console = 3,
}

/// Placeholder struct for block device configuration data.
pub struct BlkDescData {}

/// Represents different types of device descriptions.
pub enum DevDescData {
    /// Reserved block device description.
    BlkDesc(BlkDescData),
    /// Network device description.
    NetDesc(NetDescData),
    /// Console device description.
    ConsoleDesc(ConsoleDescData),
    /// No device description.
    None,
}

/// Represents various data associated with a Virtio device.
pub struct VirtDevData {
    pub activated: bool,
    pub dev_type: VirtioDeviceType,
    pub features: usize,
    pub generation: usize,
    pub int_id: usize,
    pub desc: DevDescData,
    // req: reserve; we used nfs, no need to mig blk req data
    // cache: reserve
    // stat: reserve
}

/// Represents the status of a device.
#[derive(Clone)]
pub enum DevStat {
    BlkStat(BlkStat),
    NicStat(NicStat),
    None,
}

#[derive(Clone)]
pub enum DevDesc {
    BlkDesc(BlkDesc),
    NetDesc(NetDesc),
    ConsoleDesc(ConsoleDesc),
    None,
}

#[derive(Clone)]
pub enum DevReq {
    BlkReq(VirtioBlkReq),
    None,
}

#[derive(Clone)]
pub struct VirtDev {
    inner: Arc<Mutex<VirtDevInner>>,
}

impl VirtDev {
    /// Creates a new `VirtDev` with default inner values.
    pub fn default() -> VirtDev {
        VirtDev {
            inner: Arc::new(Mutex::new(VirtDevInner::default())),
        }
    }

    /// Initializes the Virtio device with the specified parameters.
    pub fn init(&self, dev_type: VirtioDeviceType, config: &VmEmulatedDeviceConfig, mediated: bool) {
        let mut inner = self.inner.lock();
        inner.init(dev_type, config, mediated);
    }

    /// Retrieves the features supported by the Virtio device.
    pub fn features(&self) -> usize {
        let inner = self.inner.lock();
        inner.features
    }

    /// Retrieves the generation of the Virtio device.
    pub fn generation(&self) -> usize {
        let inner = self.inner.lock();
        inner.generation
    }

    /// Retrieves the device description associated with the Virtio device.
    pub fn desc(&self) -> DevDesc {
        let inner = self.inner.lock();
        inner.desc.clone()
    }

    /// Retrieves the device request associated with the Virtio device.
    pub fn req(&self) -> DevReq {
        let inner = self.inner.lock();
        inner.req.clone()
    }

    /// Retrieves the interrupt ID associated with the Virtio device.
    pub fn int_id(&self) -> usize {
        let inner = self.inner.lock();
        inner.int_id
    }

    /// Retrieves the cache associated with the Virtio device.
    pub fn cache(&self) -> usize {
        let inner = self.inner.lock();
        return inner.cache.as_ref().unwrap().pa();
    }

    /// Retrieves the status of the Virtio device.
    pub fn stat(&self) -> DevStat {
        let inner = self.inner.lock();
        inner.stat.clone()
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
        let inner = self.inner.lock();
        inner.mediated()
    }
}

/// Represents the inner data structure for `VirtDev`.
pub struct VirtDevInner {
    activated: bool,
    dev_type: VirtioDeviceType,
    features: usize,
    generation: usize,
    int_id: usize,
    desc: DevDesc,
    req: DevReq,
    cache: Option<PageFrame>,
    stat: DevStat,
}

impl VirtDevInner {
    /// Creates a new `VirtDevInner` with default values.
    pub fn default() -> VirtDevInner {
        VirtDevInner {
            activated: false,
            dev_type: VirtioDeviceType::None,
            features: 0,
            generation: 0,
            int_id: 0,
            desc: DevDesc::None,
            req: DevReq::None,
            cache: None,
            stat: DevStat::None,
        }
    }

    /// Checks if the Virtio device is mediated.
    pub fn mediated(&self) -> bool {
        match &self.req {
            DevReq::BlkReq(req) => req.mediated(),
            DevReq::None => false,
        }
    }

    /// Initializes the Virtio device with the specified parameters.
    // virtio_dev_init
    pub fn init(&mut self, dev_type: VirtioDeviceType, config: &VmEmulatedDeviceConfig, mediated: bool) {
        self.dev_type = dev_type;
        self.int_id = config.irq_id;

        match self.dev_type {
            VirtioDeviceType::Block => {
                let blk_desc = BlkDesc::default();
                blk_desc.cfg_init(config.cfg_list[1]);
                self.desc = DevDesc::BlkDesc(blk_desc);

                // TODO: blk_features_init & cache init
                self.features |= VIRTIO_BLK_F_SIZE_MAX | VIRTIO_BLK_F_SEG_MAX | VIRTIO_F_VERSION_1;

                let blk_req = VirtioBlkReq::default();
                blk_req.set_start(config.cfg_list[0]);
                blk_req.set_mediated(mediated);
                blk_req.set_size(config.cfg_list[1]);
                self.req = DevReq::BlkReq(blk_req);

                match mem_pages_alloc(BLOCKIF_IOV_MAX) {
                    Ok(page_frame) => {
                        // println!("PageFrame pa {:x}", page_frame.pa());
                        self.cache = Some(page_frame);
                        // if mediated {
                        //     // todo: change to iov ring
                        //     let cache_size = BLOCKIF_IOV_MAX * PAGE_SIZE;
                        //     add_mediated_dev(0, page_frame.pa(), cache_size);
                        // }
                    }
                    Err(_) => {
                        error!("VirtDevInner::init(): mem_pages_alloc failed");
                    }
                }

                self.stat = DevStat::BlkStat(BlkStat::default());
            }
            VirtioDeviceType::Net => {
                let net_desc = NetDesc::default();
                net_desc.cfg_init(&config.cfg_list);
                self.desc = DevDesc::NetDesc(net_desc);

                self.features |= net_features();

                match mem_pages_alloc(1) {
                    Ok(page_frame) => {
                        // println!("PageFrame pa {:x}", page_frame.pa());
                        self.cache = Some(page_frame);
                    }
                    Err(_) => {
                        error!("VirtDevInner::init(): mem_pages_alloc failed");
                    }
                }

                self.stat = DevStat::NicStat(NicStat::default());
            }
            VirtioDeviceType::Console => {
                let console_desc = ConsoleDesc::default();
                console_desc.cfg_init(config.cfg_list[0] as u16, config.cfg_list[1] as u64);
                self.desc = DevDesc::ConsoleDesc(console_desc);
                self.features |= console_features();

                match mem_pages_alloc(1) {
                    Ok(page_frame) => {
                        // println!("PageFrame pa {:x}", page_frame.pa());
                        self.cache = Some(page_frame);
                    }
                    Err(_) => {
                        error!("VirtDevInner::init(): mem_pages_alloc failed");
                    }
                }
            }
            _ => {
                panic!("ERROR: Wrong virtio device type");
            }
        }
    }
}
