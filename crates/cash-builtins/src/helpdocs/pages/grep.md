---
names: grep egrep fgrep
see: find sed crlf
---
## Description

`grep [OPTION]... PATTERNS [FILE]...` prints the lines of each file that match a
pattern, with GNU grep 3.12's options, messages and exit status: 0 when a line was
selected, 1 when none was, 2 on an error such as a missing file or a bad pattern.
`egrep` is `grep -E` and `fgrep` is `grep -F`.

Patterns are basic regular expressions (`\(`, `\{m,n\}`, `\|`), extended ones with
`-E`, or fixed strings with `-F`. They are read as the bundled `sed` reads its own, so
the two tools take one dialect: GNU's classes and operators (`[[:alpha:]]`, `\w`,
`\<`, `\b`), backreferences (`\(a\)\1`), and the same error messages for a pattern
that is malformed. `-P` (Perl syntax) is not carried; `-E` covers most of what it is
used for.

The engine is ripgrep's, so a search over a large tree is fast; `-r` reads the files
under a directory in name order.

## Windows notes

- A line that ends in CRLF is matched without its `\r`: `grep 'x$'`, `-x` and `-w`
  work on a Windows text file, and `-o` never prints the `\r`. The line is still
  printed as it was read, ending in CRLF. `-U` keeps the `\r` in the line, as GNU grep
  on Linux has it. A pattern file read with `-f` may end its lines in CRLF too.
- File names print with `/`, as every path cash prints: `grep -r foo src` names
  `src/main.rs`, and `grep -r foo` with no file names `main.rs` without a `./`.
- `--include`, `--exclude` and `--exclude-dir` match names as GNU grep does, case
  included: `--include='*.rs'` leaves out `MAIN.RS`.
- `--color=auto` colours when standard output is the terminal; the `GREP_COLORS`
  variable sets the colours as for GNU grep.
- On a clean Windows machine `grep` is not found at all, and with Git for Windows it
  is Git's copy; inside cash, `grep` is this one. `type -a grep` lists the others.

## Examples

```
grep -rn 'TODO' src --include='*.rs'
grep -c '^$' notes.txt
grep -E -o '[0-9]+\.[0-9]+' versions.txt
grep -v -e '^#' -e '^$' config.ini
git log --oneline | grep -i 'fix'
```
