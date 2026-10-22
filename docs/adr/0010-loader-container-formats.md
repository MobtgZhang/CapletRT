# ADR-0010: Dynamic Module Loader Container Formats (M3)

Status: Accepted
Date: 2026-07-10
Supersedes: —

## Context

The M3 roadmap (docv1.md §12) calls for a "small, auditable
dynamic module loader that accepts a deliberate subset of ELF
and a custom .capp container". The reason a custom container is
introduced at all is that:

- A bare ELF PT_LOAD with implicit relocations is hard to audit:
  every symbol can pull in transitive capability needs. This is
  the failure mode that has driven the seL4-style projects
  towards explicit top-level cap manifests.
- The cap manifest has to be **declared** at packaging time so
  that the loader can refuse a module before any code runs.

## Decision

`caplet-loader` accepts two formats:

1. **ELF subset** — `parse_elf` only consumes 32-bit ARM ELF,
   `ET_EXEC`, `PT_LOAD` segments only, `R_ARM_RELATIVE`
   relocations only. `PT_DYNAMIC` (and `DT_*` entries) are
   rejected with `LoaderError::ParseFailed`. This is enough to
   accept a `cargo build --target thumbv7em-none-eabihf`
   artifact unchanged, so the developer workflow is preserved
   while stripping every dynamic-linking surface.
2. **`.capp`** — a 120-byte fixed header followed by a compact
   `cap manifest` (each cap is a 28-byte tag + 4-byte perms
   word). The header carries `text_off/text_size`,
   `data_off/data_size`, `reloc_off/reloc_count`, `cap_off/cap_count`,
   `name_off`, and a 64-byte `signature`. Names are null-
   terminated UTF-8; tags are likewise fixed 28 bytes
   right-padded with zero.

Both formats are parsed up-front; the verifier stage then
checks the signature, and only on success does the loader
hand the bytes to the relocator + pool allocator.

## Consequences

- `.capp` is the production format. ELF is for developer
  convenience only.
- A module that wants more capability than its manifest
  declares is rejected **at load time**, never at runtime.
- The signature field of `.capp` keeps working in a host-only
  build (no signature required): the loader treats `flags &
  FLAG_SIGNED == 0` as "unsigned, accept".
- The `.capp` header is architecture-neutral, but the current ELF parser
  accepts only 32-bit little-endian ARM (`EM_ARM`) and the relocator implements
  only `R_ARM_RELATIVE`. Loading RISC-V or AArch64 ELF modules requires new
  machine/relocation handling; the existing path is not silently reused.
