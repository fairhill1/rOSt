# Running rOSt with VirtIO GPU

## Build
```bash
cargo build --release --target aarch64-unknown-uefi --bin uefi_boot
```

## Run with VirtIO GPU (Hardware Cursor Support)
```bash
cp target/aarch64-unknown-uefi/release/uefi_boot.efi uefi_disk/EFI/BOOT/BOOTAA64.EFI && \
qemu-system-aarch64 \
  -nodefaults \
  -M virt \
  -cpu cortex-a57 \
  -m 1G \
  -bios /opt/homebrew/share/qemu/edk2-aarch64-code.fd \
  -device virtio-gpu-pci \
  -display cocoa \
  -device virtio-keyboard-pci \
  -device virtio-mouse-pci \
  -drive file=test_disk.img,if=none,format=raw,id=hd0 \
  -device virtio-blk-pci,drive=hd0,disable-legacy=on,disable-modern=off \
  -drive format=raw,file=fat:rw:uefi_disk \
  -serial stdio
```

## Important Notes

- **Use `virtio-gpu-pci`** - Full VirtIO GPU with hardware cursor support
- **NOT `ramfb`** - That's just a simple UEFI framebuffer without VirtIO protocol
- Click on the QEMU graphical window (not terminal) for input
- Device order matters: test_disk.img must be before uefi_disk for persistence

## First Time Setup

Create the persistent disk (only needed once):
```bash
qemu-img create -f raw test_disk.img 10M
```
