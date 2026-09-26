# Crossterm, as cash carries it

The terminal library under cash's line editor (through Reedline) and a few builtins,
carried in the repository and used through `[patch.crates-io]` in the workspace
`Cargo.toml`.

- **Original source:** [`crossterm`](https://crates.io/crates/crossterm) 0.29.0 from
  crates.io, by T. Post and the crossterm contributors
- **Imported:** 2026-09-27, as published (`src`, manifest, license, README)
- **License:** MIT (see LICENSE)

It sits outside the workspace, so cash's lints do not apply to it. Its own tests run with
`cargo test --manifest-path vendor/crossterm/Cargo.toml --lib`.

## Patches

None yet.
