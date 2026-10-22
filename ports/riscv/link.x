/* Linker script for RISC-V (QEMU virt / Renode riscv_virt).
 *
 * Both simulators load an ELF `-kernel` image at its program-header
 * addresses and jump to `_start`.  The kernel therefore has to be
 * linked directly into DRAM at 0x8000_0000; linking it at the UART
 * MMIO address (0x1000_0000) can never boot.
 */

MEMORY
{
    RAM (rwx) : ORIGIN = 0x8000_0000, LENGTH = 128M
}

_stack_size = 64K;
_heap_size  = 1M;

ENTRY(_start)

SECTIONS
{
    .text : ALIGN(16) {
        *(.text.entry)
        *(.text .text.*)
        *(.rodata .rodata.*)
        _etext = .;
    } > RAM

    .data : ALIGN(16) {
        _sdata = .;
        *(.sdata .sdata.*)
        *(.data .data.*)
        _edata = .;
    } > RAM

    .bss (NOLOAD) : ALIGN(16) {
        _sbss = .;
        *(.sbss .sbss.*)
        *(.bss .bss.*)
        *(COMMON)
        _ebss = .;
    } > RAM

    .heap (NOLOAD) : ALIGN(16) {
        _sheap = .;
        . += _heap_size;
        _eheap = .;
    } > RAM

    .stack (NOLOAD) : {
        . = ALIGN(16);
        _sstack = .;
        . = . + _stack_size;
        _estack = .;
    } > RAM

    /DISCARD/ :
    {
        *(.eh_frame*)
        *(.comment)
        *(.note*)
    }
}
