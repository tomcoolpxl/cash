# uutils tools, as cash carries them

Four of the uutils coreutils that cash bundles (D48), carried in the repository with the
patch below and used through `[patch.crates-io]` in the workspace `Cargo.toml`. The other
bundled tools come from crates.io as published.

- **Original source:** [`uu_tee`](https://crates.io/crates/uu_tee),
  [`uu_sort`](https://crates.io/crates/uu_sort),
  [`uu_uniq`](https://crates.io/crates/uu_uniq) and
  [`uu_shuf`](https://crates.io/crates/uu_shuf) 0.12.0 from crates.io, by the uutils
  developers
- **Imported:** 2026-09-30, as published (all files but `Cargo.lock`)
- **License:** MIT (see each crate's LICENSE)

They sit outside the workspace, so cash's lints do not apply to them. Each manifest gains
one dependency, `cash-win32`, at its end.

## Patch

1. **A `>(...)` is opened as the pipe it is** (`uu_tee/src/tee.rs` `open`,
   `uu_sort/src/sort.rs` `Output::new`, `uu_uniq/src/uniq.rs` `open_output_file`,
   `uu_shuf/src/shuf.rs` `create_output`). cash hands a `>(...)` to these tools as a
   named pipe, `\\.\pipe\cash-procsub-…` (spec D17), and a named pipe cannot be created,
   truncated or appended to: `echo x | tee >(cat)` failed with "The parameter is
   incorrect", and `tee -a` with "Access is denied". Each tool now opens the file it is
   to write through `cash_win32::pipe::open_output`, which opens such a pipe for writing
   as it is and any other path as the tool asked. `sort`, which truncates its output
   only once it has read its input, leaves such a pipe alone in `Output::into_write` as
   well: Windows says a named pipe is a regular file, and truncating one fails. Every
   patched line is marked `// cash:`.

   A bundled tool that is not patched is handed a temp file for a `>(...)` instead, which
   every program can create and truncate; the list of patched ones is
   `SUBSTITUTION_PIPES` in `crates/cash-shell/src/bundled.rs`, and it must name exactly
   these four.
