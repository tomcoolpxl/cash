---
see: alias keys
---
## Description

An abbreviation, fish's idea, is replaced on the line as you type: after `abbr -a gco
git checkout`, typing `gco` and then Space or Enter turns it into `git checkout` on
screen, and that is what runs and what the history keeps. An alias is replaced only when
the command runs.

An abbreviation expands only as the command word (first on the line, or after `|`, `;`,
`&&` and the like), unless it was added with `--position anywhere`, and never inside
quotes. Pasted text never expands.

## Examples

```
abbr -a gco git checkout
abbr -a --position anywhere -- -h --help
abbr                     # list them, as lines that define them again
abbr -e gco              # remove one
```

## In a shared .bashrc

`abbr` is cash's own; Bash does not have it. Guard it, or put it in `~/.cashrc`:
`command -v abbr >/dev/null && abbr -a gco git checkout`.
