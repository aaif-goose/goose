ENTRY(_start)
MEMORY {
    FLASH : ORIGIN = 0x08000000, LENGTH = 1024K
    RAM : ORIGIN = 0x20000000, LENGTH = 256K
}
_stack_top = ORIGIN(RAM) + LENGTH(RAM);
SECTIONS {
    .vector_table : { KEEP(*(.vector_table)) } > FLASH
    .text : { *(.text .text.*) *(.rodata .rodata.*) } > FLASH
    .data : ALIGN(4) { _sdata = .; *(.data .data.*) . = ALIGN(4); _edata = .; } > RAM AT > FLASH
    _sidata = LOADADDR(.data);
    .bss (NOLOAD) : ALIGN(4) { _sbss = .; *(.bss .bss.*) *(COMMON) . = ALIGN(4); _ebss = .; } > RAM
    /DISCARD/ : { *(.ARM.exidx*) *(.ARM.extab*) }
}
