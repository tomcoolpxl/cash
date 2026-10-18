---
name: xargs
summary: Build and run commands from standard input.
kind: Text and file tools
see: find
---

## Syntax

    xargs [OPTION]... [COMMAND [ARG]...]

    ... | xargs COMMAND          COMMAND with the input's words
    ... | xargs -n 1 COMMAND     once for each word
    ... | xargs -I{} CMD {} x    once for each line, put where {} is
    find ... -print0 | xargs -0 CMD    names that may hold any character

## Examples

The input's words as arguments to one command:

    printf '%s\n' a b c | xargs echo

One command for each word, or for each two:

    printf '%s\n' a b c | xargs -n 1 echo got
    printf '%s\n' a b c d | xargs -n 2 echo pair

Each line put where `{}` stands:

    printf '%s\n' one two | xargs -I{} echo 'item {} done'

With `find`, names that hold spaces kept whole:

    mkdir -p 'my dir' && touch 'my dir/a b.txt'
    find 'my dir' -name '*.txt' -print0 | xargs -0 ls

Nothing to run when the input is empty:

    printf '' | xargs -r echo never

## How it works

`xargs` reads words from its standard input, separated by blanks and newlines (quotes
and backslashes keep a word whole), and runs COMMAND with as many of them as fit on a
command line, then again with the rest. Without a COMMAND it runs `echo`.

`-n N` gives each run at most N words; `-L N` at most N lines. `-I STR` runs COMMAND
once for each line, the line put in place of each STR in the arguments. `-0` reads
words separated by NUL bytes, what `find -print0` writes, so a name may hold blanks,
quotes or newlines.

`-P N` runs up to N commands at once. `-r` runs nothing when the input is empty.

## Options

    -n, --max-args=N          At most N words a command.
    -L, --max-lines=N         At most N lines a command.
    -I STR                    Once a line, the line in place of STR.
    -0, --null                Words end in a NUL byte.
    -d, --delimiter=C         Words end in C.
    -P, --max-procs=N         Run up to N commands at once.
    -r, --no-run-if-empty     Run nothing when there are no words.
    -t, --verbose             Write each command before running it.
    -p, --interactive         Ask before each command.
    -s, --max-chars=N         At most N characters a command line.
    -a, --arg-file=FILE       Read the words from FILE.

Exit status: 0 on success; 123 when a command exited 1 to 125; 124 when one exited
255; 125 when one was killed; 126 when the command cannot be run; 127 when it is not
found.

## Advanced

### Shell code for each item

`xargs` runs a program, not shell code. For a pipeline or a function, run a shell:
`xargs -I{} sh -c 'echo "$1" | wc -c' _ {}`, the item passed as an argument rather
than pasted into the code.

## Different in cash

- It is cash's own `xargs`, carried inside cash, with GNU's options and statuses.
- Commands cash carries (`ls`, `grep`, ...) run as cash runs them, without starting a
  program; a shell function is not run, as in Bash.
- A command line is at most 32,767 characters, Windows' limit; `xargs` splits the
  words to fit.
