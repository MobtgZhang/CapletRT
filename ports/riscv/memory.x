/* Memory layout for QEMU RISC-V virt board */

MEMORY
{
    /* QEMU virt simulates 512K flash at this address */
    FLASH (rx) : ORIGIN = 0x1000_0000, LENGTH = 512K
    /* 128 MiB DRAM */
    RAM (rwx)  : ORIGIN = 0x8000_0000, LENGTH = 128M
}

_stack_start = ORIGIN(RAM) + LENGTH(RAM);
_stack_size  = 4K;
_heap_size   = 32K;

ENTRY(_start)

REGION_ALIAS("FLASH", FLASH);
REGION_ALIAS("RAM", RAM);
