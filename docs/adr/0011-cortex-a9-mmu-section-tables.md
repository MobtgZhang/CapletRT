# ADR-0011: Cortex-A9 Real MMU + L1 Section Tables (M3)

Status: Accepted
Date: 2026-07-10
Supersedes: —

## Context

docv1.md §13.2 chose "L1 section descriptor rather than 2-level
page table (v1 simplification)". M3 finally implements this for
the Cortex-A9 port: the kernel builds a static `[u32; 4096]` L1
table and the `ports/cortex-a/src/mmu.rs` driver can map/unmap
1 MiB sections and TLB-invalidate a single virtual address.

## Decision

- The MMU abstraction lives in `ports/cortex-a/src/mmu.rs` and
  implements `caplet_kernel::arch::Arch` so it can be plugged
  into `Kernel::early_init`.
- Map granularity is fixed at 1 MiB. Smaller regions are
  emulated by mapping one section and tagging the unused bytes
  as inaccessible via the page-permission bits in
  `APagePerms`.
- The TTBR0 (user CSpace) and the kernel half of the virtual
  address space are managed by the kernel's
  `MmuOps`; the `ports/cortex-a/src/mmu.rs::Mmu` is a
  **Cortex-M-NoMmu fallback** for the meta-code (loader / cap
  bookkeeping) that lives in the always-valid kernel half.
- User code in the Cortex-M port keeps running on
  `caplet_kernel::mmu::NoMmu` because there is no MMU to drive.

## Consequences

- A module loaded onto the Cortex-A9 board can ask for a
  physical page via the new `ModulePool` allocator at boot.
  Pages are mapped on-demand via `MapMemory` syscall.
- The compact L1 table keeps the boot path deterministic and
  avoids the cost of a "real" 2-level page table for v1.
- Future RISC-V work (M5) will use Sv32 instead of L1 sections;
  the `MmuOps` trait isolates this.
