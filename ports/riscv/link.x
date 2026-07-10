/* Linker script for RISC-V (QEMU virt, rv32imac)
 *
 * Memory layout (QEMU virt):
 *   flash  : ORIGIN = 0x1000_0000, LENGTH = 512K (simulated ROM)
 *   ram    : ORIGIN = 0x8000_0000, LENGTH = 128M
 *
 * We link the kernel into flash (0x1000_0000) as the primary image.
 * In practice QEMU loads the ELF at 0x8000_0000 (DRAM); the flash
 * region is available as a separate storage device.
 */

MEMORY
{
    flash (rx) : ORIGIN = 0x1000_0000, LENGTH = 512K
    ram (rwx)  : ORIGIN = 0x8000_0000, LENGTH = 128M
}

_stack_size = 4K;
_heap_size  = 16K;

ENTRY(_start)

SECTIONS
{
    .text : {
        *(.text.entry)
        *(.text)
        *(.rodata)
        _etext = .;
    } > flash

    .data : {
        _sdata = .;
        *(.data)
        _edata = .;
    } > ram AT> flash

    /* Tied-to-flash sections that need loading into RAM at startup */
    .sdata : {
        _ssdata = .;
        *(.sdata)
        _esdata = .;
    } > ram AT> flash

    .bss : {
        _sbss = .;
        *(.bss)
        *(COMMON)
        _ebss = .;
    } > ram

    /* Heap and stack — allocated from RAM */
    .heap : {
        _sheap = .;
        . = ALIGN(8);
        . += _heap_size;
        _eheap = .;
    } > ram

    .stack : {
        . = ALIGN(16);
        _estack = .;
        . = . + _stack_size;
        _sstack = .;
    } > ram
}
