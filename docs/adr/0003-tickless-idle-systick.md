# ADR-003: Tickless Idle Powered by SysTick + Software Counter

Status: Accepted
Date: 2026-07-09
Supersedes: —

## Context

A classic tick-based RTOS wakes the CPU every 1 ms to advance
the tick counter and check whether any timer has expired. On
battery-powered devices this is the largest source of wakeups
between real events. The M2 demo targets the MPS2-AN386 (QEMU
model of the Cortex-M4) which exposes a single SysTick timer.

A full tickless implementation that uses the next-event timer
(typically the Cortex-M SysTick or a low-power timer) is the
standard solution. The complication is that we need a
software 64-bit counter to keep monotonic time across the
deep-sleep gaps, plus a way to schedule the wakeup at the
exact tick boundary.

## Decision

We implement tickless idle using **SysTick + a 64-bit software
counter built from two 32-bit atomics**. When the scheduler
has no work for the next `Δt` ticks, it programs SysTick with
`Δt` reload, executes `wfi`, and on wake extends the counter
by `Δt`. We deliberately do not rely on a second timer because
the QEMU target only exposes SysTick.

## Consequences

- A single Cortex-M target (Cortex-M0) without atomic 64-bit
  support still compiles: `IDLE_TICKS_LO` and `IDLE_TICKS_HI`
  are `AtomicU32` and we do the 64-bit addition in software.
- The wake-up latency is bounded by the smallest timer in the
  wheel, not by the tick period.
- If a real Cortex-A board exposes multiple timers, the
  abstraction in `caplet-kernel::tickless` lets us swap the
  backend later without touching the scheduler.

## Alternatives considered

- **Wake every tick**: rejected — measurable current draw on
  the real hardware; the demo cannot show the M2 power figures.
- **Per-target tickless backend**: rejected for M2 — we want
  one portable implementation that the QEMU CI can exercise.