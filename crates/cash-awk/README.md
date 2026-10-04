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
6. **gawk's words:** A fatal error is reported as gawk reports it (``awk: cmd. line:1: (FILENAME=- FNR=2) fatal: attempt to use scalar `x' as an array``), where posixutils-rs wrote `runtime error:` and a call trace; and gawk's `func` spells `function`. A division by zero is gawk's fatal error (a zero constant divisor its error before the program runs), a builtin given the wrong number of arguments is reported in gawk's words, and an error met outside the program's code is placed at the code that ran last, as gawk places it. A syntax error is gawk's too, the source line and a caret, after the errors gawk meets before it (pest's `-->` form and its rule names before); so are a regex's errors (`invalid regexp: unbalanced (: /(/`) and the warnings of `log`, `sqrt` and `exp`.
7. **gawk's arrays of arrays:** `a[1][2] = 3`, `for (k in a[1])`, `(k in a[1])`, `delete a[1][2]`, `split(s, a[1])`, `length(a[1])`, a subarray passed to a function, and `isarray()`, as in gawk.
8. **A reader that goes away:** awk ends in silence with status 141, as `SIGPIPE` ends gawk, where it said `write error: Broken pipe` and ended with 2.
9. **gawk's grammar and arguments:** `**` and `**=`, prefix operators one after another (`- -x`), comparisons in `print` other than `>`, `match(s, r, arr)`, `split(s, a, fs, seps)` and `close(cmd, "to"|"from")`; `for (k in a)` goes through the keys `a` had when it began, as in gawk.
10. **gawk's extensions, files and escapes:** `asort()` and `asorti()` with a destination and gawk's orders or a comparison function; `/dev/null`, `/dev/stdin`, `/dev/stdout`, `/dev/stderr` and `/dev/fd/0` to `2` as gawk takes them; the regex operators `\y`, `\B`, `\<`, `\>`, `` \` `` and `\'`, `\x` escapes, and gawk's warnings for an escape it has no meaning for. A keyword ends where gawk ends one (`printx` is a name), a plain `getline` is an operand, `sub` of a constant replaces in a copy, and `for (k in a)` goes through the indices that are positive integers last, in order, as gawk does; the others keep the order they were made in, where gawk's is a hash table's.
