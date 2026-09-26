# Reedline, as cash carries it

The line editor for cash's interactive prompt, carried in the repository with the patch
below and used through `[patch.crates-io]` in the workspace `Cargo.toml`.

- **Original source:** [`reedline`](https://crates.io/crates/reedline) 0.51.0 from
  crates.io, by the Nushell Project Developers
- **Imported:** 2026-09-26, as published (`src`, manifest, license, README)
- **License:** MIT (see LICENSE)

It sits outside the workspace, so cash's lints do not apply to it. Its own tests run with
`cargo test --manifest-path vendor/reedline/Cargo.toml --lib`.

## Patches

1. **A host command's output is not painted over** (`src/painting/painter.rs`,
   ROADMAP item 15). After a `bind -x` command (`ExecuteHostCommand`), Reedline redrew
   the prompt on its old rows whenever the cursor came back anywhere inside them, and
   that range runs one row past a single-line prompt. A command that printed one line
   ended there, so `bind -x '"\C-t": echo hi'` showed nothing: the prompt was redrawn
   over `hi`. The painter now records the cursor's cell when it suspends and redraws in
   place only when the cursor is back on exactly that cell; otherwise the prompt goes
   where the cursor is. That also covers upstream's flush-at-bottom case
   (nushell/reedline#1130). Upstream's main branch records the same cell but still
   uses the range above the bottom row, so the bug is there too.

   Cash's side (`crates/cash-interactive/src/reedline/input_backend.rs`) clears the
   line before running the command, as Bash does, so the output starts where the
   prompt was and the prompt returns below it, byte for byte as in Bash 5.3.
