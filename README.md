# Rust-Shyper: A Reliable Embedded Hypervisor Supporting VM Migration and Hypervisor Live-Update

## Introduction

**Rust-Shyper** is an embedded type-1 hypervisor built with Rust, which has both high performance and high reliability. We have proposed low overhead VM migration and hypervisor live-update mechanisms to enable Rust-Shyper to tolerate hardware faults at runtime and dynamically fix hypervisor bugs.

Rust-Shyper can offer strong isolation between VMs and provides differentiated services for mixed-criticality systems. Rust-Shyper offers differentiated services for different VMs. Memory is statically assigned using 2-stage translation; virtual interrupts are managed by hypervisor through GIC; for device models, emulated devices, virtio devices and pass-through devices are offered; and it implements vCPU scheduling for shared physical CPU cores. For real-time virtualization, we apply GIC partial pass-though (GPPT) to minimize interrupt latency in virtualized environments. For critical VMs, physical CPUs are assigned to vCPU 1-1 to guarantee the real-time performance.

## Supported Platforms

The list of supported (and work in progress) platforms is presented below:

**aarch64**
- [x] NVIDIA Jetson TX2
- [x] Raspberry Pi 4 Model B
- [ ] QEMU (still work in progress)

## Hot to Build

Simply run `make`

```bash
make <platform>
```

For example, `make tx2_release` is to build Rust-Shyper for TX2 with opimization. 

Note that please edit the MVM profile in src/config/\<plat\>_def.rs according to your requirements.

**MVM Requirements**

MVM is a privileged VM that can monitor the status of other VMs through privileged interfaces provided by the hypervisor. We implement a dedicated Linux kernel module for MVM. Through this module, MVM can make a hypercall to realize specific functions, such as VM configuration, VM migration and hypervisor live-update. Generally, there is only one MVM, and it will monopolize core 0.

The kernel module on Rpi Linux5.4.Y (for Raspberry Pi 4 Model B) and NVIDIA L4T 32.6.1 (for Jestion TX2) as MVM has been tested.

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

## References

1. Shen, Yicong, et al. "Shyper: An embedded hypervisor applying hierarchical resource isolation strategies for mixed-criticality systems." 2022 Design, Automation & Test in Europe Conference & Exhibition (DATE). IEEE, 2022.
