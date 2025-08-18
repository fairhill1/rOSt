# UEFI mouse/keyboard input broken on QEMU aarch64 - known issue?

I'm writing an OS in Rust for aarch64. Graphics work fine through UEFI GOP, but input devices are completely broken:

- UEFI Pointer Protocol finds the device (`usb-tablet`) and resets successfully, but `read_state()` always returns `None` and pointer events never signal
- UEFI keyboard input via `stdin.read_key()` also never returns any keypresses
- Confirmed the USB tablet is present via QEMU monitor (`info usb` shows Device 0.0, Product USB Tablet)

Testing on macOS with:
```bash
qemu-system-aarch64 -M virt -cpu cortex-a57 -m 1G \
  -bios /opt/homebrew/share/qemu/edk2-aarch64-code.fd \
  -device virtio-gpu-pci -device qemu-xhci -device usb-tablet \
  -drive format=raw,file=fat:rw:uefi_disk -serial stdio
```

Is this a known QEMU/EDK2 limitation for aarch64? Works on x86_64? Any workarounds besides implementing raw hardware drivers?