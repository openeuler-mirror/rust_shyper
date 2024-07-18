# Rust-Shyper

A Reliable Embedded Hypervisor Supporting VM Migration and Hypervisor Live-Update

**[English Version *README* click here](./README.md)**

## 介绍

**Rust-Shyper** 是一个使用高级语言Rust编写的面向嵌入式场景的Type-1型虚拟机监视器（Hypervisor）。其设计目标在于提高资源利用率的同时，同时保障虚拟机实时性、隔离性与可靠性的需求。为达成上述目的，首先Rust-Shyper选用Rust作为编程语言，利用语言本身的安全特性提升代码质量，从语言层面保障系统软件的可靠性。其次，为了保障虚拟机的隔离性需求，Rust-Shyper针对CPU、中断、设备、内存等公共资源实现了有效的隔离策略，保证了同一资源被不同虚拟机共享的同时，虚拟机无法越界访问不属于当前虚拟机的资源。另外，为了保障虚拟机实时性需求，Rust-Shyper实现了中断部分直通机制以及中介传递设备模型，有效缩减虚拟化对实时性能的影响。最后，为了进一步保障监视器可靠性，本项目实现了虚拟机迁移（VM migration）以及监视器动态升级（Hypervisor Live-update）两种热更新机制修复虚拟机监视器可能存在的代码漏洞。

Rust-Shyper是由北航计算机学院操作系统研究团队，在华为技术有限公司资助下开发完成。

## 目前支持的硬件平台

下表是目前Rust-Shyper已经支持（或正在开发中）的硬件平台：

**aarch64**
- [x] NVIDIA Jetson TX2
- [x] Raspberry Pi 4 Model B
- [x] QEMU (note that VM migration and Hypervisor Live-update is not supported on QEMU)
- [x] Firefly ROC-RK3588S-PC (note that VM migration and Hypervisor Live-update is not supported on ROC-RK3588S-PC)
- [x] QEMU (for RISCV64)

## 如何编译

编译需要的工具：
- [Rust](https://www.rust-lang.org/tools/install)
- [aarch64-none-elf的编译工具链](https://developer.arm.com/downloads/-/gnu-a)
- [cargo-binutils](https://crates.io/crates/cargo-binutils/0.3.6) (可选的)
- QEMU, or qemu-system-aarch64 (可选的)
- u-boot-tools (可选的)

只需要使用`make`工具即可

```bash
make <platform>
```

例如, `make tx2` 是编译Rust-Shyper的TX2版本。具体可查看Makefile文件。

主要注意的是，请在编译前，根据需求编辑管理虚拟机（MVM）的配置文件。该文件的路径是 src/config/\<plat\>_def.rs.

**RISCV64平台的支持**

目前Rust-Shyper已经支持QEMU上的RISCV64平台，并提供了完整的用户使用手册和相应的附件，具体可以参考[面向 RISCV64 的 Rust-Shyper 使用文档](./doc/RISC-V64_User_Guide.md)

**RK3588的支持**

目前已经支持Firefly ROC-RK3588S-PC平台，并提供了完整的用户使用手册和相应的附件，具体可以参考[Firefly ROC-RK3588S-PC平台的使用](https://bhpan.buaa.edu.cn/link/AA90DE4F5ED6F447E1A9DE59F7B1A8FF72)。

**MVM的需求**

MVM 是一个可以通过Hypervisor提供的私有特权接口来监控其他虚拟机状态的特权虚拟机，通常情况是一个Linux。我们为MVM实现了一个单独的Linux内核模块。通过改内核模块，MVM可以发起Hypercall来实现诸如虚拟机配置、虚拟机迁移、Hypervisor动态升级等功能。

通常情况下，MVM仅允许存在一个，且MVM会独占0号核心。

该内核模块在如下系统作为MVM时，经测试可以正常运行：NVIDIA L4T 32.6.1 (for Jestion TX2), Linux 4.9.140/5.10.160 (for QEMU), Linux 5.10.160 (for Firefly ROC-RK3588S-PC).

## 如何启动客户虚拟机（Guest VM）

由boot-loader（如u-boot等）加载并启动Rust-Shyper镜像。Rust-Shyper完成初始化后，会自动启动MVM。

登录到MVM中，以QEMU为例，按照如下步骤，就可以配置并启动客户虚拟机了。

**Step 1**: 安装内核模块

```bash
insmod tools/shyper.ko
```

**Step 2**: 启动shyper-cli守护进程

注：shyper-cli是Rust-Shyper配套的一个简单的命令行工具，以二进制的形式提供在tools目录下，其编译的目标平台为aarch64。

> `cli`目录下是Rust版本的 cli 工具源码，您可以用`cli`目录下的源码在您所在的平台上编译，得到`shyper-cli`可执行文件，然后用其代替 `tools/shyper-cli` 进行后续的虚拟机管理操作。

```bash
sudo tools/shyper system daemon [mediated-cfg.json] &
```

mediated-cfg.json用于配置其他guest VM的virtio中介磁盘（如果是物理平台可以外接磁盘设备，如/dev/sda2、/dev/nvme0n1p2）。示例如下：

```
{
    "mediated": [
        "~/vm0.img"
    ]
}
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
                "base_pa": "0x8040000",
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

## Riscv仓库说明

* `image/Image_vanilla` 所指向的镜像其实是 `riscv` 架构下的 `Linux 5.12.1` 版本内核。
* `vm0.img` 其实是装载了 `busybox` 的 `rootfs` 文件系统。

## 发表文献

1. C. Mo, L. Wang, S. Li, K. Hu, B. Jiang. Rust-Shyper: A reliable embedded hypervisor supporting VM migration and hypervisor live-update, Journal of Systems Architecture (2023), doi: https://doi.org/10.1016/j.sysarc.2023.102948.
2. Li, S., Wang, L., Hu, K., Mo, C., Jiang, B. (2022). VM Migration and Live-Update for Reliable Embedded Hypervisor. In: Dong, W., Talpin, JP. (eds) Dependable Software Engineering. Theories, Tools, and Applications. SETTA 2022. Lecture Notes in Computer Science, vol 13649. Springer, Cham. https://doi.org/10.1007/978-3-031-21213-0_4
3. Y. Shen, L. Wang, Y. Liang, S. Li and B. Jiang, "Shyper: An embedded hypervisor applying hierarchical resource isolation strategies for mixed-criticality systems," 2022 Design, Automation & Test in Europe Conference & Exhibition (DATE), Antwerp, Belgium, 2022, pp. 1287-1292, doi: 10.23919/DATE54114.2022.9774664.

了解Rust-Shyper参见以下slides [基于Rust的嵌入式虚拟机监视器及热更新技术](./doc/%E5%9F%BA%E4%BA%8ERust%E7%9A%84%E5%B5%8C%E5%85%A5%E5%BC%8F%E8%99%9A%E6%8B%9F%E6%9C%BA%E7%9B%91%E8%A7%86%E5%99%A8%E5%8F%8A%E7%83%AD%E6%9B%B4%E6%96%B0%E6%8A%80%E6%9C%AF%EF%BC%88%E7%8E%8B%E9%9B%B7%EF%BC%89.pdf)

我们还有一个可以与Rust-Shyper配合使用的Unikernel，叫做[Unishyper](https://gitee.com/unishyper/unishyper)。

#### 关于我们

Rust-Shyper的开发者来自北京航空航天大学计算机学院操作系统研究团队。如果有什么问题，请您通过电子邮件联系我们。
- **王雷**：[个人主页](https://scse.buaa.edu.cn/info/1078/8384.htm) wanglei@buaa.edu.cn
- **姜博**：[个人主页](http://jiangbo.buaa.edu.cn) jiangbo@buaa.edu.cn
- 李思然：ohmrlsr@buaa.edu.cn
- 胡柯洋：hky1999@buaa.edu.cn
- 莫策：moce4917@buaa.edu.cn
- 陈伟明：cwm0327@buaa.edu.cn
- 崔宇洋：cuiyy369@buaa.edu.cn
- 王宇康：wangyukang@buaa.edu.cn
- 王廉杰：akari@buaa.edu.cn

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
