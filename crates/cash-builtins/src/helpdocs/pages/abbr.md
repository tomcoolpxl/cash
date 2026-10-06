---
see: alias keys config
---
## Description

An abbreviation, fish's idea, is replaced on the line as you type: after `abbr -a gco
git checkout`, typing `gco` and then Space or Enter turns it into `git checkout` on
screen, and that is what runs and what the history keeps. An alias is replaced only when
the command runs.

An abbreviation expands only as the command word (first on the line, or after `|`, `;`,
`&&` and the like), unless it was added with `--position anywhere`, and never inside
quotes. Pasted text never expands.

## Kept across sessions

`abbr -a` once is enough: `-a`, `-e` and `-r` also write `%APPDATA%\cash\abbreviations`,
one `NAME=EXPANSION` per line (`anywhere NAME=EXPANSION` for `--position anywhere`),
and every interactive shell reads it after `~/.bashrc`, so a definition in the rc file
wins over the file's for the same name. The file is state, not configuration:
`--no-config` leaves it alone. A line that is not `NAME=EXPANSION` is reported once at
startup and the file ignored until it is mended; a name with `=` in it, or an expansion
with a newline, is kept for the session only. A shell that runs a script or `-c` does
not read the file, since nothing expands there.

## Examples

```
abbr -a gco git checkout
abbr -a --position anywhere -- -h --help
abbr                     # list them, as lines that define them again
abbr -e gco              # remove one, from the file too
```

## In a shared .bashrc

`abbr` is cash's own; Bash does not have it. Guard it, or put it in `~/.cashrc`:
`command -v abbr >/dev/null && abbr -a gco git checkout`. Or define it once at the
prompt and let the file keep it.
