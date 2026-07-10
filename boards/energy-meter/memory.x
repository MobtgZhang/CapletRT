MEMORY
{
  FLASH (rx) : ORIGIN = 0x00000000, LENGTH = 4M
  RAM (rwx)  : ORIGIN = 0x20000000, LENGTH = 4M
}
_stack_size = 16K;
_heap_size = 128K;
