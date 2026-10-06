---
names: diff cmp
see: comm crlf
---
## Description

`diff` compares two files line by line and prints what changed, in the normal format
(`3c3`, `< old`, `> new`), unified (`-u`, what `patch` and Git read), context (`-c`),
ed script (`-e`) or side by side (`-y`); `-r` compares two folders file by file. `cmp`
compares two files byte by byte and says where they first differ. Both are carried
inside cash, with GNU diffutils' options, messages and exit status: 0 when the inputs
are the same, 1 when they differ, 2 for trouble, so a script's `if diff -q a b` works
as it does on Linux.

## Windows notes

- A CRLF file against an LF file differs on every line, as it does under GNU diff on
  Linux: the carriage return is a character. `--strip-trailing-cr` removes it from both
  inputs before comparing, and `-w` and `-b` ignore it as white space. `cmp` compares
  bytes, so it counts the carriage return too. See `help crlf`.
- Paths print as you typed them: `diff -r src build` reports `Only in src/lib: x.rs`
  and heads a unified diff with `--- src/lib/x.rs`, joined with `/`.
- The time stamps in the `---`/`+++` and `***`/`---` headers are in the zone an
  exported `TZ` names (`TZ=UTC`), else Windows' own; `-L LABEL` replaces a header.
- `--color` colours a terminal as GNU's does; `--color=always` colours a pipe.
- Folder entries compare in byte order of their names, as on Linux, so `B` comes
  before `a`; the file system's case rules decide whether `A` and `a` are one file.
- `-y` lays the columns out with tabs, as GNU's does, which Windows Terminal shows at
  the same stops.

## Examples

```
diff -u old.txt new.txt > change.patch
diff -rq release/ build/            # which files differ, and which are only on one side
diff --strip-trailing-cr -u unix.txt windows.txt
if cmp -s a.bin b.bin; then echo same; fi
cmp -l a.bin b.bin | head           # the first differing bytes, in octal
```
