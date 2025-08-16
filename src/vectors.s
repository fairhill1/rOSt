.section .text.vectors
.align 11
.global vectors

vectors:
    // Current EL with SP0
    .align 7
    b       sync_invalid_el1t
    .align 7
    b       irq_invalid_el1t
    .align 7
    b       fiq_invalid_el1t
    .align 7
    b       serror_invalid_el1t

    // Current EL with SPx
    .align 7
    b       sync_handler
    .align 7
    b       irq_handler
    .align 7
    b       fiq_handler
    .align 7
    b       serror_handler

    // Lower EL using AArch64
    .align 7
    b       sync_invalid_el0_64
    .align 7
    b       irq_invalid_el0_64
    .align 7
    b       fiq_invalid_el0_64
    .align 7
    b       serror_invalid_el0_64

    // Lower EL using AArch32
    .align 7
    b       sync_invalid_el0_32
    .align 7
    b       irq_invalid_el0_32
    .align 7
    b       fiq_invalid_el0_32
    .align 7
    b       serror_invalid_el0_32

.macro ventry label
    .align 7
    b       \label
.endm

.macro handle_invalid_entry type
    mov     x0, #\type
    mrs     x1, esr_el1
    mrs     x2, elr_el1
    b       show_invalid_entry_message
.endm

.macro save_context
    sub     sp, sp, #256
    stp     x0, x1, [sp, #16 * 0]
    stp     x2, x3, [sp, #16 * 1]
    stp     x4, x5, [sp, #16 * 2]
    stp     x6, x7, [sp, #16 * 3]
    stp     x8, x9, [sp, #16 * 4]
    stp     x10, x11, [sp, #16 * 5]
    stp     x12, x13, [sp, #16 * 6]
    stp     x14, x15, [sp, #16 * 7]
    stp     x16, x17, [sp, #16 * 8]
    stp     x18, x19, [sp, #16 * 9]
    stp     x20, x21, [sp, #16 * 10]
    stp     x22, x23, [sp, #16 * 11]
    stp     x24, x25, [sp, #16 * 12]
    stp     x26, x27, [sp, #16 * 13]
    stp     x28, x29, [sp, #16 * 14]
    str     x30, [sp, #16 * 15]
.endm

.macro restore_context
    ldr     x30, [sp, #16 * 15]
    ldp     x28, x29, [sp, #16 * 14]
    ldp     x26, x27, [sp, #16 * 13]
    ldp     x24, x25, [sp, #16 * 12]
    ldp     x22, x23, [sp, #16 * 11]
    ldp     x20, x21, [sp, #16 * 10]
    ldp     x18, x19, [sp, #16 * 9]
    ldp     x16, x17, [sp, #16 * 8]
    ldp     x14, x15, [sp, #16 * 7]
    ldp     x12, x13, [sp, #16 * 6]
    ldp     x10, x11, [sp, #16 * 5]
    ldp     x8, x9, [sp, #16 * 4]
    ldp     x6, x7, [sp, #16 * 3]
    ldp     x4, x5, [sp, #16 * 2]
    ldp     x2, x3, [sp, #16 * 1]
    ldp     x0, x1, [sp, #16 * 0]
    add     sp, sp, #256
.endm

sync_invalid_el1t:
    handle_invalid_entry 0

irq_invalid_el1t:
    handle_invalid_entry 1

fiq_invalid_el1t:
    handle_invalid_entry 2

serror_invalid_el1t:
    handle_invalid_entry 3

sync_handler:
    save_context
    mov     x0, sp
    bl      sync_exception_handler
    restore_context
    eret

irq_handler:
    save_context
    mov     x0, sp
    bl      irq_exception_handler
    restore_context
    eret

fiq_handler:
    save_context
    mov     x0, sp
    bl      fiq_exception_handler
    restore_context
    eret

serror_handler:
    save_context
    mov     x0, sp
    bl      serror_exception_handler
    restore_context
    eret

sync_invalid_el0_64:
    handle_invalid_entry 4

irq_invalid_el0_64:
    handle_invalid_entry 5

fiq_invalid_el0_64:
    handle_invalid_entry 6

serror_invalid_el0_64:
    handle_invalid_entry 7

sync_invalid_el0_32:
    handle_invalid_entry 8

irq_invalid_el0_32:
    handle_invalid_entry 9

fiq_invalid_el0_32:
    handle_invalid_entry 10

serror_invalid_el0_32:
    handle_invalid_entry 11

show_invalid_entry_message:
    b       show_invalid_entry_message

.global set_vbar_el1
set_vbar_el1:
    msr     vbar_el1, x0
    ret