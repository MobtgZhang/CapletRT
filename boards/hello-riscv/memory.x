/* Memory layout for hello-riscv board */
MEMORY
{
    FLASH (rx) : ORIGIN = 0x1000_0000, LENGTH = 512K
    RAM (rwx)  : ORIGIN = 0x8000_0000, LENGTH = 128M
}

_stack_size  = 4K;
_heap_size   = 16K;
