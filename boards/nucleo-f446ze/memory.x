/* STM32F446ZE memory layout (Nucleo-F446ZE) */

MEMORY
{
  FLASH (rx)  : ORIGIN = 0x08000000, LENGTH = 512K
  RAM   (rwx) : ORIGIN = 0x20000000, LENGTH = 128K
}

/* Reserve top 1K of RAM for the kernel stack */
_stack_size = 8K;

/* Heap is shared with the kernel; boards may shrink it. */
_heap_size  = 16K;