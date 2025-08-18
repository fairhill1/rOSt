# Rust OS for AArch64

A minimal operating system written in Rust for AArch64 architecture.

## Build

```bash
cargo build --release
```

## Run (Text Mode)

```bash
qemu-system-aarch64 -M virt -cpu cortex-a72 -nographic -kernel target/aarch64-unknown-none/release/rust_os
```

## Run (Graphics Mode with UEFI)

We now have proper UEFI support! This provides working graphics initialization:

### Step 1: UEFI firmware (already installed with QEMU)
The EDK2 firmware files are already available in your QEMU installation.

### Step 2: Run with UEFI bootloader
```bash
qemu-system-aarch64 \
    -M virt \
    -cpu cortex-a57 \
    -m 1G \
    -bios /opt/homebrew/share/qemu/edk2-aarch64-code.fd \
    -device virtio-gpu-pci \
    -device qemu-xhci \
    -device usb-tablet \
    -drive format=raw,file=fat:rw:uefi_disk \
    -serial stdio
```

### Step 3: Build and test the kernel
```bash
# Build the UEFI bootloader with kernel
cargo build --target aarch64-unknown-uefi --release --bin uefi_boot

# Copy to UEFI disk and run
cp target/aarch64-unknown-uefi/release/uefi_boot.efi uefi_disk/EFI/BOOT/BOOTAA64.EFI && \
qemu-system-aarch64 \
    -nodefaults \
    -M virt \
    -cpu cortex-a57 \
    -m 1G \
    -bios /opt/homebrew/share/qemu/edk2-aarch64-code.fd \
    -device ramfb \
    -display cocoa \
    -device qemu-xhci \
    -device usb-tablet \
    -drive format=raw,file=fat:rw:uefi_disk \
    -serial stdio \
    -fw_cfg "name=opt/org.tianocore/X-Cpuhp-Bugcheck-Override,string=yes"
```

This will boot our UEFI application which initializes graphics properly!

To exit QEMU: Press `Ctrl-A` then `X` (text mode) or close the window (graphics mode)

## Alternative (using Makefile)

```bash
make run
```