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
11. **IGNORECASE, parentheses and long strings:** gawk's `IGNORECASE` is honoured where gawk honours it: every regex (`~`, `match`, `sub`, `gsub`, `split`, FS and RS of more than one character), `index`, the comparison of strings, and `asort`/`asorti`; a separator of one character, `in` and subscripts are not affected. A variable in parentheses is a value, as in gawk: no lvalue for `sub`, no array for `split` or a function. `s = s x` appends to `s` in place, where it copied the whole string twice, so building a string a piece at a time took time in its square.
12. **gensub:** gawk's `gensub(regexp, replacement, how [, target])` returns `target` (`$0` by default) with every match replaced (`how` starting with `g` or `G`) or only the `how`th, and leaves `target` as it was; `&` and `\0` are the match, `\1` to `\9` its groups, and a `how` not above 0 is 1, with gawk's warning.
13. **gawk's `/=` and the end of a program:** `/=` after a variable that starts an expression is an assignment; after any other operand it starts a regex (`(x) /= 2/` is `x` joined to `/= 2/`), as in gawk's grammar, and an assignment may follow a comparison, a match, `&&` or `||` (`a || b = 1`). A backslash and a newline join a regex's lines, and a newline may follow a parameter's comma. A program file ends where it ends, as in gawk: a rule it leaves incomplete, or a syntax error at a last token gawk's lexer looks past, is gawk's `(END OF FILE)` error and a regex its `unterminated regexp at end of file`; a function's header fails at the token gawk's parser meets, while gawk's newline after a program given as an argument lets a backslash last continue its line.

## Deliberate differences from gawk

- **`split()` into an array whose element a parameter is bound to.** In
  `function f(s) { split("x y", a); s[1] = 1 } BEGIN { a[0][1] = 7; f(a[0]) }`, gawk 5.4
  stops with "attempt to use scalar parameter `s' as an array": `split()` frees the
  subarray `a[0]` and gawk reuses the freed node, now a scalar, for `s`. cash detaches the
  parameter when its element is removed, as gawk does for `delete`, and `s` stays an array
  of its own. Copying the outcome of a freed node's reuse would not be a rule anyone could
  rely on.
- **IGNORECASE folds case as Unicode does.** cash reads text as UTF-8 whatever the locale,
  so `"É" ~ /é/` is true under IGNORECASE, as in gawk in a UTF-8 locale; gawk in the C
  locale compares bytes and does not fold `É`.
- **The order of `for (k in a)` for indices that are not positive integers.** gawk goes
  through them in the order of its hash table; cash goes through them in the order they
  were made. The positive integers come after them, in order, in both.
- **`gensub`'s groups after the first match.** Git for Windows' gawk 5.4.0 gives `\1` to
  `\9` their text only in the first match it replaces: `gensub(/(.)(.)/, "\\2\\1", "g",
  "abcdef")` is `ba` there, where gawk's manual and cash give `badcfe`. cash replaces
  each match with its own groups, as gensub is documented to.
