---
title: Paths: C:/ vs /c/ vs C:\
summary: How cash spells paths, which spellings it accepts, and why `$PATH` is different.
see: winpath cd pwd ln vars
spec: D3 D4 D5 D10 D16 D27 D28 D29 D31 D53
---
## The short version

cash prints every path as `C:/Users/me`: drive letter, forward slashes. Write paths that
way too, or as a quoted `"C:\Users\me"`: every program understands both. `pwd`, `$PWD`,
`~` and the paths cash builds all come out as `C:/...`. The Unix spelling `/c/Users/me`
works where cash itself opens the path, but not as an argument to other commands.

## Why forward slashes and a drive letter

A path cash prints usually becomes an argument to a Windows program, where the wrong
spelling is fatal: `terraform -chdir="$(pwd)/modules"` must work, and `/c/src` means
nothing to a native `.exe`. Windows' file APIs take `/` as a separator, so `C:/src` works
everywhere `C:\src` does (D3). The cost: `case $PWD in /c/*)` no longer matches.

## Unix spellings

`/c/src`, `/tmp` (your `TEMP` folder), `/dev/null`, `/dev/tty`, `/dev/stdin` and the
other `/dev` names work wherever cash itself opens the path (D10): in redirections
(`> /tmp/out`, `2> /dev/null`), `cd`, `source`, `test` and `[`, and cash's own builtins
such as `ls`. An argument to any other command reaches it as written, and a Windows
program, or a bundled tool such as `cat` or `rm`, has no `/c` or `/tmp` (D4); cash
warns when it sees one and names the `C:/` spelling. `/dev/null` is the null device; a
bare `nul` is the device too, as every Windows program sees it (D28).

## Backslashes

Backslash is Bash's escape character, so an unquoted `cd C:\Users\me` reaches `cd` as
`C:Usersme` in a script. Quote it, and it works:

```
cd "C:\Users\me"
cd 'C:\Program Files'
dir=$(some-tool.exe --print-path)   # C:\foo\bar from a Windows tool
cd "$dir"
```

At the interactive prompt, `shopt winpaths` (on by default there, off in scripts) lets
a word that starts with a drive and a backslash keep its backslashes, so a path pasted
from Explorer works unquoted (D53). `shopt -u winpaths` turns it off.

## Arguments are never rewritten

cash never guesses which arguments are paths and rewrites them, the guessing MSYS2
needs `MSYS2_ARG_CONV_EXCL` to escape (D4). When an old tool needs backslashes, convert
on purpose: `old-tool.exe "$(winpath -w "$dir")"`. `winpath -u` gives `/c/...`, and
`winpath -c` cash's own `C:/...`.

## $PATH is the one exception

`$PATH` reads Unix-style, `/c/tools:/c/Windows`, because `:` separates its entries and a
drive letter has one too. So `PATH=/c/tools:$PATH` and `IFS=: read -ra dirs <<< "$PATH"`
work. Programs cash starts get the Windows form, with semicolons. No other variable is
translated: `GOPATH` or `PYTHONPATH` pass through as you wrote them (D5). Variable names
are case-insensitive, as in Windows: `$Path` is `$PATH` (D31).

## Long paths, globbing, links

- Paths of any length work in cash's own file operations, whatever the long-path
  setting (D29). Windows starts no program in a folder over 258 characters; cash starts
  it in the folder's 8.3 short name instead.
- Globbing ignores case, as the file system does: `*.sh` matches `Setup.SH`.
  `shopt -u nocaseglob` makes it case-sensitive (D16).
- `ln -s` makes a real symbolic link when Windows allows it (Developer Mode or an
  elevated shell), a junction for a folder otherwise, and fails for a file (D27).
