.global kernel_trampoline_to_high_half_with_tcr
kernel_trampoline_to_high_half_with_tcr:
    // x0 = kernel page table address
    // x1 = new TCR_EL1 value

    // Debug: Write 'A' to UART to show we entered the trampoline
    mov x9, #0x41  // 'A'
    movz x10, #0x0900, lsl #16
    strb w9, [x10]

    // Step 1: Switch TTBR1_EL1 to the new kernel page table
    msr ttbr1_el1, x0
    isb

    // Debug: Write 'B' to UART to show TTBR1 is set
    mov x9, #0x42  // 'B'
    strb w9, [x10]

    // Step 2: Update TCR_EL1 to enable the TTBR0/TTBR1 split
    msr tcr_el1, x1
    isb

    // Debug: Write 'C' to UART to show TCR is set
    mov x9, #0x43  // 'C'
    strb w9, [x10]

    // Step 3: Flush TLB entries (required when changing TCR/TTBR)
    tlbi vmalle1is   // Invalidate all TLB entries for EL1, inner shareable
    dsb ish
    isb

    // Debug: Write 'D' to UART to show TLB is flushed
    mov x9, #0x44  // 'D'
    strb w9, [x10]

    // Step 4: Calculate the address of continue_in_high_half
    adrp x2, continue_in_high_half
    add  x2, x2, :lo12:continue_in_high_half

    // Debug: Write 'E' to UART to show address calculated
    mov x9, #0x45  // 'E'
    strb w9, [x10]

    // Step 5: Convert to high-half address by ORing with KERNEL_BASE
    // KERNEL_BASE = 0xFFFF_FF00_0000_0000 (L0 index 510)
    // L0 index 510 = 0x1FE requires bit 39 = 0, so we use 0xFF00 not 0xFE80
    movz x3, #0xFFFF, lsl #48      // x3 = 0xFFFF_0000_0000_0000
    movz x4, #0xFF00, lsl #32      // x4 = 0x0000_FF00_0000_0000
    orr  x3, x3, x4                // x3 = 0xFFFF_FF00_0000_0000 (KERNEL_BASE)
    orr  x2, x2, x3                // x2 = low_addr | KERNEL_BASE

    // Debug: Print the jump target address in hex
    // Print newline first
    mov x9, #0x0A  // '\n'
    strb w9, [x10]
    mov x9, #0x0D  // '\r'
    strb w9, [x10]

    // Print "JMP: "
    mov x9, #0x4A  // 'J'
    strb w9, [x10]
    mov x9, #0x4D  // 'M'
    strb w9, [x10]
    mov x9, #0x50  // 'P'
    strb w9, [x10]
    mov x9, #0x3A  // ':'
    strb w9, [x10]
    mov x9, #0x20  // ' '
    strb w9, [x10]

    // Print address in x2 (16 hex digits)
    mov x11, x2
    mov x12, #16
print_hex_loop:
    ror x11, x11, #60    // Rotate to get top nibble in bottom 4 bits
    and x9, x11, #0xF
    cmp x9, #10
    blt is_digit
    add x9, x9, #0x37    // 'A' - 10
    b print_char
is_digit:
    add x9, x9, #0x30    // '0'
print_char:
    strb w9, [x10]
    sub x12, x12, #1
    cbnz x12, print_hex_loop

    // Print newline
    mov x9, #0x0A  // '\n'
    strb w9, [x10]
    mov x9, #0x0D  // '\r'
    strb w9, [x10]

    // TEST: Try to read from the high-half address before jumping
    // This will verify that the MMU translation works for data access
    ldr x5, [x2]     // Try to load a word from the high-half address

    // If we get here, the data read worked! Print "RD:OK\n"
    mov x9, #0x52  // 'R'
    strb w9, [x10]
    mov x9, #0x44  // 'D'
    strb w9, [x10]
    mov x9, #0x3A  // ':'
    strb w9, [x10]
    mov x9, #0x4F  // 'O'
    strb w9, [x10]
    mov x9, #0x4B  // 'K'
    strb w9, [x10]
    mov x9, #0x0A  // '\n'
    strb w9, [x10]
    mov x9, #0x0D  // '\r'
    strb w9, [x10]

    // Step 6: Jump to the high-half kernel
    br x2

// Continuation code - will be executed at high virtual address
.global continue_in_high_half
continue_in_high_half:
    // CRITICAL: Flush instruction cache NOW that we're in high-half
    ic iallu
    dsb sy
    isb

    // Debug: Write 'G' to UART to show we're in high-half
    mov x9, #0x47  // 'G'
    movz x10, #0x0900, lsl #16
    strb w9, [x10]

    // Debug: Write 'H' to UART before calling Rust
    mov x9, #0x48  // 'H'
    strb w9, [x10]

    // Now executing in the high-half at 0xFFFF_xxxx_xxxx_xxxx
    // We can't use bl (PC-relative) because we've moved to high-half
    // Instead, calculate the absolute high-half address and call it
    adrp x0, kernel_high_half_cleanup
    add  x0, x0, :lo12:kernel_high_half_cleanup

    // Convert to high-half address using correct KERNEL_BASE
    movz x1, #0xFFFF, lsl #48
    movz x2, #0xFF00, lsl #32
    orr  x1, x1, x2                // x1 = 0xFFFF_FF00_0000_0000
    orr  x0, x0, x1                // x0 = high-half address

    // Call the function at its high-half address
    blr x0

    // Debug: Write 'I' if we return (shouldn't happen)
    mov x9, #0x49  // 'I'
    movz x10, #0x0900, lsl #16
    strb w9, [x10]

    // Should never reach here
1:  b 1b