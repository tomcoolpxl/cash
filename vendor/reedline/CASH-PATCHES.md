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

2. **A keystroke no longer redraws the prompt** (`src/painting/painter.rs`,
   `src/painting/utils.rs`). Reedline cleared from the prompt's first row and printed
   the prompt and the line again on every key. With Starship that is a colored status
   line, glyphs included, for each character typed or deleted: 178 bytes per Backspace
   through ConPTY where PowerShell sends 68, all of it for the terminal to parse and draw
   again. The painter now records the prompt it drew (text, colors, row, screen size);
   a paint that would draw the same one, with nothing written, scrolled or re-anchored
   in between, moves to where the input starts and redraws from there. A right prompt,
   a large buffer, a prompt ending on the margin, a resize, a scroll, a new line or
   anything that invalidates the anchor still gets the full paint.

   Such a paint erases only the rest of the input's row when neither it nor the paint
   before drew anything below that row (no wrap, no menu, no multi-line input or hint).
   ConPTY resends every row an erase covers, blank or not, so erasing to the end of the
   screen made a prompt at the top of an empty screen cost the whole screen on each key.

3. **A paste is not held back for 100 ms** (`src/engine.rs`). After a burst of more than
   ten events Reedline waits `POLL_WAIT` for more before painting, and every paste ended
   with that wait running out. It is 10 ms now: a paste reaches the console in one piece,
   and one that stalls longer is painted in two batches, which costs a repaint.

4. **An abbreviation expands however the keys were read** (`src/engine.rs`, spec D60).
   Reedline merges the keys of one read into one edit and tried abbreviation expansion
   only when that edit began with a space. Keys that arrive faster than the prompt
   repaints are read together, so `gco` then Space became one edit starting with `o`, and
   `gco` stayed as typed. The edit now runs up to each space it inserts, tries expansion
   there, and goes on. A bracketed paste arrives as one inserted string rather than as
   spaces, so pasted text still never expands, as in fish.

5. **Keys typed after a bound key are kept** (`src/engine.rs`). A key bound to a host
   command (`bind -x`, and cash's own Ctrl-X Ctrl-E and Alt-arrows) ends the read, and
   Reedline dropped the rest of the batch it arrived in: typing on straight after Ctrl-R
   for atuin, or after Ctrl-T for fzf, lost those keys. The rest of the batch is now kept,
   unparsed, and read first by the next read, so it meets the bindings the command may
   have changed. Keys typed after Enter never reach this batch: the console is read only
   up to an Enter (crossterm's patch).

6. **Tab over a history hint opens the menu on the hint's value** (`src/engine.rs`,
   `src/menu/mod.rs`, `src/menu/columnar_menu.rs`, `src/menu/menu_functions.rs`; spec
   D40). The hint and the completion menu knew nothing of each other. With
   `cd docker-labs/` in history, `cd dock` showed the hint `er-labs/`, and Tab opened
   the menu on its first value, `docker-fullstack-lab/`.
   - As a menu opens, the engine offers it the line the hint would make, through a new
     `Menu::set_hinted_line` that other menus ignore. It asks the hinter again for the
     line on screen, because keys read in one batch leave the last paint's hint behind.
     This happens only where a hint could be accepted: hints active, the cursor at the
     buffer end.
   - On its first final answer, the columnar menu selects the value whose acceptance
     turns the buffer into the longest start of that line
     (`CompletionDisplay::index_leading_to`). A value that appends a space counts only
     where the line has whitespace after it, or ends.
   - A move or an edit in the menu drops the line.
   - Cash's menu wrapper (`QuoteAwareMenu`) forwards the call.

7. **A Tab that inserts the shared prefix does only that** (`src/engine.rs`, spec D40).
   With partial completions on, Reedline spliced in the prefix the suggestions share and
   opened the menu beneath it, on the same Tab. With the new
   `with_shared_prefix_first(true)`, which cash sets, the menu is closed again, and the
   next Tab opens it, as Bash lists only on a later Tab.
   - A Tab with nothing shared left to insert opens the menu at once.
   - `decide_menu_completion` now returns what the caller does next (`MenuOpening`), so
     the `Menu` event and a late answer in `settle_completions` still decide alike.
   - Off by default, so Reedline's own behaviour and tests stay as they were.

8. **Text typed ahead of an Esc keeps its order in vi mode** (`src/engine.rs`,
   2026-10-05). `process_input_batch` parsed a whole batch of keys, then ran the events
   under the mode the parsing ended in. Text and an Esc read together (fast typing, a
   paste, a slow link) were inserted under vi normal mode, whose cursor rests on a
   character, so each insert after the first landed before it: `abc` Esc became `bca`.
   Each event now carries the mode it was parsed under, the editor takes that mode
   before running it, and a run of fused edits is cut where the mode changes;
   `run_edit_commands` keeps re-syncing to the current mode when a host calls it.
   Test: `engine::tests::text_and_esc_in_one_batch_insert_in_order`.

9. **Accepting the next line can be set on a `Reedline` in use** (`src/engine.rs`,
   2026-10-05). `with_immediately_accept` is a builder, which takes the `Reedline` by
   value; cash's croot picker (spec D73) sets a line like `cd src/` and has it accepted
   by the next `read_line`, which paints it first, on the `Reedline` the input backend
   holds. `set_immediately_accept` does that by reference.
