/*
 * QEMU virt / Renode Cortex-A53 memory map.  The ELF is loaded at its
 * physical address (0x4000_0000) and entered at `_start` with the MMU
 * off.  The initial stack sits at the top of the 128 MiB DRAM window.
 */

ENTRY(_start)

MEMORY
{
    RAM (rwx) : ORIGIN = 0x40000000, LENGTH = 128M
}

_estack = ORIGIN(RAM) + LENGTH(RAM);

SECTIONS
{
    . = ORIGIN(RAM);

    .text : ALIGN(16)
    {
        KEEP(*(.text.entry))
        *(.text .text.*)
        *(.rodata .rodata.*)
        . = ALIGN(8);
        _etext = .;
    } > RAM

    .data : ALIGN(16)
    {
        _sdata = .;
        *(.data .data.*)
        . = ALIGN(8);
        _edata = .;
    } > RAM

    .bss (NOLOAD) : ALIGN(16)
    {
        _sbss = .;
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(8);
        _ebss = .;
    } > RAM

    _sheap = .;
    _eheap = ORIGIN(RAM) + LENGTH(RAM) - 64K;

    /DISCARD/ :
    {
        *(.eh_frame*)
        *(.comment)
        *(.note*)
    }
}
