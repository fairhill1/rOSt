.section .text.boot
.global _start

_start:
    // Read CPU ID, stop slave cores
    mrs     x0, mpidr_el1
    and     x0, x0, #3
    cbnz    x0, halt

    // Check current exception level
    mrs     x0, CurrentEL
    and     x0, x0, #12
    cmp     x0, #8      // Check if EL2
    b.eq    el2_entry
    cmp     x0, #4      // Check if EL1
    b.eq    el1_entry
    b       halt        // EL3 or unknown

el2_entry:
    // Configure EL2 before dropping to EL1
    // Enable AArch64 in EL1
    mov     x0, #(1 << 31)      // AArch64
    orr     x0, x0, #(1 << 1)   // SWIO hardwired
    msr     hcr_el2, x0
    
    // Set up SCTLR_EL1 with safe defaults
    mov     x0, #0x0
    msr     sctlr_el1, x0
    
    // Drop from EL2 to EL1
    mov     x0, #0x3c5  // EL1h with interrupt disabled
    msr     spsr_el2, x0
    adr     x0, el1_entry
    msr     elr_el2, x0
    eret

el1_entry:
    // Set stack pointer
    ldr     x0, =_stack_top
    mov     sp, x0

    // Clear BSS
    ldr     x0, =__bss_start
    ldr     x1, =__bss_end
clear_bss:
    cmp     x0, x1
    b.ge    done_clear
    str     xzr, [x0], #8
    b       clear_bss

done_clear:
    // Jump to Rust code
    bl      kernel_main

halt:
    wfe
    b       halt