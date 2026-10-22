# `stack-analyzer` (M3 placeholder)

This directory is a **placeholder** for the worst-case stack-usage analyzer
listed in the M3 plan appendix. No analysis is implemented.

## Status

- The crate compiles and documents the intended report schema.
- It does **not** read ELF files, build call graphs, or compute stack bounds.
- It is excluded from the root workspace and from CI gates.

## Intended inputs and outputs (M3)

Inputs:

- board ELF with DWARF call-site information,
- linker symbols for `.stack` / heap boundaries,
- interrupt vector table and task entry points.

Outputs (per task and per ISR):

- direct and indirect call-chain depth,
- estimated worst-case bytes with an explicit confidence marker,
- call chains that cross an unsafe or extern boundary,
- a non-zero exit when a bound exceeds the declared task stack.

Until this exists, task stack sizes remain hand-reviewed static constants in
each board crate. Do not cite this tool as evidence of stack safety.
