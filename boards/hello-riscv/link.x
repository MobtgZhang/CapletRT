/* Minimal RISC-V linker script for hello-riscv.
 * This links the kernel directly into DRAM at 0x8000_0000.
 * QEMU loads the ELF at this address.
 */

MEMORY
{
    RAM (rwx)  : ORIGIN = 0x8000_0000, LENGTH = 128M
}

_stack_size = 4K;
_heap_size  = 8K;

ENTRY(_start)

SECTIONS
{
    .text : {
        *(.text.entry)
        *(.text)
        *(.rodata)
        _etext = .;
    } > RAM

    .data : {
        _sdata = .;
        *(.data)
        _edata = .;
    } > RAM

    .sdata : {
        _ssdata = .;
        *(.sdata)
        _esdata = .;
    } > RAM

    .bss : {
        _sbss = .;
        *(.bss)
        *(COMMON)
        _ebss = .;
    } > RAM

    .heap : {
        . = ALIGN(8);
        _sheap = .;
        . += _heap_size;
        _eheap = .;
    } > RAM

    .stack : {
        . = ALIGN(16);
        _sstack = .;
        . += _stack_size;
        _estack = .;
    } > RAM
}
