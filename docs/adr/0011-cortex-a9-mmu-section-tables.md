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
- Map granularity is fixed at 1 MiB: both the virtual address and size passed
  to `map_region` must be 1 MiB aligned. Sub-section protection is not
  emulated; callers needing finer isolation must use a future 2-level table.
- `APagePerms` encodes privilege/user read-write permissions and the
  strongly-ordered device attribute directly in the section descriptor.
- TTBR0 points at one process-wide, 16 KiB-aligned static L1 table. `enable`
  programs TTBR0, puts domain 0 in client mode, invalidates the local TLB, and
  enables the MMU, D-cache and I-cache. Secondary cores share the table but
  each programs its own TTBR0/DACR/TLB/SCTLR state.
- The Cortex-M port continues to use `caplet_kernel::mmu::NoMmu` because the
  M-profile boards in this repository do not configure an MPU/MMU-backed
  process address space.

## Consequences

- A module loaded onto the Cortex-A9 board can request a mapped region through
  the `MapMemory` syscall; the Cortex-A MMU applies it at MiB granularity, and
  `UnmapMemory` clears the sections and invalidates the local TLB.
- The compact L1 table keeps the boot path deterministic and
  avoids the cost of a "real" 2-level page table for v1.
- The RISC-V port uses Sv32/Sv39 instead of ARM L1 sections; the shared MMU
  abstraction isolates the architecture-specific page-table format.
