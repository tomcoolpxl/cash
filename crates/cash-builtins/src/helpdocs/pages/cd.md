---
see: pwd pushd prevd winpath paths
spec: D3 D10 D53 D62
---
## Description

`cd [DIR]` changes the shell's working folder: to `$HOME` without an argument, to the
previous folder with `cd -`. `-P` resolves links, `-L` (the default) keeps them.

## Windows notes

- Any spelling works: `cd C:/src`, `cd /c/src`, `cd "C:\src"`. `pwd` and `$PWD` then
  say `C:/src` (D3).
- Unquoted, `cd C:\Users\me` reaches `cd` as `C:Usersme` in a script, because backslash
  is Bash's escape. At the prompt `shopt winpaths` keeps the backslashes (D53), and a
  failed `cd` to such a mangled path says so.
- Each `cd` is recorded in the folder history: `prevd`, `nextd`, `cdh`, and Alt-Left /
  Alt-Right on an empty line (D62).
- `CDPATH` is not used.
