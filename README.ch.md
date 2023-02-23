# Rust-Shyper

A Reliable Embedded Hypervisor Supporting VM Migration and Hypervisor Live-Update

**[English Version *README* click here](./README.md)**

## 介绍

**Rust-Shyper** 是一个使用高级语言Rust编写的面向嵌入式场景的Type-1型虚拟机监视器（Hypervisor）。其设计目标在于提高资源利用率的同时，同时保障虚拟机实时性、隔离性与可靠性的需求。为达成上述目的，首先Rust-Shyper选用Rust作为编程语言，利用语言本身的安全特性提升代码质量，从语言层面保障系统软件的可靠性。其次，为了保障虚拟机的隔离性需求，Rust-Shyper针对CPU、中断、设备、内存等公共资源实现了有效的隔离策略，保证了同一资源被不同虚拟机共享的同时，虚拟机无法越界访问不属于当前虚拟机的资源。另外，为了保障虚拟机实时性需求，Rust-Shyper实现了中断部分直通机制以及中介传递设备模型，有效缩减虚拟化对实时性能的影响。最后，为了进一步保障监视器可靠性，本文实现了虚拟机迁移（VM migration）以及监视器动态升级（Hypervisor Live-update）两种热更新机制修复虚拟机监视器可能存在的代码漏洞。

## 目前支持的硬件平台

下表是目前Rust-Shyper已经支持（或正在开发中）的硬件平台：

**aarch64**
- [x] NVIDIA Jetson TX2
- [x] Raspberry Pi 4 Model B
- [ ] QEMU (still work in progress)

## 如何编译

只需要使用`make`工具即可

```bash
make <platform>
```

例如, `make tx2_release` 是编译包含优化Rust-Shyper的TX2版本。具体可查看Makefile文件。

主要注意的是，请在编译前，根据需求编辑管理虚拟机（MVM）的配置文件。该文件的路径是 src/config/\<plat\>_def.rs.

**MVM的需求**

MVM 是一个可以通过Hypervisor提供的私有特权接口来监控其他虚拟机状态的特权虚拟机，通常情况是一个Linux。我们为MVM实现了一个单独的Linux内核模块。通过改内核模块，MVM可以发起Hypercall来实现诸如虚拟机配置、虚拟机迁移、Hypervisor动态升级等功能。

通常情况下，MVM仅允许存在一个，且MVM会独占0号核心。

该内核模块在如下系统作为MVM时，经测试可以正常运行：Rpi Linux5.4.Y (for Raspberry Pi 4 Model B) 和 NVIDIA L4T 32.6.1 (for Jestion TX2).

## 如何启动客户虚拟机（Guest VM）

由boot-loader（如u-boot等）加载并启动Rust-Shyper镜像。Rust-Shyper完成初始化后，会自动启动MVM。

登录到MVM中，按照如下步骤，就可以配置并启动客户虚拟机了。

**Step 1**: 安装内核模块

```bash
insmod tools/shyper.ko
```

**Step 2**: 启动shyper-cli守护进程

注：shyper-cli是Rust-Shyper配套的一个简单的命令行工具，以二进制的形式提供在tools目录下，其编译的目标平台为aarch64。

```bash
# mediated-cfg.json is optional
sudo tools/shyper system daemon [mediated-cfg.json] &
```

**Step 3**: 通过配置文件来配置一个客户虚拟机

```bash
sudo tools/shyper vm config <vm-config.json>
```

**客户虚拟机配置文件的模板**如下:

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

**Step 4**: 启动客户虚拟机

```bash
sudo tools/shyper vm boot <VMID>
```

然后就可以和客户虚拟机进行交互了

## 参考文献

1. Li, Siran, et al. "VM Migration and Live-Update for Reliable Embedded Hypervisor." Dependable Software Engineering. Theories, Tools, and Applications: 8th International Symposium, SETTA 2022, Beijing, China, October 27-29, 2022, Proceedings. Cham: Springer Nature Switzerland, 2022.


#### 参与贡献

1.  Fork 本仓库
2.  新建 Feat_xxx 分支
3.  提交代码
4.  新建 Pull Request


#### 特技

1.  使用 Readme\_XXX.md 来支持不同的语言，例如 Readme\_en.md, Readme\_zh.md
2.  Gitee 官方博客 [blog.gitee.com](https://blog.gitee.com)
3.  你可以 [https://gitee.com/explore](https://gitee.com/explore) 这个地址来了解 Gitee 上的优秀开源项目
4.  [GVP](https://gitee.com/gvp) 全称是 Gitee 最有价值开源项目，是综合评定出的优秀开源项目
5.  Gitee 官方提供的使用手册 [https://gitee.com/help](https://gitee.com/help)
6.  Gitee 封面人物是一档用来展示 Gitee 会员风采的栏目 [https://gitee.com/gitee-stars/](https://gitee.com/gitee-stars/)

