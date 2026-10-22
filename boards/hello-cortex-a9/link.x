/*
 * Cortex-A9 bring-up image for QEMU `vexpress-a9` and Renode.
 *
 * RAM starts at 0x6000_0000. The first 64 MiB are mapped by both the
 * QEMU machine and the Renode platform in this repository. Keep the
 * image inside that window so the same ELF runs on both simulators.
 */

ENTRY(_reset)

MEMORY
{
    RAM (rwx) : ORIGIN = 0x60000000, LENGTH = 64M
}

_estack = ORIGIN(RAM) + LENGTH(RAM);

SECTIONS
{
    . = ORIGIN(RAM);

    .text :
    {
        KEEP(*(.text.boot))
        *(.text .text.*)
    } > RAM

    .rodata :
    {
        *(.rodata .rodata.*)
    } > RAM

    .data :
    {
        *(.data .data.*)
    } > RAM

    .bss (NOLOAD) :
    {
        _sbss = .;
        *(.bss .bss.*)
        *(COMMON)
        _ebss = .;
    } > RAM

    /DISCARD/ :
    {
        *(.ARM.exidx*)
        *(.ARM.extab*)
        *(.comment)
        *(.note*)
        *(.eh_frame*)
    }
}
