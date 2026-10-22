# Re-using Kani Harnesses as Living Documentation

The Kani harnesses in `harness/` are written in plain Rust and only
rely on the `#[kani::proof]` attribute, so they are also valid
documentation:

* Each file starts with a docstring stating the property being
  proved (the "what").
* Inputs are constructed with `kani::any()` followed by `kani::assume`
  preconditions, which act as input-space documentation.
* The body of the proof lists the post-conditions as `assert!`
  statements, which are the "then this" clauses of the spec.

When the harnesses are not being model-checked (e.g. when reviewing
the code), they read like executable specifications.

## Synchronising with the codebase

If a harness ever fails to compile because the underlying API
changed, the right fix is usually to update both the implementation
and the harness together — the harness expresses the intent of the
API. Do not silence a failing harness; either:

1. The harness was wrong (the property is no longer true). Update
   the docstring to reflect the new contract and add a regression
   test in `crates/*/tests/`.
2. The implementation regressed. Fix the implementation, then
   re-run the harness.

## Performance budget

A single Kani run should complete in under 60 s on a developer
laptop. If a harness exceeds that:

* Tighten the `kani::assume` preconditions (smaller `n`, fewer
  branches).
* Split the harness into smaller properties.
* Add `#[kani::unwind(N)]` annotations to bound loop iterations.

The default unwind bound is 1 — most of our harnesses are loop-free
and need no annotation.
