/* Memory layout for the QEMU MPS2-AN385/AN386 (Cortex-M3 / Cortex-M4).
 *
 * The MPS2 AN385/AN386 FPGA images use a unified SRAM-backed code region
 * (0x00000000 to 0x003FFFFF) for code, and 0x20000000 to 0x203FFFFF
 * for data. The on-board flash (the original QEMU "ZBT SSRAM1") is
 * modeled as 4 MiB at 0x00000000.
 *
 * 4 MiB code (FLASH / ZBT SSRAM1), 4 MiB data (RAM / ZBT SSRAM2 + SSRAM3).
 */

MEMORY
{
  FLASH (rx) : ORIGIN = 0x00000000, LENGTH = 4M
  RAM (rwx)  : ORIGIN = 0x20000000, LENGTH = 4M
}

_stack_size = 16K;
_heap_size = 256K;
