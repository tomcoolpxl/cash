# cash-crossterm

Terminal I/O for cash, under its line editor and a few builtins:
[crossterm](https://github.com/crossterm-rs/crossterm) 0.29.0, © 2019 T. Post and the
crossterm contributors, under the MIT License (`LICENSE`), made cash's own on 2026-10-10.
From 2026-09-27 until then it was carried in `vendor/crossterm` as a patched copy.

It keeps crossterm's package name and version: `uu_more`, which cash bundles unchanged
from crates.io, asks for crossterm 0.29, and `[patch.crates-io]` in the workspace hands it
this one, which only a crate of that name and version can be. Its features keep the names
manifests ask for, `use-dev-tty` and `libc` among them, though they do nothing here.

## Made cash's own (2026-10-10)

- **Windows only.** The Unix code is gone: the terminal, cursor and event backends, the
  `mio` and tty event sources and their wakers, the file descriptor and `/dev/tty` code,
  with `libc`, `rustix`, `mio`, `signal-hook` and `filedescriptor`. So are the features
  cash never turned on, with their code: the async `EventStream` (`event-stream`,
  `futures-core`) and OSC 52 clipboard writes (`osc52`, `base64`), and the test
  dependencies only those used (tokio, async-std, futures).
- **What could panic now returns an error or cannot**: a command whose `write_ansi`
  fails without an I/O error, `SetStyle` and `Print` asked to run through WinAPI, and a
  window title whose `Display` fails are errors; the original console colour is the
  console's default before it was read; the input parser's surrogates and case changes
  no longer unwrap; a timeout's leftover saturates.
- **Every unsafe block says why it is sound**, one operation to a block.
- `#rrggbb` colours are parsed as one number, not sliced.
- **Lints and edition.** The workspace's lints apply; the style ones crossterm was not
  written to are allowed in `src/lib.rs`, its undocumented public API among them, as for
  `cash-sed`. The 2021 edition stays until moving to the workspace's is a change of its
  own.
- Its colour tests turn colour on themselves, so `NO_COLOR` in a developer's shell does
  not fail them, and `test_parse_ansi_bg` tests the background, as its name says.

## The changes made while it was a patched copy

The tests and comments elsewhere in cash refer to these by number.

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
