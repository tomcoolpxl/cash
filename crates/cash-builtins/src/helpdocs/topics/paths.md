---
title: Paths: C:/ vs /c/ vs C:\
summary: How cash spells paths, which spellings it accepts, and why `$PATH` is different.
see: winpath cd pwd ln vars
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
everywhere `C:\src` does. The cost: `case $PWD in /c/*)` no longer matches.

## Unix spellings

`/c/src`, `/tmp` (your `TEMP` folder), `/dev/null`, `/dev/tty`, `/dev/stdin` and the
other `/dev` names work wherever cash itself opens the path: in redirections
(`> /tmp/out`, `2> /dev/null`), `cd`, `source`, `test` and `[`, globs, and cash's own
builtins such as `ls`. A glob gives its matches in cash's spelling, so
`cat /c/logs/*.txt` hands `cat` `C:/logs/a.txt`. An argument to any other command
reaches it as written, and a Windows
program, or a bundled tool such as `cat` or `rm`, has no `/c` or `/tmp`; cash
warns when it sees one and names the `C:/` spelling. `/dev/null` is the null device; a
bare `nul` is the device too, as every Windows program sees it.

`/dev/tcp/HOST/PORT` and `/dev/udp/HOST/PORT` open a socket, as in Bash, in a
redirection only: `exec 3<>/dev/tcp/example.com/80` makes descriptor 3 a connection to
write to (`>&3`) and read from (`<&3`), `cat </dev/tcp/host/port` reads one, and `exec
3>&-` closes it. `PORT` may be a service name (`http`, `https`, `ssh`). A connection that
fails is reported in Bash's two lines, `connect: Connection refused` and the
redirection's own; as an argument to a program the name is a file that does not exist.

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
a word that starts with a drive and a backslash, or with `\\` and a server's name, keep
its backslashes, so a path pasted from Explorer works unquoted: `cd C:\Users\me`,
`cd \\nas\share\docs`. `shopt -u winpaths` turns it off. Tab completes a path in the
spelling it was typed in, backslashes included.

## Network paths

`//server/share/dir` and `\\server\share\dir` (quoted in a script, or at the prompt
with `winpaths`) work wherever a drive path does: `cd`, redirections, globs, Tab and
Alt-E. `pwd` says `//server/share/dir`.

## Arguments are never rewritten

cash never guesses which arguments are paths and rewrites them, the guessing MSYS2
needs `MSYS2_ARG_CONV_EXCL` to escape. When an old tool needs backslashes, convert
on purpose: `old-tool.exe "$(winpath -w "$dir")"`. `winpath -u` gives `/c/...`, and
`winpath -c` cash's own `C:/...`.

## $PATH is the one exception

`$PATH` reads Unix-style, `/c/tools:/c/Windows`, because `:` separates its entries and a
drive letter has one too. So `PATH=/c/tools:$PATH` and `IFS=: read -ra dirs <<< "$PATH"`
work. Programs cash starts get the Windows form, with semicolons. No other variable is
translated: `GOPATH` or `PYTHONPATH` pass through as you wrote them. Variable names
are case-insensitive, as in Windows: `$Path` is `$PATH`.

## Long paths, globbing, links

- Paths of any length work in cash's own file operations, whatever the long-path
  setting. Windows starts no program in a folder over 258 characters; cash starts
  it in the folder's 8.3 short name instead.
- Globbing ignores case, as the file system does: `*.sh` matches `Setup.SH`.
  `shopt -u nocaseglob` makes it case-sensitive.
- `ln -s` makes a real symbolic link when Windows allows it (Developer Mode or an
  elevated shell), a junction for a folder otherwise, and fails for a file.
