/* QEMU virt / Renode riscv_virt memory map. */
MEMORY
{
    RAM (rwx) : ORIGIN = 0x80000000, LENGTH = 128M
}

_stack_start = ORIGIN(RAM) + LENGTH(RAM);
_stack_size = 64K;
