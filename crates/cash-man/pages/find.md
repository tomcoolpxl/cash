---
name: find
summary: Search a directory tree for files that match an expression.
kind: Text and file tools
see: xargs where ls tree grep paths
---

## Syntax

    find [-H] [-L] [-P] [PATH...] [EXPRESSION]

    find                          every file and directory below this one
    find DIRECTORY -name PATTERN  those whose name matches PATTERN
    find . -type f                files only; -type d directories
    find . -name PATTERN -delete  delete what matches
    find . -name PATTERN -exec COMMAND {} +
                                  run COMMAND on what matches

## Examples

The Markdown files below this directory:

    find . -name '*.md'

The text files that hold a word, through `grep`:

    find . -type f -name '*.txt' -exec grep -l cat {} +

Only here, not in the directories below:

    find . -maxdepth 1 -type f -name '*.cfg'

The log files smaller than 2 KiB, and the text files changed today:

    find . -name '*.log' -size -2k
    find . -mtime -1 -name '*.txt'

Hand what is found to another command, names with spaces and all:

    find . -name '*.md' -print0 | xargs -0 wc -l

Delete the backups, or ask before each one:

    find . -name '*.bak' -delete
    find . -name '*.bak' -ok rm {} \;

Run a command in each file's own directory:

    find . -name '*.md' -execdir wc -l {} +

Leave a directory out, here `node_modules`, and search the rest:

<!-- parse only: needs a project's directories -->

    find . -name node_modules -prune -o -name package.json -print

The Rust files under `src` with a TODO in them:

<!-- parse only: needs a project's directories -->

    find src -type f -name '*.rs' -exec grep -l TODO {} +

## How it works

`find` walks each PATH, the current directory without one, and every directory below
it, and for each file and directory it meets it works out the expression from left to
right. A test (`-name`, `-type`, `-size`) is true or false; an action (`-print`,
`-delete`, `-exec`) does something and is true. Without an action, every file the
expression is true for is printed, as `-print` prints it: the path from where the
walk started, `./notes.txt`.

Tests side by side must all be true: `-type f -name '*.md'` is `-type f -a -name
'*.md'`. `-o` is "or", `!` is "not", and parentheses group; quote them, as the shell
reads them otherwise: `find . \( -name '*.md' -o -name '*.txt' \)`.

### Patterns

`-name` matches a file's name, its last part, against a glob: `*`, `?` and `[abc]`.
Quote the pattern, or the shell expands it first, in the current directory. `-path`
matches the whole path as printed, `./src/lib.rs`, and its `*` crosses `/`.

### Numbers

`-size` and `-mtime` take a number with an optional sign: `+N` more than N, `-N` less
than N, `N` exactly N. `-size` counts in 512-byte blocks unless a unit follows (`c`
bytes, `k` KiB, `M` MiB, `G` GiB); `-mtime` counts days since the last change, so
`-mtime -1` is changed within a day, `-mtime +30` more than 30 days ago.

## Options

### Operators, strongest first

    ( EXPR )          EXPR, grouped.
    ! EXPR            True when EXPR is false; -not EXPR too.
    EXPR1 EXPR2       Both; -a and -and say it too. EXPR2 is not tried when
                      EXPR1 is false.
    EXPR1 -o EXPR2    Either; -or too. EXPR2 is tried only when EXPR1 is
                      false.

### Options

    -H, -P            Do not follow links while walking (the default).
    -L                Follow links and junctions while walking; -follow too.
    -depth            A directory's contents before the directory itself.
    -maxdepth LEVELS  Go at most LEVELS directories below each PATH.
    -mindepth LEVELS  Test nothing less than LEVELS below each PATH.

### Tests

    -name PATTERN     The name, its last part, matches PATTERN.
    -iname PATTERN    The same; names match without regard to case anyway.
    -path PATTERN     The whole path as printed matches; -wholename too.
    -ipath PATTERN    The same, case aside.
    -type T           Of kind T: f a file, d a directory, l a link.
    -size N[bckMG]    Its size, in 512-byte blocks or the unit given, is N.
    -mtime N          It last changed N days ago, counted in whole days.
    -newer FILE       It changed later than FILE did.
    -empty            An empty file, or a directory with nothing in it.
    -true, -false     Always true, or always false.

### Actions

    -print            Print the path and a newline (the default action).
    -print0           Print the path and a NUL, for xargs -0.
    -delete           Delete it, a directory only empty; implies -depth.
    -prune            Do not go into this directory.
    -quit             Stop at once.
    -exec CMD ;       Run CMD for each file, {} standing for its path.
    -exec CMD {} +    Run CMD once for as many paths as fit.
    -execdir CMD ;    The same, run in the file's own directory.
    -ok CMD ;         Ask first, for each file; -okdir in its directory.

Exit status: 0 when every file was reached; 1 when a file could not be read, a
command run by `-exec ... +` failed, or the expression is wrong.

## Advanced

### Running commands

`-exec COMMAND {} ;` runs COMMAND once for each file, `{}` replaced by its path; the
`;` must be quoted or escaped, `\;`, as it ends a command in the shell. `-exec COMMAND
{} +` runs it once for many files at a time, which is much faster; when a run
fails, `find` exits 1. `-execdir` runs it in the directory of the file, with
`./name` for `{}`, which is safer when the tree may change under you. `-ok` and
`-okdir` ask first, on standard error, and run the command on an answer starting
with `y`.

### Pruning

`-prune` stops `find` going into a directory; with `-o` it leaves a directory out of
the search and keeps the rest: `find . -name .git -prune -o -type f -print`. Without
the `-print` at the end, the pruned directory is printed too.

### Deleting

`-delete` removes what the expression is true for, so try the expression with
`-print` first. It works depth first, so a directory emptied by it can be deleted in
the same run.

## Different in cash

- On a clean Windows machine, `find` is `C:\Windows\System32\find.exe`, a different
  command that searches for text inside files; a script's `find . -name '*.log'` meets
  it with `FIND: Parameter format not correct`. Inside cash, `find` is this one;
  `help` marks it as hiding a Windows program.
- `-name` and `-path` ignore case, as the file system does, so `-iname` and `-name`
  agree: `find . -name 'README*'` finds `readme.md`.
- Paths print as you spelled the start, `./a.txt` or `C:/src/a.txt`, never with
  backslashes, so `find | xargs` keeps them whole.
- `-H` is taken as `-P`: Windows' links are followed only with `-L`.
- What `-exec` runs is in the job, as every command is: Ctrl-C stops it too.
- Tests GNU's find has beyond these (`-regex`, `-printf`, `-user`, ...) are refused by
  name rather than ignored: a `find` that dropped part of the expression would return
  the wrong files.
