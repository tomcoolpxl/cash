---
names: less more
see: cat head tail
---
## Description

`less` pages through files or standard input: Space (or `f`) and `b` a page forward and
back, Enter or `j` and `k` a line, `g` and `G` the start and end, `/` to search and `n`
for the next match, `q` to quit. `more` is the same pager with `more`'s habits: it quits at the
end and prompts `--More--(45%)`.

When standard output is not a terminal, both are `cat`, as GNU less is, so
`cmd | less > out` writes the text and nothing else.

## Windows notes

`less` is not part of Windows, and comes with Git for Windows only. `more` on Windows
is `C:\Windows\System32\more.com`, the DOS tool with other options; inside cash, `more`
is this pager. `-R` and `-X` are accepted: cash passes colour through and never clears
the screen on exit.

## Examples

```
git log | less
less -N build.log
```
