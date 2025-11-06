#!/bin/bash
echo "=== Starting QEMU with GDB stub ==="
echo "QEMU will run normally. When it freezes, run: lldb -s debug_freeze.lldb"
echo ""
qemu-system-aarch64 \
  -nodefaults -M virt -cpu cortex-a57 -m 1G \
  -bios /opt/homebrew/share/qemu/edk2-aarch64-code.fd \
  -device virtio-gpu-pci -display cocoa \
  -device virtio-keyboard-pci -device virtio-mouse-pci \
  -netdev user,id=net0 \
  -device virtio-net-pci,netdev=net0,disable-legacy=on,disable-modern=off \
  -drive file=test_disk.img,if=none,format=raw,id=hd0 \
  -device virtio-blk-pci,drive=hd0,disable-legacy=on,disable-modern=off \
  -drive format=raw,file=fat:rw:uefi_disk \
  -serial stdio \
  -s
