---
names: diff cmp
summary: Compare two files line by line, or two directories file by file.
summary cmp: Compare two files byte by byte, and say where they first differ.
kind: Text and file tools
see: comm grep crlf
---

## Syntax

    diff [OPTION]... FILE1 FILE2
    cmp [OPTION]... FILE1 FILE2

    diff A B            what changed, in diff's own format
    diff -u A B         unified, as patch and Git read it
    diff -q A B         only whether they differ
    diff -r DIR1 DIR2   two directories, file by file
    cmp A B             where two files first differ

## Examples

What changed between two files:

    printf '%s\n' one two three > old.txt
    printf '%s\n' one 2 three four > new.txt
    diff old.txt new.txt

The same, unified, with names of your own in the header:

    printf '%s\n' one two three > old.txt
    printf '%s\n' one 2 three four > new.txt
    diff -u --label before --label after old.txt new.txt

Only whether they differ, as an `if` wants it:

    if diff -q notes.txt notes.txt >/dev/null; then echo same; fi
    diff -q notes.txt pairs.txt; echo "status $?"

Two directories:

    mkdir -p a b && echo x > a/f && echo y > b/f && echo z > a/only
    diff -rq a b

Where two files first differ, byte by byte:

    cmp notes.txt pairs.txt; echo "status $?"

## How it works

`diff` compares two files line by line and writes the changes that turn the first into
the second. The normal format names line numbers and an action, `2c2` (change), `3a4`
(add), `5d4` (delete), with `<` before the first file's lines and `>` before the
second's. `-u` writes the unified format: a header, then `@@` hunks with `-` for lines
removed, `+` for lines added and a space for context. That is what `patch` applies and
`git diff` writes.

With `-r`, two directories are compared: files of the same name by their contents, and
names on one side only reported as `Only in`. `-q` says only which differ.

`cmp` compares bytes and writes where the first difference is, `differ: byte 5, line
1`; `-s` writes nothing, for an `if`.

Both exit 0 when the inputs are the same, 1 when they differ, and 2 for trouble.

## Options

    -u, -U N, --unified[=N]  Unified format, with N lines of context (3).
    -c, -C N                 Context format.
    -y, --side-by-side       The two files side by side.
    -q, --brief              Only whether they differ.
    -r, --recursive          Compare directories, all through.
    -N, --new-file           A file on one side only is compared as empty.
    -i, --ignore-case        Upper and lower case alike.
    -w, --ignore-all-space   White space does not count.
    -b, --ignore-space-change  Runs of white space count as one.
    -B, --ignore-blank-lines Empty lines added or removed do not count.
    --strip-trailing-cr      A CR at a line's end does not count.
    -x, --exclude=PAT        Leave out files whose names match PAT.
    --label LABEL            A header's name, given twice for two.
    --color[=WHEN]           Colour the output: never, always, auto.

For `cmp`: `-s` (silent), `-l` (every differing byte, in octal), `-n N` (only the
first N bytes), `-i SKIP` (skip bytes first).

Exit status: 0 when the inputs are the same; 1 when they differ; 2 on trouble.

## Advanced

### Making and applying a patch

`diff -u old new > change.patch` writes a patch; `patch < change.patch` applies it to
`old`. `diff -ruN old_dir new_dir` makes one for a whole tree.

## Different in cash

- A CRLF file against an LF file differs on every line, as it does under GNU diff on
  Linux: the carriage return is a character. `--strip-trailing-cr` removes it from both
  inputs before comparing, and `-w` and `-b` ignore it as white space. `cmp` compares
  bytes, so it counts the carriage return too. See `help crlf`.
- Paths print as you typed them, joined with `/`: `Only in src/lib: x.rs`.
- The times in the `---`/`+++` headers are in the zone an exported `TZ` names
  (`TZ=UTC`), else Windows' own; `--label` replaces a header's name and time.
- Directory entries compare in byte order of their names, as on Linux, so `B` comes
  before `a`.
