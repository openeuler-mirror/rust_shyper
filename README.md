# Rust-Shyper

A Reliable Embedded Hypervisor Supporting VM Migration and Hypervisor Live-Update

**中文版说明[*README*](./README.ch.md)**

## Introduction

**Rust-Shyper** is an embedded type-1 hypervisor built with Rust, which has both high performance and high reliability. Designed for embedded platform, Rust-Shyper provides a small TCB and ensures isolation between different VMs. Furthermore, it can offer differentiated services for VMs such that the real-time performance of critical VMs are guaranteed. We have proposed low overhead VM migration and hypervisor live-update mechanisms to enable Rust-Shyper to tolerate hardware faults at runtime and dynamically fix hypervisor bugs. 

Rust-Shyper was developed by the OS research team of the School of Computer Science and Engineering(SCSE), Beihang University(BUAA) with a funding of Huawei Technologies Co.,Ltd.

## Supported Platforms

The list of supported (and work in progress) platforms is presented below:

**aarch64**
- [x] NVIDIA Jetson TX2
- [x] Raspberry Pi 4 Model B
- [x] QEMU (note that VM migration and Hypervisor Live-update is not supported on QEMU)

## How to Build

Tools for compiling: please install:
- [Rust](https://www.rust-lang.org/tools/install)
- [aarch64-none-elf toolchain](https://developer.arm.com/downloads/-/gnu-a)
- [cargo-binutils](https://crates.io/crates/cargo-binutils/0.3.6) (optional)
- QEMU, or qemu-system-aarch64 (optional)
- u-boot-tools (optional)

Simply run `make`

```bash
make <platform>
```

For example, `make tx2` is to build Rust-Shyper for TX2. 

Note that please edit the MVM profile in src/config/\<plat\>_def.rs according to your requirements.

**MVM Requirements**

MVM is a privileged VM that can monitor the status of other VMs through privileged interfaces provided by the hypervisor. We implement a dedicated Linux kernel module for MVM. Through this module, MVM can make a hypercall to realize specific functions, such as VM configuration, VM migration and hypervisor live-update. Generally, there is only one MVM, and it will monopolize core 0.

The kernel module on NVIDIA L4T 32.6.1 (for Jestion TX2) as MVM has been tested.

## How to Run Guest VM

When starting Rust-Shyper, the MVM will boot automatically. Logging on to the MVM (a Linux priviledged VM), then can we configure and start the Guest VMs.

**Step 1**: Install the kernel module

```bash
insmod tools/shyper.ko
```

**Step 2**: Start the shyper-cli daemon

```bash
# mediated-cfg.json is optional
sudo tools/shyper system daemon [mediated-cfg.json] &
```

**Step 3**: Configure a VM through profile

```bash
sudo tools/shyper vm config <vm-config.json>
```

**Guest VM Configuration Profile Template** is as follow:

```
{
    "name": "guest-os-1",
    "type": "VM_T_LINUX",
    "cmdline": "earlycon console=hvc0,115200n8 root=/dev/vda rw audit=0",
    "image": {
        "kernel_filename": "</path/to/kernel/image>",
        "kernel_load_ipa": "0x80080000",
        "kernel_entry_point": "0x80080000",
        "device_tree_filename": "-",
        "device_tree_load_ipa": "0x80000000",
        "ramdisk_filename": "initrd.gz",
        "ramdisk_load_ipa": "0"
    },
    "memory": {
        "region": [
            {
                "ipa_start": "0x80000000",
                "length": "0x40000000"
            }
        ]
    },
    "cpu": {
        "num": 1,
        "allocate_bitmap": "0b0100",
        "master": 2
    },
    "emulated_device": {
        "emulated_device_list": [
            {
                "name": "intc@8000000",
                "base_ipa": "0x8000000",
                "length": "0x1000",
                "irq_id": 0,
                "type": "EMU_DEVICE_T_GICD"
            },
            {
                "name": "virtio_blk@a000000",
                "base_ipa": "0xa000000",
                "length": "0x1000",
                "irq_id": 48,
                "cfg_num": 2,
                "cfg_list": [
                    0,
                    209715200
                ],
                "type": "EMU_DEVICE_T_VIRTIO_BLK_MEDIATED"
            },
            {
                "name": "virtio_net@a001000",
                "base_ipa": "0xa001000",
                "length": "0x1000",
                "irq_id": 49,
                "cfg_num": 6,
                "cfg_list": [
                    "0x74",
                    "0x56",
                    "0xaa",
                    "0x0f",
                    "0x47",
                    "0xd1"
                ],
                "type": "EMU_DEVICE_T_VIRTIO_NET"
            },
            {
                "name": "virtio_console@a002000",
                "base_ipa": "0xa002000",
                "length": "0x1000",
                "irq_id": 50,
                "cfg_num": 2,
                "cfg_list": [
                    "0",
                    "0xa002000"
                ],
                "type": "EMU_DEVICE_T_VIRTIO_CONSOLE"
            }
        ]
    },
    "passthrough_device": {
        "passthrough_device_list": [
            {
                "name": "gicv",
                "base_pa": "0x3886000",
                "base_ipa": "0x8010000",
                "length": "0x2000",
                "irq_num": 1,
                "irq_list": [
                    27
                ]
            }
        ]
    },
    "dtb_device": {
        "dtb_device_list": [
            {
                "name": "gicd",
                "type": "DTB_DEVICE_T_GICD",
                "irq_num": 0,
                "irq_list": [],
                "addr_region_ipa": "0x8000000",
                "addr_region_length": "0x1000"
            },
            {
                "name": "gicc",
                "type": "DTB_DEVICE_T_GICC",
                "irq_num": 0,
                "irq_list": [],
                "addr_region_ipa": "0x8010000",
                "addr_region_length": "0x2000"
            }
        ]
    }
}
```

**Step 4**: Boot the Guest VM

```bash
sudo tools/shyper vm boot <VMID>
```

then you can interact with the guest VM.

## Publications

1. Siran Li, Lei Wang, Keyang Hu, Ce Mo, Bo Jiang, VM Migration and Live-Update for Reliable Embedded Hypervisor. In: Dong, W., Talpin, JP. (eds) Dependable Software Engineering. Theories, Tools, and Applications. SETTA 2022. Lecture Notes in Computer Science, vol 13649. Springer, Cham. https://doi.org/10.1007/978-3-031-21213-0_4
2. Yicong Shen, Lei Wang, Yuanzhi Liang, Siran Li, Bo Jiang, "Shyper: An embedded hypervisor applying hierarchical resource isolation strategies for mixed-criticality systems," 2022 Design, Automation & Test in Europe Conference & Exhibition (DATE), Antwerp, Belgium, 2022, pp. 1287-1292, doi: 10.23919/DATE54114.2022.9774664.

For more information about Rust-Shyper, see the following slides [Rust Embedded Hypervisor with VM Migration and Live-update](./doc/%E5%9F%BA%E4%BA%8ERust%E7%9A%84%E5%B5%8C%E5%85%A5%E5%BC%8F%E8%99%9A%E6%8B%9F%E6%9C%BA%E7%9B%91%E8%A7%86%E5%99%A8%E5%8F%8A%E7%83%AD%E6%9B%B4%E6%96%B0%E6%8A%80%E6%9C%AF%EF%BC%88%E7%8E%8B%E9%9B%B7%EF%BC%89.pdf)

#### About Us

The developers of Rust-Shyper come from the OS research team of the School of Computer Science and Engineering, Beihang University. If you have any questions, please contact us via e-mail.
- Lei Wang: Professor, [Homepage](https://scse.buaa.edu.cn/info/1387/8398.htm) wanglei@buaa.edu.cn
- Bo Jiang: Associate Professor, [Homepage](http://jiangbo.buaa.edu.cn) jiangbo@buaa.edu.cn
- Siran Li: Postgraduate student, ohmrlsr@buaa.edu.cn
- Keyang Hu: Postgraduate student, hky1999@buaa.edu.cn
- Ce Mo: Postgraduate student, moce4917@buaa.edu.cn

#### Contribution

1.  Fork the repository
2.  Create Feat_xxx branch
3.  Commit your code
4.  Create Pull Request


#### Gitee Feature

1.  You can use Readme\_XXX.md to support different languages, such as Readme\_en.md, Readme\_zh.md
2.  Gitee blog [blog.gitee.com](https://blog.gitee.com)
3.  Explore open source project [https://gitee.com/explore](https://gitee.com/explore)
4.  The most valuable open source project [GVP](https://gitee.com/gvp)
5.  The manual of Gitee [https://gitee.com/help](https://gitee.com/help)
6.  The most popular members  [https://gitee.com/gitee-stars/](https://gitee.com/gitee-stars/)
