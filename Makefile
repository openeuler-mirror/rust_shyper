# Path
# please make a rootfs image by yourself
DISK = vm0.img

# Compile
ARCH ?= aarch64
PROFILE ?= release
BOARD ?= tx2
# features, seperate with comma `,`
FEATURES ?=

# Toolchain
ifeq ($(ARCH), aarch64)
	TOOLCHAIN := aarch64-none-elf
else ifeq ($(ARCH), riscv64)
	TOOLCHAIN := riscv64-linux-gnu
else
$(error bad arch: $(ARCH))
endif

QEMU := qemu-system-$(ARCH)

GDB = ${TOOLCHAIN}-gdb
OBJDUMP = ${TOOLCHAIN}-objdump
OBJCOPY = ${TOOLCHAIN}-objcopy
LD = ${TOOLCHAIN}-ld

GIC_VERSION ?= 2

ifeq ($(GIC_VERSION),3)
	FEATURES += gicv3,
else ifneq ($(GIC_VERSION),2)
$(error Bad gic version)
endif

TEXT_START ?= 0x83000000
VM0_IMAGE_PATH ?= "./image/L4T"

RELOCATE_IMAGE=librust_shyper.a
IMAGE=rust_shyper

TARGET_DIR=target/${ARCH}/${PROFILE}

# Use target_cfg depending on ARCH
TARGET_CFG := $(CURDIR)/cfg/${ARCH}.json

# Combine board(tx2, qemu, pi4, ...) with previous features as cargo's features
CARGO_FLAGS ?= -Z build-std=core,alloc -Zbuild-std-features=compiler-builtins-mem --target ${TARGET_CFG} --no-default-features --features "${BOARD},${FEATURES}"
ifeq (${PROFILE}, release)
CARGO_FLAGS := ${CARGO_FLAGS} --release
endif

# Make 'cc' crate in dependencies cross compiles properly.
export CROSS_COMPILE := ${TOOLCHAIN}-

ifeq ($(ARCH), aarch64)
	export CFLAGS += -mgeneral-regs-only
endif

ifeq ($(ARCH), riscv64)
	export CRATE_CC_NO_DEFAULTS := true
	export CFLAGS := -ffunction-sections -fdata-sections \
		-fPIC -fno-omit-frame-pointer -mabi=lp64 -mcmodel=medany -march=rv64ima \
		-ffreestanding
endif

CARGO_ACTION ?= build

TFTP_SERVER ?= root@192.168.106.153:/tftp

UBOOT_IMAGE ?= Image$(USER)_$(ARCH)_$(BOARD)

.PHONY: build upload qemu rk3588 tx2 pi4 tx2_update tx2_ramdisk gdb clean

build:
	cargo ${CARGO_ACTION} ${CARGO_FLAGS}
	bash linkimg.sh -i ${TARGET_DIR}/${RELOCATE_IMAGE} -m ${VM0_IMAGE_PATH} \
		-t ${LD} -f linkers/${ARCH}.ld -s ${TEXT_START} -o ${TARGET_DIR}/${IMAGE}
	${OBJDUMP} --demangle -d ${TARGET_DIR}/${IMAGE} > ${TARGET_DIR}/t.txt
	${OBJCOPY} ${TARGET_DIR}/${IMAGE} -O binary ${TARGET_DIR}/${IMAGE}.bin

# TODO: fix the mkimage ARCH because it only accept "arm64" and "AArch64" for aarch64
upload: build
	@mkimage -n ${IMAGE} -A arm64 -O linux -T kernel -C none -a $(TEXT_START) -e $(TEXT_START) -d ${TARGET_DIR}/${IMAGE}.bin ${TARGET_DIR}/${UBOOT_IMAGE}
	@echo "*** Upload Image ${UBOOT_IMAGE} ***"
	@scp ${TARGET_DIR}/${UBOOT_IMAGE} ${TFTP_SERVER}/${UBOOT_IMAGE}

qemu:
	$(MAKE) build BOARD=qemu TEXT_START=0x40080000 VM0_IMAGE_PATH="./image/Image_vanilla"

rk3588:
	$(MAKE) upload BOARD=rk3588 TEXT_START=0x00480000 VM0_IMAGE_PATH="./image/Image-5.10.160"

tx2:
	$(MAKE) upload BOARD=tx2 TEXT_START=0x83000000 VM0_IMAGE_PATH="./image/L4T"

tx2_ramdisk:
	$(MAKE) upload BOARD=tx2 FEATURES=ramdisk TEXT_START=0x83000000 VM0_IMAGE_PATH="./image/L4T"

tx2_update:
	$(MAKE) upload BOARD=tx2 FEATURES=update TEXT_START=0x8a000000 VM0_IMAGE_PATH="./image/L4T"

tx2_update_low:
	$(MAKE) upload BOARD=tx2 FEATURES=update_low TEXT_START=0x83000000 VM0_IMAGE_PATH="./image/L4T"

pi4:
	$(MAKE) upload BOARD=pi4 TEXT_START=0xf0080000 VM0_IMAGE_PATH="./image/Image_pi4_5.4.83_tlb"

ifeq (${ARCH}, aarch64)
QEMU_COMMON_OPTIONS = -machine virt,virtualization=on,gic-version=$(GIC_VERSION)\
	-m 8g -cpu cortex-a57 -smp 4 -display none -global virtio-mmio.force-legacy=false\
	-kernel ${TARGET_DIR}/${IMAGE}.bin
else ifeq (${ARCH}, riscv64)
QEMU_COMMON_OPTIONS = -machine virt,virtualization=on\
	-m 8g -cpu rv64 -smp 4 -display none -global virtio-mmio.force-legacy=false\
	-kernel ${TARGET_DIR}/${IMAGE}.bin
else
$(error bad qemu arch: $(ARCH))
endif

QEMU_SERIAL_OPTIONS = -serial mon:stdio #\
	-serial telnet:localhost:12345,server

# QEMU_NETWORK_OPTIONS = -netdev tap,id=tap0,ifname=tap0,script=no,downscript=no -device virtio-net-device,bus=virtio-mmio-bus.24,netdev=tap0
#/home/cwm/c-hyper/syberx-hypervisor/build/shyper_qemuv3.bin

QEMU_NETWORK_OPTIONS = -netdev user,id=n0,hostfwd=tcp::5555-:22 -device virtio-net-device,bus=virtio-mmio-bus.24,netdev=n0

QEMU_DISK_OPTIONS = -drive file=${DISK},if=none,format=raw,id=x0 -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.25

run: qemu
	${QEMU} ${QEMU_COMMON_OPTIONS} ${QEMU_SERIAL_OPTIONS} ${QEMU_NETWORK_OPTIONS} ${QEMU_DISK_OPTIONS} \

debug: qemu
	${QEMU} ${QEMU_COMMON_OPTIONS} ${QEMU_SERIAL_OPTIONS} ${QEMU_NETWORK_OPTIONS} ${QEMU_DISK_OPTIONS} \
		-s -S

gdb:
	${GDB} -x gdb/$(ARCH).gdb

clean:
	cargo clean
