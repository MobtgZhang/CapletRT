/*
 * RISC-V linker script for CapletRT industrial boards.
 *
 * The image is linked into DRAM at 0x8000_0000, matching both QEMU
 * `virt` and the Renode `riscv_virt` platform.  `.text.entry` carries
 * the M-mode `_start` stub from `caplet-riscv`.
 */

ENTRY(_start)

MEMORY
{
    RAM (rwx) : ORIGIN = 0x80000000, LENGTH = 128M
}

/* Initial M-mode stack at the top of the 128 MiB RAM window. */
_estack = ORIGIN(RAM) + LENGTH(RAM);

SECTIONS
{
    . = ORIGIN(RAM);

    .text : ALIGN(16)
    {
        KEEP(*(.text.entry))
        *(.text .text.*)
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
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
