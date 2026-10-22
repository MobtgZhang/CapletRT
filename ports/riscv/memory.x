/* Memory layout for the QEMU virt / Renode riscv_virt board. */

MEMORY
{
    RAM (rwx)  : ORIGIN = 0x8000_0000, LENGTH = 128M
}

_stack_start = ORIGIN(RAM) + LENGTH(RAM);
_stack_size  = 64K;
_heap_size   = 1M;

ENTRY(_start)

REGION_ALIAS("RAM", RAM);
