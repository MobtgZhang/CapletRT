/* Custom linker script for the QEMU MPS2-AN500 (Cortex-M4 with FPU).
 *
 * This file is a stripped-down subset of the `cortex-m-rt` link.x template.
 * The differences are:
 *   - The `MEMORY` block is inlined here for the QEMU MPS2-AN500 board
 *     (1 MiB flash, 1 MiB SRAM) so we don't need a separate `memory.x`.
 *   - Stack and heap sizes are inlined as `_stack_size` and `_heap_size`.
 *
 * All other symbols (DefaultHandler_, HardFault_, __EXCEPTIONS, __INTERRUPTS,
 * __RESET_VECTOR, Reset, DefaultPreInit) are provided by the `cortex-m-rt`
 * crate itself and resolved at link time.
 */

/* Developer notes
 *
 * - Symbols that start with a double underscore (__) are considered "private"
 *
 * - Symbols that start with a single underscore (_) are considered "semi-public"; they can be
 *   overridden in a user linker script, but should not be referred from user code (e.g. `extern "C" {
 *   static mut __sbss }`).
 *
 * - `EXTERN` forces the linker to keep a symbol in the final binary. We use this to make sure a
 *   symbol is not dropped if it appears in or near the front of the linker arguments and "it's not
 *   needed" by any of the preceding objects (linker arguments)
 *
 * - `PROVIDE` is used to provide default values that can be overridden by a user linker script
 */

/* Memory layout for the QEMU MPS2-AN500 (Cortex-M4 with FPU). */
MEMORY
{
  FLASH (rx) : ORIGIN = 0x00000000, LENGTH = 512K
  RAM (rwx)  : ORIGIN = 0x20000000, LENGTH = 128K
}

_stack_size = 8K;
_heap_size = 16K;

/* Entry point = reset vector */
EXTERN(Reset);
EXTERN(__EXCEPTIONS); /* depends on all the these PROVIDED symbols */
EXTERN(__INTERRUPTS); /* `static` variable similar to `__EXCEPTIONS` */
EXTERN(__RESET_VECTOR);
EXTERN(__pre_init);
ENTRY(Reset);

/* Exception vectors */
EXTERN(DefaultHandler_);
EXTERN(HardFault_);
EXTERN(DefaultPreInit);
EXTERN(DefaultHandler);
PROVIDE(NonMaskableInt = DefaultHandler);
EXTERN(HardFaultTrampoline);
PROVIDE(MemoryManagement = DefaultHandler);
PROVIDE(BusFault = DefaultHandler);
PROVIDE(UsageFault = DefaultHandler);
PROVIDE(SecureFault = DefaultHandler);
PROVIDE(SVCall = DefaultHandler);
PROVIDE(DebugMonitor = DefaultHandler);
PROVIDE(PendSV = DefaultHandler);
PROVIDE(SysTick = DefaultHandler);

PROVIDE(DefaultHandler = DefaultHandler_);
PROVIDE(HardFault = HardFault_);

/* Pre-initialization function */
PROVIDE(__pre_init = DefaultPreInit);

/* Sections */
SECTIONS
{
  PROVIDE(_ram_start = ORIGIN(RAM));
  PROVIDE(_ram_end = ORIGIN(RAM) + LENGTH(RAM));
  PROVIDE(_stack_start = _ram_end);

  /* Vector table */
  .vector_table ORIGIN(FLASH) :
  {
    __vector_table = .;
    LONG(_stack_start & 0xFFFFFFF8);
    KEEP(*(.vector_table.reset_vector));
    __exceptions = .;
    KEEP(*(.vector_table.exceptions));
    __eexceptions = .;
    KEEP(*(.vector_table.interrupts));
  } > FLASH

  PROVIDE(_stext = ADDR(.vector_table) + SIZEOF(.vector_table));

  .text _stext :
  {
    __stext = .;
    *(.Reset);
    *(.text .text.*);
    *(.HardFaultTrampoline);
    *(.HardFault.*);
    . = ALIGN(4);
    __etext = .;
  } > FLASH

  .rodata : ALIGN(4)
  {
    . = ALIGN(4);
    __srodata = .;
    *(.rodata .rodata.*);
    . = ALIGN(4);
    __erodata = .;
  } > FLASH

  .data : ALIGN(4)
  {
    . = ALIGN(4);
    __sdata = .;
    *(.data .data.*);
    . = ALIGN(4);
  } > RAM AT>FLASH

  . = ALIGN(4);
  __edata = .;
  __sidata = LOADADDR(.data);

  .gnu.sgstubs : ALIGN(32)
  {
    . = ALIGN(32);
    __veneer_base = .;
    *(.gnu.sgstubs*);
    . = ALIGN(32);
  } > FLASH
  . = ALIGN(32);
  __veneer_limit = .;

  .bss (NOLOAD) : ALIGN(4)
  {
    . = ALIGN(4);
    __sbss = .;
    *(.bss .bss.*);
    *(COMMON);
    . = ALIGN(4);
  } > RAM
  . = ALIGN(4);
  __ebss = .;

  .uninit (NOLOAD) : ALIGN(4)
  {
    . = ALIGN(4);
    __suninit = .;
    *(.uninit .uninit.*);
    . = ALIGN(4);
    __euninit = .;
  } > RAM

  PROVIDE(__sheap = __euninit);
  PROVIDE(_stack_end = __euninit);

  .got (NOLOAD) :
  {
    KEEP(*(.got .got.*));
  }

  /DISCARD/ :
  {
    *(.ARM.exidx);
    *(.ARM.exidx.*);
    *(.ARM.extab.*);
  }
}

/* Alignment checks */
ASSERT(ORIGIN(FLASH) % 4 == 0, "
ERROR: the start of the FLASH region must be 4-byte aligned");

ASSERT(ORIGIN(RAM) % 4 == 0, "
ERROR: the start of the RAM region must be 4-byte aligned");

ASSERT(__sdata % 4 == 0 && __edata % 4 == 0, "
BUG: .data is not 4-byte aligned");

ASSERT(__sidata % 4 == 0, "
BUG: the LMA of .data is not 4-byte aligned");

ASSERT(__sbss % 4 == 0 && __ebss % 4 == 0, "
BUG: .bss is not 4-byte aligned");

ASSERT(__sheap % 4 == 0, "
BUG: start of .heap is not 4-byte aligned");

ASSERT(_stack_start % 8 == 0, "
ERROR: stack start address is not 8-byte aligned.");

ASSERT(_stack_end % 4 == 0, "
ERROR: end of stack is not 4-byte aligned");

ASSERT(_stack_start >= _stack_end, "
ERROR: stack end address is not below stack start.");

ASSERT(__exceptions == ADDR(.vector_table) + 0x8, "
BUG: the reset vector is missing");

ASSERT(__eexceptions == ADDR(.vector_table) + 0x40, "
BUG: the exception vectors are missing");

ASSERT(SIZEOF(.vector_table) > 0x40, "
ERROR: The interrupt vectors are missing.
Possible solutions, from most likely to less likely:
- Link to a svd2rust generated device crate
- Check that you actually use the device/hal/bsp crate in your code
- Disable the 'device' feature of cortex-m-rt to build a generic application (a dependency may be enabling it)
- Supply the interrupt handlers yourself.");

ASSERT(ADDR(.vector_table) + SIZEOF(.vector_table) <= _stext, "
ERROR: The .text section can't be placed inside the .vector_table section
Set _stext to an address greater than the end of .vector_table (See output of `nm`)");

ASSERT(_stext > ORIGIN(FLASH) && _stext < ORIGIN(FLASH) + LENGTH(FLASH), "
ERROR: The .text section must be placed inside the FLASH memory.
Set _stext to an address within the FLASH region.");

ASSERT(SIZEOF(.got) == 0, "
ERROR: .got section detected in the input object files
Dynamic relocations are not supported. If you are linking to C code compiled using
the 'cc' crate then modify your build script to compile the C code _without_
the -fPIC flag.");

ASSERT(SIZEOF(.vector_table) <= 0x400, "
There can't be more than 240 interrupt handlers.");
