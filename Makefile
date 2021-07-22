disk = /home/ohmr/work/hypervisor/disk-img

qemu_debug:
	RUSTFLAGS="-C llvm-args=-global-isel=false" \
	cargo build -Z build-std=core,alloc --target aarch64.json --features qemu
	aarch64-linux-gnu-objdump -d target/aarch64/debug/rust_hypervisor > target/aarch64/debug/t.txt

qemu_release:
	RUSTFLAGS="-C llvm-args=-global-isel=false" \
	cargo build -Z build-std=core,alloc --target aarch64.json --features qemu --release
	aarch64-linux-gnu-objdump -d target/aarch64/release/rust_hypervisor > target/aarch64/release/t.txt

tx2:
	RUSTFLAGS="-C llvm-args=-global-isel=false" \
	cargo build -Z build-std=core,alloc --target aarch64-tx2.json --features tx2
	bash upload
	aarch64-linux-gnu-objdump -d target/aarch64-tx2/debug/rust_hypervisor > target/aarch64-tx2/debug/t.txt

tx2_release:
	RUSTFLAGS="-C llvm-args=-global-isel=false" \
	cargo build -Z build-std=core,alloc --target aarch64-tx2.json --features tx2 --release
	bash upload_release
	aarch64-linux-gnu-objdump -d target/aarch64-tx2/release/rust_hypervisor > target/aarch64-tx2/release/t.txt

run:
	/usr/share/qemu/bin/qemu-system-aarch64 \
		-machine virt,virtualization=on,gic-version=2\
		-drive file=${disk}/disk.img,if=none,format=raw,id=x0 -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0 \
		-m 8g \
		-cpu cortex-a57 \
		-smp 8 \
		-kernel target/aarch64/debug/rust_hypervisor \
		-global virtio-mmio.force-legacy=false \
		-serial stdio \
		-serial tcp:127.0.0.1:12345 \
		-serial tcp:127.0.0.1:12346 \
		-display none

run_release:
	/usr/share/qemu/bin/qemu-system-aarch64 \
		-machine virt,virtualization=on,gic-version=2\
		-drive file=${disk}/disk.img,if=none,format=raw,id=x0 -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0 \
		-m 8g \
		-cpu cortex-a57 \
		-smp 8 \
		-kernel target/aarch64/release/rust_hypervisor \
		-global virtio-mmio.force-legacy=false \
		-serial stdio \
		-serial tcp:127.0.0.1:12345 \
		-serial tcp:127.0.0.1:12346 \
		-display none

debug:
	/usr/share/qemu/bin/qemu-system-aarch64 \
		-machine virt,virtualization=on,gic-version=2\
		-drive file=${disk}/disk.img,if=none,format=raw,id=x0 -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0 \
		-m 8g \
		-cpu cortex-a57 \
		-smp 8 \
		-kernel target/aarch64/debug/rust_hypervisor \
		-global virtio-mmio.force-legacy=false \
		-serial stdio \
		-serial tcp:127.0.0.1:12345 \
		-serial tcp:127.0.0.1:12346 \
		-display none \
		-s -S


gdb:
	aarch64-linux-gnu-gdb -x gdb/aarch64.gdb

clean:
	rm -rf target