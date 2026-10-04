---
names: test [
see: chmod ln paths
spec: D23 D27 D46
---
## Description

`test EXPR` and `[ EXPR ]` evaluate a condition and return 0 when it holds: files (`-e`,
`-f`, `-d`, `-s`, `-x`, `-L`, `-nt`), strings (`-z`, `-n`, `=`, `!=`), numbers (`-eq`,
`-lt`, ...), combined with `!`, `-a`, `-o` and parentheses. `[[ ... ]]` is the shell's
own keyword, with patterns and `=~`.

## Windows notes

- `-x FILE` is true when the access list lets you run the file **and** it is an
  executable kind: an extension in `PATHEXT` (`.exe`, `.cmd`, ...) or a `#!` line.
  Windows grants execute on every file you own, so the access list alone would call
  `README.md` executable (D23).
- `-L FILE` is true for symbolic links and junctions, not for App Execution Aliases or
  `.lnk` shortcuts (D27).
- `-s FILE` is false for App Execution Aliases (`python.exe` from the Store), which are
  empty files that run anyway (D46).
- Paths may be spelled `C:/x`, `/c/x` or a quoted `"C:\x"`.
