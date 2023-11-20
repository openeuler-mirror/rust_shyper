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
TOOLCHAIN=aarch64-none-elf
QEMU = qemu-system-aarch64
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

# Cargo flags.
CARGO_FLAGS ?= -Z build-std=core,alloc --target ${ARCH}.json --no-default-features --features ${BOARD},${FEATURES}
ifeq (${PROFILE}, release)
CARGO_FLAGS := ${CARGO_FLAGS} --release
endif

# Make 'cc' crate in dependencies cross compiles properly.
export CROSS_COMPILE := ${TOOLCHAIN}-
export CFLAGS := -mgeneral-regs-only

CARGO_ACTION ?= build

.PHONY: build qemu tx2 pi4 tx2_update tx2_ramdisk rk3588_release gdb clean

build:
	cargo ${CARGO_ACTION} ${CARGO_FLAGS}
	bash linkimg.sh -i ${TARGET_DIR}/${RELOCATE_IMAGE} -m ${VM0_IMAGE_PATH} \
		-t ${LD} -f linkers/${ARCH}.ld -s ${TEXT_START} -o ${TARGET_DIR}/${IMAGE}
	${OBJDUMP} --demangle -d ${TARGET_DIR}/${IMAGE} > ${TARGET_DIR}/t.txt

qemu:
	$(MAKE) build BOARD=qemu TEXT_START=0x40080000 VM0_IMAGE_PATH="./image/Image_vanilla"
	${OBJCOPY} ${TARGET_DIR}/${IMAGE} -O binary ${TARGET_DIR}/${IMAGE}.bin

tx2:
	$(MAKE) build BOARD=tx2 TEXT_START=0x83000000 VM0_IMAGE_PATH="./image/L4T"
	bash upload_release

tx2_ramdisk:
	$(MAKE) build BOARD=tx2 FEATURES=ramdisk TEXT_START=0x83000000 VM0_IMAGE_PATH="./image/L4T"
	# bash upload_release

tx2_update:
	$(MAKE) build BOARD=tx2 FEATURES=update TEXT_START=0x8a000000 VM0_IMAGE_PATH="./image/L4T"
	bash upload_update
	
tx2_update_low:
	$(MAKE) build BOARD=tx2 FEATURES=update_low TEXT_START=0x83000000 VM0_IMAGE_PATH="./image/L4T"
	bash upload_update_low

pi4:
	$(MAKE) build BOARD=pi4 TEXT_START=0xf0080000 VM0_IMAGE_PATH="./image/Image_pi4_5.4.83_tlb"
	# bash pi4_upload_release

QEMU_COMMON_OPTIONS = -machine virt,virtualization=on,gic-version=$(GIC_VERSION)\
	-m 8g -cpu cortex-a57 -smp 4 -display none -global virtio-mmio.force-legacy=false\
	-kernel ${TARGET_DIR}/${IMAGE}.bin

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
