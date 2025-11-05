ENTRY(_start)

SECTIONS
{
    /* Start at 0 for position-independent code (relocatable) */
    . = 0x0;
    
    .text :
    {
        KEEP(*(.text.boot))
        *(.text .text.*)
    }
    
    .rodata :
    {
        *(.rodata .rodata.*)
    }
    
    .data :
    {
        *(.data .data.*)
    }

    /* Relocation sections for PIE/PIC */
    .rela.dyn :
    {
        *(.rela.dyn)
        *(.rela.text .rela.text.*)
        *(.rela.rodata .rela.rodata.*)
        *(.rela.data .rela.data.*)
    }

    .bss ALIGN(8):
    {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    }
    
    . = ALIGN(8);
    . = . + 0x10000;
    _stack_top = .;
    
    /DISCARD/ :
    {
        *(.comment)
        *(.gnu*)
        *(.note*)
        *(.eh_frame*)
    }
}