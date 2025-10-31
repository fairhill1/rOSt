TARGET := aarch64-unknown-none
KERNEL := target/$(TARGET)/release/rust_os

.PHONY: all build run clean

all: run

build:
	cargo build --release

run: build
	qemu-system-aarch64 \
		-M virt \
		-cpu cortex-a72 \
		-m 512M \
		-serial stdio \
		-device virtio-gpu-pci \
		-device virtio-keyboard-pci \
		-device virtio-mouse-pci \
		-device usb-ehci,id=ehci \
		-device usb-kbd,bus=ehci.0 \
		-device usb-mouse,bus=ehci.0 \
		-kernel $(KERNEL)

debug: build
	qemu-system-aarch64 \
		-M virt \
		-cpu cortex-a72 \
		-nographic \
		-kernel $(KERNEL) \
		-s -S

clean:
	cargo clean