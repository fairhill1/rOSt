// ARM64 Exception Vector Table with proper context save/restore
// Each vector entry is 128 bytes (0x80), total table is 2KB

.section .text, "ax"
.balign 2048  // Exception vectors must be 2KB aligned

.global exception_vector_table
exception_vector_table:

// Current EL with SP0 (offset 0x000)
.balign 128
current_el_sp0_sync:
    b handle_exception_entry
.balign 128
current_el_sp0_irq:
    b handle_irq_entry
.balign 128
current_el_sp0_fiq:
    b handle_fiq_entry
.balign 128
current_el_sp0_serror:
    b handle_serror_entry

// Current EL with SPx (offset 0x200)
.balign 128
current_el_spx_sync:
    b handle_exception_entry
.balign 128
current_el_spx_irq:
    b handle_irq_entry
.balign 128
current_el_spx_fiq:
    b handle_fiq_entry
.balign 128
current_el_spx_serror:
    b handle_serror_entry

// Lower EL using AArch64 (offset 0x400) - USER MODE SYSCALLS
.balign 128
lower_el_aarch64_sync:
    // DEBUG: Print 'S' to show we got EL0 sync exception
    mov x10, #0x09000000
    mov x11, #'S'
    strb w11, [x10]
    b handle_el0_syscall_entry
.balign 128
lower_el_aarch64_irq:
    b handle_irq_entry
.balign 128
lower_el_aarch64_fiq:
    b handle_fiq_entry
.balign 128
lower_el_aarch64_serror:
    b handle_serror_entry

// Lower EL using AArch32 (offset 0x600) - not used
.balign 128
lower_el_aarch32_sync:
    b .
.balign 128
lower_el_aarch32_irq:
    b .
.balign 128
lower_el_aarch32_fiq:
    b .
.balign 128
lower_el_aarch32_serror:
    b .

// EL0 syscall entry point - saves all registers, calls Rust handler, restores, returns
.balign 16
handle_el0_syscall_entry:
    // DEBUG: Print 'K' to show we entered handler
    mov x10, #0x09000000
    mov x11, #'K'
    strb w11, [x10]

    // Save all general-purpose registers to stack
    sub sp, sp, #272           // 34 registers * 8 bytes = 272 bytes

    // DEBUG: Print 'L' after sub sp
    mov x10, #0x09000000
    mov x11, #'L'
    strb w11, [x10]

    stp x0, x1, [sp, #16 * 0]

    // DEBUG: Print 'M' after first stp
    mov x10, #0x09000000
    mov x11, #'M'
    strb w11, [x10]
    stp x2, x3, [sp, #16 * 1]
    stp x4, x5, [sp, #16 * 2]
    stp x6, x7, [sp, #16 * 3]
    stp x8, x9, [sp, #16 * 4]
    stp x10, x11, [sp, #16 * 5]
    stp x12, x13, [sp, #16 * 6]
    stp x14, x15, [sp, #16 * 7]
    stp x16, x17, [sp, #16 * 8]
    stp x18, x19, [sp, #16 * 9]
    stp x20, x21, [sp, #16 * 10]
    stp x22, x23, [sp, #16 * 11]
    stp x24, x25, [sp, #16 * 12]
    stp x26, x27, [sp, #16 * 13]
    stp x28, x29, [sp, #16 * 14]
    str x30, [sp, #16 * 15]

    // Save ELR_EL1 (return address) and SPSR_EL1 (saved program status)
    mrs x0, elr_el1
    mrs x1, spsr_el1
    // CRITICAL: Clear IRQ mask bit (bit 7) in SPSR so interrupts work after eret
    // When handling syscalls, SPSR has IRQ masked (we're in exception handler)
    // We must clear this bit so EL0 can receive timer interrupts
    bic x1, x1, #(1 << 7)      // Clear bit 7 (IRQ mask)
    stp x0, x1, [sp, #16 * 16]

    // DEBUG: Print 'N' before calling Rust
    mov x10, #0x09000000
    mov x11, #'N'
    strb w11, [x10]

    // DON'T switch TTBR0 here! The OS architect is correct:
    // - Kernel runs from TTBR1 (high-half 0xFFFF...)
    // - User memory accessible through TTBR0 (stays as USER_TTBR0)
    // - Only switch TTBR0 if kernel needs special user memory access
    // Removing the incorrect UEFI_TTBR0 switching that was causing hangs

    // Call Rust syscall handler
    // Pass pointer to saved context as first argument
    mov x0, sp
    bl handle_el0_syscall_rust

    // DEBUG: Print 'O' after Rust handler returns
    mov x10, #0x09000000
    mov x11, #'O'
    strb w11, [x10]

    // No need to switch TTBR0 back - it never changed!
    // TTBR0 stays as USER_TTBR0 throughout syscall handling

    // Restore ELR_EL1 and SPSR_EL1
    ldp x0, x1, [sp, #16 * 16]
    msr elr_el1, x0
    msr spsr_el1, x1

    // Restore all general-purpose registers
    ldp x0, x1, [sp, #16 * 0]
    ldp x2, x3, [sp, #16 * 1]
    ldp x4, x5, [sp, #16 * 2]
    ldp x6, x7, [sp, #16 * 3]
    ldp x8, x9, [sp, #16 * 4]
    ldp x10, x11, [sp, #16 * 5]
    ldp x12, x13, [sp, #16 * 6]
    ldp x14, x15, [sp, #16 * 7]
    ldp x16, x17, [sp, #16 * 8]
    ldp x18, x19, [sp, #16 * 9]
    ldp x20, x21, [sp, #16 * 10]
    ldp x22, x23, [sp, #16 * 11]
    ldp x24, x25, [sp, #16 * 12]
    ldp x26, x27, [sp, #16 * 13]
    ldp x28, x29, [sp, #16 * 14]
    ldr x30, [sp, #16 * 15]

    add sp, sp, #272           // Restore stack pointer

    eret                       // Return to EL0

// Return from fake EL0 exception for user thread launch
// This is called when a user thread starts for the first time
// It restores a pre-crafted ExceptionContext and erets to EL0
.balign 16
el0_syscall_entry_return:
    // DEBUG: Print 'A' to UART to show we entered trampoline
    mov x10, #0x09000000
    mov x11, #'A'
    strb w11, [x10]

    // At this point, sp points to a pre-crafted ExceptionContext
    // Save sp (we'll need it later)
    mov x20, sp

    // DEBUG: Print 'B'
    mov x11, #'B'
    strb w11, [x10]

    // Switch TTBR0 to user tables before launching into EL0
    adrp x10, USER_TTBR0
    ldr x10, [x10, #:lo12:USER_TTBR0]
    msr ttbr0_el1, x10
    dsb ish
    isb

    // DEBUG: Print 'C'
    mov x10, #0x09000000
    mov x11, #'C'
    strb w11, [x10]

    // DEBUG: Print 'D'
    mov x11, #'D'
    strb w11, [x10]

    // Restore sp
    mov sp, x20

    // DEBUG: Print 'E'
    mov x11, #'E'
    strb w11, [x10]

    // Just restore all registers and eret to EL0 user program

    // Restore all general-purpose registers from ExceptionContext
    ldp x0, x1, [sp, #16 * 0]
    ldp x2, x3, [sp, #16 * 1]
    ldp x4, x5, [sp, #16 * 2]
    ldp x6, x7, [sp, #16 * 3]
    ldp x8, x9, [sp, #16 * 4]
    ldp x10, x11, [sp, #16 * 5]
    ldp x12, x13, [sp, #16 * 6]
    ldp x14, x15, [sp, #16 * 7]
    ldp x16, x17, [sp, #16 * 8]
    ldp x18, x19, [sp, #16 * 9]
    ldp x20, x21, [sp, #16 * 10]
    ldp x22, x23, [sp, #16 * 11]
    ldp x24, x25, [sp, #16 * 12]
    ldp x26, x27, [sp, #16 * 13]
    ldp x28, x29, [sp, #16 * 14]
    ldr x30, [sp, #16 * 15]

    // Set up SP_EL0 (user stack pointer) from x29 (frame pointer) in context
    msr sp_el0, x29

    // Load ELR_EL1 and SPSR_EL1 for debug before restoring
    ldr x0, [sp, #16 * 16]      // ELR_EL1
    ldr x1, [sp, #16 * 16 + 8]  // SPSR_EL1

    // Debug: Print 'F'
    mov x2, #0x09000000
    mov x3, #'F'
    strb w3, [x2]

    // Save ELR/SPSR
    mov x2, x0              // Save ELR_EL1
    mov x3, x1              // Save SPSR_EL1

    // Restore the saved registers
    mov x0, x2              // Restore ELR_EL1
    mov x1, x3              // Restore SPSR_EL1

    msr elr_el1, x0
    msr spsr_el1, x1

    // DEBUG: Print 'G' before eret
    mov x4, #0x09000000
    mov x5, #'G'
    strb w5, [x4]

    add sp, sp, #272           // Restore stack pointer

    // DEBUG: Print 'H' right before eret
    mov x4, #0x09000000
    mov x5, #'H'
    strb w5, [x4]

    eret                       // Execute exception return to EL0 user program

// Generic exception entry (for EL1 exceptions)
.balign 16
handle_exception_entry:
    // DEBUG: Print '!' to show unexpected exception
    mov x10, #0x09000000
    mov x11, #'!'
    strb w11, [x10]
    mov x11, #'X'
    strb w11, [x10]
    b .  // Infinite loop for now

// IRQ handler entry - handles timer interrupts from both EL1 and EL0
.balign 16
handle_irq_entry:
    // Save all general-purpose registers to stack
    sub sp, sp, #272           // 34 registers * 8 bytes = 272 bytes

    stp x0, x1, [sp, #16 * 0]
    stp x2, x3, [sp, #16 * 1]
    stp x4, x5, [sp, #16 * 2]
    stp x6, x7, [sp, #16 * 3]
    stp x8, x9, [sp, #16 * 4]
    stp x10, x11, [sp, #16 * 5]
    stp x12, x13, [sp, #16 * 6]
    stp x14, x15, [sp, #16 * 7]
    stp x16, x17, [sp, #16 * 8]
    stp x18, x19, [sp, #16 * 9]
    stp x20, x21, [sp, #16 * 10]
    stp x22, x23, [sp, #16 * 11]
    stp x24, x25, [sp, #16 * 12]
    stp x26, x27, [sp, #16 * 13]
    stp x28, x29, [sp, #16 * 14]
    str x30, [sp, #16 * 15]

    // Save ELR_EL1 (return address) and SPSR_EL1 (saved program status)
    mrs x0, elr_el1
    mrs x1, spsr_el1
    stp x0, x1, [sp, #16 * 16]

    // Call Rust IRQ handler
    bl handle_irq_rust

    // Restore ELR_EL1 and SPSR_EL1
    ldp x0, x1, [sp, #16 * 16]
    msr elr_el1, x0
    msr spsr_el1, x1

    // Restore all general-purpose registers
    ldp x0, x1, [sp, #16 * 0]
    ldp x2, x3, [sp, #16 * 1]
    ldp x4, x5, [sp, #16 * 2]
    ldp x6, x7, [sp, #16 * 3]
    ldp x8, x9, [sp, #16 * 4]
    ldp x10, x11, [sp, #16 * 5]
    ldp x12, x13, [sp, #16 * 6]
    ldp x14, x15, [sp, #16 * 7]
    ldp x16, x17, [sp, #16 * 8]
    ldp x18, x19, [sp, #16 * 9]
    ldp x20, x21, [sp, #16 * 10]
    ldp x22, x23, [sp, #16 * 11]
    ldp x24, x25, [sp, #16 * 12]
    ldp x26, x27, [sp, #16 * 13]
    ldp x28, x29, [sp, #16 * 14]
    ldr x30, [sp, #16 * 15]

    add sp, sp, #272           // Restore stack pointer

    eret                       // Return from interrupt

// FIQ handler entry
.balign 16
handle_fiq_entry:
    b .  // Infinite loop for now

// SError handler entry
.balign 16
handle_serror_entry:
    b .  // Infinite loop for now

// Function to transition from EL1 to EL0
// Arguments: x0 = user entry point, x1 = user stack pointer
.balign 16
.global drop_to_el0
drop_to_el0:
    // Save the original user arguments
    mov x20, x0      // Save user entry point
    mov x21, x1      // Save user stack pointer

    // === GET PRE-PREPARED PAGE TABLE ADDRESSES ===
    // Page tables were prepared during kernel initialization
    bl get_page_table_addresses

    // Function returns: x0 = user table address, x1 = kernel table address
    // These were prepared safely during kernel init while on UEFI mappings

    // === ACTUAL MMU SWITCH - USING PRE-PREPARED TABLES ===
    // CRITICAL: Since we copied UEFI mappings, our code should be mapped in both tables

    // Switch TTBR1 to our kernel tables (TTBR1 was unused by UEFI)
    msr ttbr1_el1, x1    // Switch TTBR1 to kernel page tables

    // Enable TTBR1 in TCR (TTBR1 was previously disabled)
    mrs x2, tcr_el1        // Read current TCR
    orr x2, x2, #0x40000000   // T1SZ = 25 (48-bit addresses)
    orr x2, x2, #0x30000000   // TG1 = 4KB granule
    orr x2, x2, #0x03000000   // SH1 = Inner Shareable
    orr x2, x2, #0x00800000   // ORGN1 = Normal memory
    orr x2, x2, #0x00400000   // IRGN1 = Normal memory
    msr tcr_el1, x2        // Enable TTBR1

    // Memory barrier after TTBR1 switch
    dsb sy
    isb sy

    // NOW switch TTBR0 - our code should be accessible through TTBR1
    msr ttbr0_el1, x0    // Switch TTBR0 to user page tables

    // CRITICAL: Invalidate TLB after changing TTBR0
    // This ensures CPU uses new page tables immediately
    tlbi vmalle1         // Invalidate all TLB entries for current ASID at EL1
    dsb sy               // Ensure TLB invalidation completes
    isb                  // Synchronize context

    // === MMU SWITCH COMPLETE - ON OUR DUAL TABLES ===

    // Restore the original user arguments
    mov x0, x20      // Restore user entry point to x0
    mov x1, x21      // Restore user stack pointer to x1

    // Set up ELR_EL1 (exception link register) with user entry point
    msr elr_el1, x0

    // Set up SP_EL0 (user mode stack pointer)
    msr sp_el0, x1

    // Set up SPSR_EL1 (saved program status register) for EL0
    // We want: EL0t (exception level 0 with SP_EL0), interrupts enabled
    // SPSR_EL1 bits:
    //   [3:0] = 0000 (EL0t - EL0 with SP_EL0)
    //   [6] = 0 (FIQ not masked)
    //   [7] = 0 (IRQ not masked)
    //   [8] = 0 (SError not masked)
    //   [9] = 0 (Debug exceptions not masked)
    mov x2, #0x0
    msr spsr_el1, x2

    // Clear all general-purpose registers for security
    // (Don't leak kernel data to user space)
    mov x0, #0
    mov x1, #0
    mov x2, #0
    mov x3, #0
    mov x4, #0
    mov x5, #0
    mov x6, #0
    mov x7, #0
    mov x8, #0
    mov x9, #0
    mov x10, #0
    mov x11, #0
    mov x12, #0
    mov x13, #0
    mov x14, #0
    mov x15, #0
    mov x16, #0
    mov x17, #0
    mov x18, #0
    mov x19, #0
    mov x20, #0
    mov x21, #0
    mov x22, #0
    mov x23, #0
    mov x24, #0
    mov x25, #0
    mov x26, #0
    mov x27, #0
    mov x28, #0
    mov x29, #0
    mov x30, #0

    // Execute exception return - this switches to EL0 and jumps to ELR_EL1
    eret
