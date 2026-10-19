---
name: dircolors
summary: Print commands that set `LS_COLORS`.
kind: Bundled coreutils, from uutils
see: ls
---

## Syntax

    dircolors [OPTION]... [FILE]

    eval "$(dircolors)"          LS_COLORS set to the default colours
    eval "$(dircolors ~/.dircolors)"   to the colours a file of yours gives
    dircolors -p > ~/.dircolors  the defaults, to change in a file

## Examples

The command it prints, and the defaults it starts from:

    dircolors -b | head -1 | cut -c1-12
    dircolors -p | head -1

## How it works

`dircolors` prints a shell command that sets `LS_COLORS`, the variable `ls` reads for
the colour of each kind of file, from its defaults or from FILE; `eval "$(dircolors)"`
in `~/.bashrc` runs it. `-b` prints it for Bash and sh, `-c` for csh. `-p` prints the
defaults in the file's format, a line for each kind of file and extension, to copy and
change. `--print-ls-colors` shows each entry in its own colour.

## Options

    -b, --sh, --bourne-shell    A command for Bash and sh: the default.
    -c, --csh, --c-shell        A command for csh.
    -p, --print-database        The defaults, as a file.
    --print-ls-colors           Each entry in its own colour.

Exit status: 0, or 1 for a FILE that cannot be read.
