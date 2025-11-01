# TCP/HTTP Test Instructions

## Quick Test with Local HTTP Server

### Step 1: Start HTTP Server (Terminal 1)

```bash
cd /tmp
echo '<html><body><h1>TCP WORKS!</h1><p>Your OS successfully made an HTTP request!</p></body></html>' > index.html
python3 -m http.server 8888
```

Keep this running.

### Step 2: Start QEMU with Port Forwarding (Terminal 2)

```bash
cd /Users/adne/dev/sandbox/rOSt

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
  -netdev user,id=net0,guestfwd=tcp:10.0.2.2:8888-tcp:127.0.0.1:8888 \
  -device virtio-net-pci,netdev=net0,disable-legacy=on,disable-modern=off \
  -drive file=test_disk.img,if=none,format=raw,id=hd0 \
  -device virtio-blk-pci,drive=hd0,disable-legacy=on,disable-modern=off \
  -drive format=raw,file=fat:rw:uefi_disk \
  -serial stdio
```

**Important:** Note the guest forwarding: `guestfwd=tcp:10.0.2.2:8888-tcp:127.0.0.1:8888`

This forwards connections from the guest (10.0.2.2:8888) to the host (127.0.0.1:8888)

### Step 3: Test in QEMU

1. Click on the QEMU graphical window (not the terminal)
2. Click "Terminal" from the menu bar
3. Type: `httptest`

You should see:
- "Connected!" (TCP handshake succeeded)
- The HTML content from your local server
- "SUCCESS!"

## What This Tests

- ✅ TCP 3-way handshake (SYN → SYN-ACK → ACK)
- ✅ TCP data transmission
- ✅ HTTP GET request
- ✅ Receiving HTTP response
- ✅ Full TCP/IP stack working!

## Troubleshooting

**No response?**
- Make sure HTTP server is running (check Terminal 1)
- Verify port forwarding in QEMU command
- Check firewall isn't blocking port 8888

**Connection failed?**
- The port forwarding syntax must be exact
- The HTTP server must be on port 8888
- Click in the QEMU window, not the terminal
