/* Keep link-time task registrations in flash.
 *
 * `linkme` emits a writable, relocatable custom section. Left to the
 * linker's orphan-section placement it lands in RAM immediately after
 * `.data`, but `cortex-m-rt` only copies the `.data` output section, so
 * the registrations read as zero at runtime. An explicit output section
 * in FLASH gives the slice stable contents without requiring any boot
 * code changes. The section is read-only after relocation, so keeping
 * it in flash is safe.
 */

SECTIONS
{
  linkme_LINKED_TASKS : ALIGN(4)
  {
    KEEP(*(linkme_LINKED_TASKS))
  } > FLASH

  linkm2_LINKED_TASKS : ALIGN(4)
  {
    KEEP(*(linkm2_LINKED_TASKS))
  } > FLASH
} INSERT AFTER .rodata;
