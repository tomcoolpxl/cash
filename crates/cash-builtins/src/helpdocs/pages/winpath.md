---
see: paths cd pwd
spec: D3 D4 D45
---
## Description

`winpath` converts paths between the spellings Windows and Unix tools expect: `-w` gives
`C:\Users\me` with backslashes, `-u` gives `/c/Users/me`, and `-c`, the default, gives
cash's own `C:/Users/me`. With no paths, it converts each line of standard input.

## Why it exists

cash never rewrites a command's arguments on its own (D4): guessing which arguments are
paths is how MSYS2 came to need `MSYS2_ARG_CONV_EXCL`. Most Windows programs take
`C:/src` as it is. When an old tool insists on backslashes, convert on purpose.

## Examples

```
old-tool.exe "$(winpath -w "$PWD/build")"
winpath -u "C:\Program Files"          # /c/Program Files
cmd.exe /c dir "$(winpath -w ~)"
```
