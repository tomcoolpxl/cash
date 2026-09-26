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

1. **Console input is read in batches** (`src/event/source/windows.rs`, and the
   `consoleapi` and `wincon` features of `winapi` in `Cargo.toml`, which the patch calls
   into and which `crossterm_winapi` already turns on). Upstream read one input record per
   call, and each call is a round trip to the console host. Through ConPTY a paste arrives
   as a key-down and a key-up record per character, so a 2000-character paste cost
   thousands of round trips, a quarter of a second before the line was drawn.

   The source now peeks at what is waiting and reads, in one call, the run of records at
   the front that only type text: printable characters without Ctrl or Alt, modifier keys
   on their own, and key releases. Converting them to events is unchanged and happens one
   `try_read` at a time, in order, from a queue. Any other record ends the batch and is
   read on its own, exactly as before. That keeps Reedline's rule of reading nothing past
   Enter: keys typed after it stay in the console for the command it starts, which
   `crates/cash/tests/conpty_interactive_tests.rs` checks with `read`.
