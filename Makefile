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
		-nographic \
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