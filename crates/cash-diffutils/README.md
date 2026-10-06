# diffutils

Cash's `diff` and `cmp`, absorbed from [uutils/diffutils](https://github.com/uutils/diffutils)
(revision `7aa5409c2ed723c7b9768ef4cd10f4176cf78f45`, crate `diffutils` 0.6.0, 2026-10-05),
© Michael Howell and the uutils developers, under the MIT or Apache-2.0 licence at the
reader's option; cash takes it under the MIT one. The licence texts are in
`LICENSE-MIT` and `LICENSE-APACHE`, the copyright notice in `COPYRIGHT`.

## In Cash

The upstream crate is a `diff` with the normal, unified, context, ed and side-by-side
formats and a `cmp`; its option parser took each option as a word of its own and knew
none of GNU diff's comparison options. Cash's copy keeps upstream's `cmp` and its
tab-expansion and time-stamp helpers, and gives `diff` GNU diffutils 3.12's interface:
a getopt-style parser (bundled short options, long-option prefixes, GNU's messages),
`-r`, `-N`, `-x`, the `-i -E -Z -b -w -B -I` comparison options, `--strip-trailing-cr`,
`-L`, `-T`, `-a`, `--color`, and GNU's exit codes. The change-script engine and the
five output formats were rewritten over one script of changes, in the shape of GNU's
`analyze.c`, `normal.c`, `context.c`, `ed.c` and `side.c`, so that every format sees the
same hunks and the same ignored changes. `CASH-PATCHES.md` lists every change.

Both tools are checked byte for byte against GNU diffutils 3.12 by
`crates/cash/tests/oracle/diff_cases.sh` and `cmp_cases.sh`, run under cash by
`crates/cash/tests/it/diff_builtin.rs`.

`src/bin/diffutils.rs` is upstream's multi-call program (`diffutils diff ...`,
`diffutils cmp ...`), kept for `tests/integration.rs` and for running the tools outside
the shell.
