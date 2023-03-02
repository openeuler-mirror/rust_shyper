# Path
DISK = vm0.img

# Compile
ARCH ?= aarch64
BUILD_STD = core,alloc

# Toolchain
TOOLCHAIN=aarch64-none-elf
QEMU = qemu-system-aarch64
GDB = ${TOOLCHAIN}-gdb
OBJDUMP = ${TOOLCHAIN}-objdump

IMAGE=rust_shyper

qemu_debug:
	cargo build -Z build-std=${BUILD_STD} --target aarch64-qemu.json --features qemu
	${TOOLCHAIN}-objcopy target/aarch64-qemu/debug/${IMAGE} -O binary target/aarch64-qemu/debug/${IMAGE}.bin
	${OBJDUMP} --demangle -d target/aarch64-qemu/debug/${IMAGE} > target/aarch64-qemu/debug/t.txt

qemu_release:
	cargo build -Z build-std=${BUILD_STD} --target aarch64-qemu.json --features qemu --release
	${TOOLCHAIN}-objcopy target/aarch64-qemu/release/${IMAGE} -O binary target/aarch64-qemu/release/${IMAGE}.bin
	${OBJDUMP} --demangle -d target/aarch64-qemu/release/${IMAGE} > target/aarch64-qemu/release/t.txt

tx2:
	cargo build -Z build-std=${BUILD_STD} --target aarch64-tx2.json --features tx2
	bash upload
	${OBJDUMP} --demangle -d target/aarch64-tx2/debug/${IMAGE} > target/aarch64-tx2/debug/t.txt

tx2_release:
	cargo build -Z build-std=${BUILD_STD} --target aarch64-tx2.json --features tx2 --release
	bash upload_release
	${OBJDUMP} --demangle -d target/aarch64-tx2/release/${IMAGE} > target/aarch64-tx2/release/t.txt

tx2_ramdisk:
	cargo build -Z build-std=${BUILD_STD} --target aarch64-tx2.json --features "tx2 ramdisk" --release
	bash upload_release
	${OBJDUMP} --demangle -d target/aarch64-tx2/release/${IMAGE} > target/aarch64-tx2/release/t.txt

tx2_update:
	cargo build -Z build-std=${BUILD_STD} --target aarch64-tx2-update.json --features "tx2 update" --release
	bash upload_update
	${OBJDUMP} --demangle -d target/aarch64-tx2-update/release/${IMAGE} > target/aarch64-tx2-update/release/update.txt

pi4_release:
	cargo build -Z build-std=${BUILD_STD} --target aarch64-pi4.json --features pi4 --release
	bash pi4_upload_release
	${OBJDUMP} --demangle -d target/aarch64-pi4/release/${IMAGE} > target/aarch64-pi4/release/t.txt


QEMU_COMMON_OPTIONS = -machine virt,virtualization=on,gic-version=2\
	-m 8g -cpu cortex-a57 -smp 4 -display none -global virtio-mmio.force-legacy=false

QEMU_SERIAL_OPTIONS = -serial stdio #\
	-serial telnet:localhost:12345,server

QEMU_NETWORK_OPTIONS = -netdev user,id=n0,hostfwd=tcp::5555-:22 -device virtio-net-device,bus=virtio-mmio-bus.24,netdev=n0

QEMU_DISK_OPTIONS = -drive file=${DISK},if=none,format=raw,id=x0 -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.25

run:
	${QEMU} ${QEMU_COMMON_OPTIONS} ${QEMU_SERIAL_OPTIONS} ${QEMU_NETWORK_OPTIONS} ${QEMU_DISK_OPTIONS} \
		-kernel target/aarch64-qemu/debug/${IMAGE}.bin

run_release:
	${QEMU} ${QEMU_COMMON_OPTIONS} ${QEMU_SERIAL_OPTIONS} ${QEMU_NETWORK_OPTIONS} ${QEMU_DISK_OPTIONS} \
		-kernel target/aarch64-qemu/release/${IMAGE}.bin

debug:
	${QEMU} ${QEMU_COMMON_OPTIONS} ${QEMU_SERIAL_OPTIONS} ${QEMU_NETWORK_OPTIONS} ${QEMU_DISK_OPTIONS} \
		-kernel target/aarch64-qemu/debug/${IMAGE}.bin \
		-s -S

.PHONY: gdb clean

gdb:
	${GDB} -x gdb/aarch64.gdb

clean:
	cargo clean
