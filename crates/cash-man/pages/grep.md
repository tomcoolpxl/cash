---
names: grep egrep fgrep
summary: Print the lines of files that match a pattern, with GNU grep's options.
summary egrep: Print the lines that match an extended regular expression: `grep -E`.
summary fgrep: Print the lines that contain a fixed string: `grep -F`.
kind: Text and file tools
see: find sed awk crlf
---

## Syntax

    grep [OPTION]... PATTERNS [FILE]...
    grep [OPTION]... -e PATTERNS ... [FILE]...
    grep [OPTION]... -f PATTERN-FILE ... [FILE]...

    grep WORD FILE           the lines of FILE that hold WORD
    grep -r WORD DIRECTORY   the same in every file under DIRECTORY
    COMMAND | grep WORD      the lines COMMAND prints that hold WORD
    egrep, fgrep             grep -E and grep -F

## Examples

The lines that hold a word, with their numbers:

    grep -n cat notes.txt

How many lines hold it, or which files do:

    grep -c ERROR app.log
    grep -l 2025 *.md

Case aside, the word whole, not inside another:

    grep -i -w error app.log

Every line but comments and empty ones:

    grep -v -e '^#' -e '^$' settings.conf

Only the part that matched, here the numbers:

    grep -o '[0-9]\+' app.log

Search every file below a directory:

    grep -rn opt .

A test in a script: is it there?

    if grep -q ERROR app.log; then echo 'errors found'; fi

Search what a command prints:

<!-- parse only: needs a git repository -->

    git log --oneline | grep -i fix

## How it works

`grep` reads each file, or its standard input without one, a line at a time, and
prints each line that matches one of the patterns. With several files, each line is
preceded by its file's name. `-v` turns the choice round: the lines that do not
match.

Patterns are basic regular expressions by default: `.` any character, `*` repeats,
`[abc]` a class, `^` and `$` the start and the end of the line, `\(…\)` a group,
`\{m,n\}` a count, `\|` an alternative. `-E` takes extended ones, where the same are
written `(…)`, `{m,n}`, `|`, `+` and `?`; `-F` takes plain strings. Several patterns
(`-e` more than once, or lines of a file with `-f`) match if any one matches.

GNU's classes and operators work: `[[:alpha:]]`, `[[:digit:]]`, `\w` a word
character, `\<` and `\>` the edges of a word, `\b`, and back-references, `\(ab\)\1`.

The exit status says what happened, so a script can test it: 0 when a line was
chosen, 1 when none was, 2 on an error such as a missing file or a bad pattern. `-q`
prints nothing, only the status.

## Options


### Pattern selection and interpretation

    -E, --extended-regexp     PATTERNS are extended regular expressions.
    -F, --fixed-strings       PATTERNS are strings.
    -G, --basic-regexp        PATTERNS are basic regular expressions.
    -e, --regexp=PATTERNS     Use PATTERNS for matching.
    -f, --file=FILE           Take PATTERNS from FILE.
    -i, --ignore-case         Ignore case distinctions in patterns and data.
    --no-ignore-case          Do not ignore case distinctions (default)
    -w, --word-regexp         Match only whole words.
    -x, --line-regexp         Match only whole lines.
    -z, --null-data           A data line ends in 0 byte, not newline.

### Miscellaneous

    -s, --no-messages         Suppress error messages.
    -v, --invert-match        Select non-matching lines.
    -V, --version             Display version information and exit.
    --help                    Display this help text and exit.

### Output control

    -m, --max-count=NUM       Stop after NUM selected lines.
    -b, --byte-offset         Print the byte offset with output lines.
    -n, --line-number         Print line number with output lines.
    --line-buffered           Flush output on every line.
    -H, --with-filename       Print file name with output lines.
    -h, --no-filename         Suppress the file name prefix on output.
    --label=LABEL             Use LABEL as the standard input file name
                              prefix.
    -o, --only-matching       Show only nonempty parts of lines that match.
    -q, --quiet, --silent     Suppress all normal output.
    --binary-files=TYPE       Assume that binary files are TYPE; TYPE is
                              'binary', 'text', or 'without-match'.
    -a, --text                Equivalent to --binary-files=text.
    -I                        Equivalent to --binary-files=without-match.
    -d, --directories=ACTION  How to handle directories; ACTION is 'read',
                              'recurse', or 'skip'.
    -D, --devices=ACTION      How to handle devices, FIFOs and sockets;
                              ACTION is 'read' or 'skip'.
    -r, --recursive           Like --directories=recurse.
    -R, --dereference-recursive
                              Likewise, but follow all symlinks.
    --include=GLOB            Search only files that match GLOB (a file
                              pattern)
    --exclude=GLOB            Skip files that match GLOB.
    --exclude-from=FILE       Skip files that match any file pattern from
                              FILE.
    --exclude-dir=GLOB        Skip directories that match GLOB.
    -L, --files-without-match
                              Print only names of FILEs with no selected
                              lines.
    -l, --files-with-matches  Print only names of FILEs with selected lines.
    -c, --count               Print only a count of selected lines per FILE.
    -T, --initial-tab         Make tabs line up (if needed)
    -Z, --null                Print 0 byte after FILE name.

### Context control

    -B, --before-context=NUM  Print NUM lines of leading context.
    -A, --after-context=NUM   Print NUM lines of trailing context.
    -C, --context=NUM         Print NUM lines of output context.
    -NUM                      Same as --context=NUM.
    --group-separator=SEP     Print SEP on line between matches with
                              context.
    --no-group-separator      Do not print separator for matches with
                              context.
    --color[=WHEN],
    --colour[=WHEN]           Use markers to highlight the matching strings;
                              WHEN is 'always', 'never', or 'auto'.
    -U, --binary              Do not strip CR characters at EOL
                              (MSDOS/Windows)

`PATTERNS` may hold several patterns, a line each. `FILE` is a file to read; `-` or no
`FILE` is standard input (with `-r`, the current directory).

Exit status: 0 when a line was selected; 1 when none was; 2 when an error occurred,
unless `-q` was given and a line was selected.

## Advanced

### Context

`-A N`, `-B N` and `-C N` print N lines after, before, or around each match, with
`--` between groups that are not next to each other: `grep -C 2 panic log.txt`.

### Which files

With `-r`, `--include='*.rs'` searches only the files whose names match,
`--exclude='*.min.js'` leaves some out, and `--exclude-dir=target` leaves out a
directory and all below it. `-l` names the files that match, `-L` those that do not,
`-c` counts the lines in each.

### Binary files

A file with NUL bytes is binary: `grep` says that it matches rather than printing the
line. `-a` treats it as text, `-I` leaves it out.

## Different in cash

- A line that ends in CRLF is matched without its `\r`: `grep 'x$'`, `-x` and `-w`
  work on a Windows text file, and `-o` never prints the `\r`. The line is still
  printed as it was read, ending in CRLF. `-U` keeps the `\r` in the line, as GNU grep
  on Linux has it. A pattern file read with `-f` may end its lines in CRLF too.
- Patterns are read as the bundled `sed` reads its own, so the two tools take one
  dialect and give the same messages for a pattern that is malformed. `-P` (Perl
  syntax) is not carried: it says so, and `-E` covers most of what it is used for.
- The engine is ripgrep's, so a search over a large tree is fast; `-r` reads the
  files under a directory in name order.
- File names print with `/`, as every path cash prints: `grep -r foo src` names
  `src/main.rs`, and `grep -r foo` with no file names `main.rs` without a `./`.
- `--include`, `--exclude` and `--exclude-dir` match names as GNU grep does, case
  included: `--include='*.rs'` leaves out `MAIN.RS`.
- `--color=auto` colours when standard output is the terminal; `GREP_COLORS` sets the
  colours as for GNU grep.
- On a clean Windows machine `grep` is not found at all, and with Git for Windows it
  is Git's copy; inside cash, `grep` is this one. `type -a grep` lists the others.
