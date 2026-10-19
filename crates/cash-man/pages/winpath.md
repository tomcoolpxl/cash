---
name: winpath
summary: Convert a path between Windows and Unix spellings.
kind: Windows commands
see: paths cd pwd
---

## Syntax

    winpath [-w | -u | -c] [PATH]...

    winpath -w ~/src      C:\Users\me\src, for a tool that wants backslashes
    winpath -u 'C:\x'     /c/x
    winpath PATH          C:/x, cash's own spelling

## Examples

The three spellings, and paths from standard input:

<!-- parse only: Git Bash has no winpath to compare with -->

    winpath -w /c/Users
    winpath -u 'C:\Program Files'
    winpath 'C:\x\y'
    printf '/c/a\n/c/b\n' | winpath -w

## How it works

`winpath` converts paths between the spellings Windows and Unix tools expect: `-w` gives
`C:\Users\me` with backslashes, `-u` gives `/c/Users/me`, and `-c`, the default, gives
cash's own `C:/Users/me`. With no paths, it converts each line of standard input.

### Why it exists

cash never rewrites a command's arguments on its own: guessing which arguments are
paths is how MSYS2 came to need `MSYS2_ARG_CONV_EXCL`. Most Windows programs take
`C:/src` as it is. When an old tool insists on backslashes, convert on purpose.
## Options

    -w    Windows' spelling, with backslashes: C:\foo.
    -u    The Unix spelling: /c/foo.
    -c    cash's own spelling: C:/foo. The default.

Exit status: 0, or 2 for an option it does not know.

