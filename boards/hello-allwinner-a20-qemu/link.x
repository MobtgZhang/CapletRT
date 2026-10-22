/*
 * Linker script for the Allwinner A20 / QEMU `vexpress-a9` hello target.
 *
 * Memory map facts for QEMU's vexpress-a9 machine:
 *   - RAM base: 0x6000_0000
 *   - RAM size: 512 MiB by default (-m), i.e. RAM spans
 *     0x6000_0000 .. 0x8000_0000.
 *
 * QEMU's `-kernel` loader places an ELF image at its program-header
 * addresses and starts execution at `e_entry`. The board resets in ARM
 * (A32) state at SVC/EL1 with the MMU off, so the image must be loaded
 * identity-mapped inside RAM.
 *
 * A previous revision placed the image at 0x6000_0000 but put the
 * initial stack at 0x6400_0000 and used a bare `*` section layout. That
 * combination produced an image whose first instruction never ran
 * correctly (QEMU printed nothing at all). This script pins the reset
 * stub to offset 0, uses an explicit RAM region, and places the stack at
 * the top of the 64 MiB low-RAM window that vexpress-a9 reliably maps
 * for bare-metal images.
 */

ENTRY(_reset)

MEMORY
{
    /* Low 64 MiB window of vexpress-a9 DRAM — always present. */
    RAM (rwx) : ORIGIN = 0x60000000, LENGTH = 64M
}

/* Initial (SVC-mode) stack, growing down from the top of low RAM.
 * 16-byte aligned per the AAPCS. */
_estack = ORIGIN(RAM) + LENGTH(RAM);

SECTIONS
{
    . = ORIGIN(RAM);

    /* Boot/reset stub first, so `_reset` lands at the image base.
     * `ENTRY(_reset)` sets e_entry, and keeping the vector code at
     * offset 0 also makes the ELF usable as a raw binary. */
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
