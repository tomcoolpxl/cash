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

2. **Keys that arrived as VT text are decoded** (`src/event/source/windows/vt_keys.rs`,
   and `next_event` and `vt_text` in `src/event/source/windows.rs`). While a program has
   `ENABLE_VIRTUAL_TERMINAL_INPUT` on, the console turns each key into the text a
   terminal sends as it arrives: a key-down with no virtual key and no scan code per
   character, `\r` for Enter, `\x7f` for Backspace, `ESC [ A` for Up. Keys typed ahead
   while such a program ran are still in that form after it exits and cash has turned
   VT input off (spec D68). Upstream found no key in them: Enter, Tab and Escape were
   dropped, so a command typed ahead needed a second Enter, and Backspace and the arrows
   typed DEL, `[` and `A` into the line.

   Such records are now decoded as crossterm decodes a terminal's bytes on Unix: Enter,
   Tab and Backspace; Ctrl with a letter for other control characters; the cursor,
   editing and function keys with xterm's modifiers, in both cursor-key modes; Alt with
   the key after an escape; and a lone escape as the Escape key. Other reports (mouse,
   focus, bracketed-paste markers) are dropped. Printable characters were already typed
   correctly and are left to upstream, and keys with a virtual key are untouched. Tested
   by the module's unit tests and by
   `conpty_a_program_that_had_vt_input_on_does_not_eat_keys_typed_ahead`.
