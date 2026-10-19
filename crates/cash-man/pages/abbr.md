---
name: abbr
summary: Manage fish-style abbreviations, expanded as you type.
kind: cash's shell commands
see: alias keys config
---

## Syntax

    abbr [-a] [--position command|anywhere] NAME EXPANSION...
    abbr -e NAME...
    abbr [-s] | -l
    abbr -q NAME...
    abbr -r OLD NEW

    abbr -a gco git checkout    gco becomes git checkout as you type
    abbr                        every abbreviation, as abbr -a lines
    abbr -e gco                 remove one, from the file too

## Examples

Added, listed by name and in full, asked about, renamed and removed:

    abbr -a gco git checkout
    abbr -a --position anywhere -- -h --help
    abbr -l
    abbr
    abbr -q gco && echo "gco is an abbreviation"
    abbr -r gco gch
    abbr -e gch

## How it works

An abbreviation, fish's idea, is replaced on the line as you type: after `abbr -a gco
git checkout`, typing `gco` and then Space or Enter turns it into `git checkout` on
screen, and that is what runs and what the history keeps. An alias is replaced only when
the command runs.

An abbreviation expands only as the command word (first on the line, or after `|`, `;`,
`&&` and the like), unless it was added with `--position anywhere`, and never inside
quotes. Pasted text never expands.

### Kept across sessions

`abbr -a` once is enough: `-a`, `-e` and `-r` also write `%APPDATA%\cash\abbreviations`,
one `NAME=EXPANSION` per line (`anywhere NAME=EXPANSION` for `--position anywhere`),
and every interactive shell reads it after `~/.bashrc`, so a definition in the rc file
wins over the file's for the same name. The file is state, not configuration:
`--no-config` leaves it alone. A line that is not `NAME=EXPANSION` is reported once at
startup and the file ignored until it is mended; a name with `=` in it, or an expansion
with a newline, is kept for the session only. A shell that runs a script or `-c` does
not read the file, since nothing expands there.

### In a shared .bashrc

`abbr` is cash's own; Bash does not have it. Guard it, or put it in `~/.cashrc`:
`command -v abbr >/dev/null && abbr -a gco git checkout`. Or define it once at the
prompt and let the file keep it.
## Options

    -a, --add                  Add NAME, expanding to EXPANSION.
    --position WHERE           command (the default) or anywhere.
    -e, --erase                Remove each NAME.
    -s, --show                 Every one, as abbr -a lines: the default.
    -l, --list                 The names alone.
    -q, --query                Whether each NAME is an abbreviation.
    -r, --rename               Rename OLD to NEW.

Exit status: 0; 1 when `-q` finds a NAME that is not one, or `-e` a NAME to remove.

