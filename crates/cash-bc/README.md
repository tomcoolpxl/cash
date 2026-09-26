# cash-bc

POSIX `bc` for Cash, absorbed and adapted from [posixutils-rs](https://github.com/rustcoreutils/posixutils-rs).

- **Original source:** `posixutils-calc` 0.9.0 (`calc/bc.rs` and `calc/bc_util`)
- **Imported revision:** `96bd8a372541cf7e4c427010a5414a40cf20c805`
- **Revision date:** 2026-09-25
- **License:** MIT (see LICENSE)

`src/bc_util` is upstream's, with the one change listed below; `src/lib.rs` replaces
upstream's `bc.rs` binary, and `tests/cases` is upstream's suite, which cash runs from
`crates/cash/tests/bc.rs`.

## Adaptations for Cash

1. **Library entry point:** `pub fn run_bc(args) -> i32` for Cash's bundled-command
   dispatcher (`cash --invoke-bundled bc`), returning the exit status instead of calling
   `std::process::exit`. The interpreter still runs on a large-stack thread, so its own
   recursion limit, not a guard page, stops a runaway program.
2. **No gettext, `plib` or rustyline:** messages are English literals; `src/diag.rs`
   reproduces the part of `plib::diag` bc uses, with the same output format; standard
   input is read a line at a time without prompts, as POSIX bc reads it.
3. **CRLF input (Cash D20):** a CRLF line is read as an LF line, from a file or standard
   input; a lone CR is still an illegal character.
4. **No leading zero:** a value below one prints as `.33`, `-.5`, as GNU, BSD and
   BusyBox bc print it; upstream printed `0.33`. This is the change in
   `bc_util/number.rs`, and the expected output in upstream's unit tests,
   `tests/cases/*.out` and three tests in `tests/cases/mod.rs` changed with it.
5. **GNU options accepted:** `-q`/`--quiet`, `-s`/`--standard` and `-w`/`--warn` change
   nothing (there is no banner, and this bc is always POSIX bc); `--mathlib` is `-l`;
   `-i`/`--interactive` recovers from errors as a terminal session does.
6. **GNU extensions named:** POSIX bc only, by decision. When input fails to parse,
   `src/gnu.rs` names the first GNU-only construct in it — `print`, `read()`, `else`,
   `&&`, `||`, `!`, `#` comments, `last`, `halt`, `continue`, `limits`, multi-letter
   names — so a GNU script fails with a reason rather than only "expected a newline".
7. **Tests:** `tests/cases/mod.rs` reaches its helpers through `crate::plib`, which
   `crates/cash/tests/bc.rs` provides by running `bc` inside cash.
