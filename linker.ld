ENTRY(_start)

SECTIONS
{
    . = 0x40080000;
    
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