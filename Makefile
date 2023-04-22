# Path
# please make a rootfs image by yourself
DISK = vm0.img

# Compile
ARCH ?= aarch64
PROFILE ?= release
BOARD ?= tx2
# features, seperate with comma `,`
FEATURES =

# Toolchain
TOOLCHAIN=aarch64-none-elf
QEMU = qemu-system-aarch64
GDB = ${TOOLCHAIN}-gdb
OBJDUMP = ${TOOLCHAIN}-objdump
OBJCOPY = ${TOOLCHAIN}-objcopy

IMAGE=rust_shyper

<<<<<<< HEAD
TARGET_DIR=target/${ARCH}/${PROFILE}
=======
qemu_debug:
	cargo build -Z build-std=${BUILD_STD} --target aarch64-qemu.json --features qemu
	${TOOLCHAIN}-objcopy target/aarch64-qemu/debug/${IMAGE} -O binary target/aarch64-qemu/debug/${IMAGE}.bin
	${OBJDUMP} --demangle -d target/aarch64-qemu/debug/${IMAGE} > target/aarch64-qemu/debug/t.txt

qemu_gicv3_debug:
	cargo build -Z build-std=${BUILD_STD} --target aarch64-qemu.json --features "qemu gicv3"
	${TOOLCHAIN}-objcopy target/aarch64-qemu/debug/${IMAGE} -O binary target/aarch64-qemu/debug/${IMAGE}.bin
	${OBJDUMP} --demangle -d target/aarch64-qemu/debug/${IMAGE} > target/aarch64-qemu/debug/t.txt
>>>>>>> finish vgicv3 but have fdt bug

# Cargo flags.
CARGO_FLAGS ?= -Z build-std=core,alloc --target ${ARCH}.json --no-default-features --features ${BOARD},${FEATURES}
ifeq (${PROFILE}, release)
CARGO_FLAGS := ${CARGO_FLAGS} --release
endif

.PHONY: build qemu tx2 pi4 tx2_update tx2_ramdisk gdb clean

build:
	cargo build ${CARGO_FLAGS}
	${OBJDUMP} --demangle -d ${TARGET_DIR}/${IMAGE} > ${TARGET_DIR}/t.txt

qemu:
	$(MAKE) build BOARD=qemu
	${OBJCOPY} ${TARGET_DIR}/${IMAGE} -O binary ${TARGET_DIR}/${IMAGE}.bin

tx2:
	$(MAKE) build BOARD=tx2
	# bash upload_release

tx2_ramdisk:
	$(MAKE) build BOARD=tx2 FEATURES=ramdisk
	# bash upload_release

tx2_update:
	$(MAKE) build BOARD=tx2 FEATURES=update
	# bash upload_update

pi4:
	$(MAKE) build BOARD=pi4
	# bash pi4_upload_release

rk3588_release:
	cargo build -Z build-std=${BUILD_STD} --target aarch64-rk3588.json --features "rk3588 gicv3" --release
	bash rk3588_upload_release
	${OBJDUMP} --demangle -d target/aarch64-rk3588/release/${IMAGE} > target/aarch64-tx2/release/t.txt

QEMU_COMMON_OPTIONS = -machine virt,virtualization=on,gic-version=2\
	-m 8g -cpu cortex-a57 -smp 4 -display none -global virtio-mmio.force-legacy=false\
	-kernel ${TARGET_DIR}/${IMAGE}.bin

QEMU_SERIAL_OPTIONS = -serial mon:stdio #\
	-serial telnet:localhost:12345,server

QEMU_NETWORK_OPTIONS = -netdev tap,id=tap0,ifname=tap0,script=no,downscript=no -device virtio-net-device,bus=virtio-mmio-bus.24,netdev=tap0

QEMU_DISK_OPTIONS = -drive file=${DISK},if=none,format=raw,id=x0 -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.25

run: qemu
	${QEMU} ${QEMU_COMMON_OPTIONS} ${QEMU_SERIAL_OPTIONS} ${QEMU_NETWORK_OPTIONS} ${QEMU_DISK_OPTIONS} \

debug: qemu
	${QEMU} ${QEMU_COMMON_OPTIONS} ${QEMU_SERIAL_OPTIONS} ${QEMU_NETWORK_OPTIONS} ${QEMU_DISK_OPTIONS} \
		-s -S

gdb:
	${GDB} -x gdb/aarch64.gdb

clean:
	cargo clean
