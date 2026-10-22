/*
 * Linker script for the RISC-V hello board (QEMU virt / Renode
 * riscv_virt).  Both platforms load the ELF segments at their physical
 * addresses; the kernel image is linked directly into DRAM.
 */

ENTRY(_start)

MEMORY
{
    RAM (rwx) : ORIGIN = 0x80000000, LENGTH = 128M
}

/* Initial S-mode stack at the top of the 128 MiB RAM window. */
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
        *(.sdata .sdata.*)
        *(.data .data.*)
        . = ALIGN(8);
        _edata = .;
    } > RAM

    .bss (NOLOAD) : ALIGN(16)
    {
        _sbss = .;
        *(.sbss .sbss.*)
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
