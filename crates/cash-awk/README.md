# cash-awk

Native AWK implementation for Cash, absorbed and adapted from [posixutils-rs](https://github.com/rustcoreutils/posixutils-rs).

- **Original source:** `posixutils-awk` 0.9.0
- **Imported revision:** `96bd8a372541cf7e4c427010a5414a40cf20c805`
- **Revision date:** 2026-09-25
- **License:** MIT (see LICENSE)

## Adaptations for Cash

1. **Windows regex:** Replaced Unix libc `regcomp`/`regexec` with a Rust regex adapter using `regex-automata` configured for `LeftmostLongest` matching, preserving POSIX ERE earliest/longest matching semantics.
2. **Subprocess execution:** Replaced libc `system()`, `popen()`, and `pclose()` with a subprocess command host running child processes (`cash -c`, falling back to `cmd /c`), providing proper pipe handling, exit codes, and resource cleanup without CRT leaks.
3. **Library entry point:** Provides `pub fn run_awk(args: impl IntoIterator<Item = impl Into<std::ffi::OsString>>) -> i32` for Cash's process-backed bundled command dispatcher instead of calling `std::process::exit`.
4. **Robustness:** Fixed array deletion bookkeeping bug where `swap_remove` at the end caused out-of-bounds panics, fixed function scalar-as-array panics, and prevented script errors from unwinding out of the process.
5. **Windows only:** Cash builds for Windows only, so the Unix-only code paths were removed: the `sh -c` shell fallback and the signal-number exit status (`128 + signal`) in `system()`.
6. **gawk's words:** A fatal error is reported as gawk reports it (``awk: cmd. line:1: (FILENAME=- FNR=2) fatal: attempt to use scalar `x' as an array``), where posixutils-rs wrote `runtime error:` and a call trace; and gawk's `func` spells `function`.
