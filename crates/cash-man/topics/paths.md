---
title: Paths: C:/ vs /c/ vs C:\
summary: How cash spells paths, which spellings it accepts, and why `$PATH` is different.
see: winpath cd pwd ln vars
---
## The short version

cash prints every path as `C:/Users/me`: drive letter, forward slashes. Write paths that
way too, or as a quoted `"C:\Users\me"`: every program understands both. `pwd`, `$PWD`,
`~` and the paths cash builds all come out as `C:/...`. The Unix spelling `/c/Users/me`
works where cash and its own tools open the path, but not as an argument to a Windows
program. `shopt -s unixpaths` spells paths as Git Bash does, `/c/Users/me`, and hands a
Windows program `C:/...` for them (below).

## Why forward slashes and a drive letter

A path cash prints usually becomes an argument to a Windows program, where the wrong
spelling is fatal: `terraform -chdir="$(pwd)/modules"` must work, and `/c/src` means
nothing to a native `.exe`. Windows' file APIs take `/` as a separator, so `C:/src` works
everywhere `C:\src` does. The cost: `case $PWD in /c/*)` no longer matches.

## Unix spellings

`/c/src`, `/tmp` (your `TEMP` directory), `/dev/null`, `/dev/tty`, `/dev/stdin` and the
other `/dev` names work wherever cash itself opens the path: in redirections
(`> /tmp/out`, `2> /dev/null`), `cd`, `source`, `test` and `[`, globs, and cash's own
builtins such as `ls`. A glob gives its matches in cash's spelling, so
`cat /c/logs/*.txt` hands `cat` `C:/logs/a.txt`. cash's own tools read them too, as an
MSYS tool does: `cat /c/logs/a.txt`, `cp /c/a /c/b` and `sed -n p /c/x` open the files,
while a `sed` or `awk` script is never touched, and `dirname /c/x/y` gives back `/c/x`.
Every drive letter works so, `/d/data` for `D:/data` and `/z/` for a network drive: a
one-letter name after the first `/` is always a drive, as in Git Bash, so a directory
`C:\d` is reached as `/c/d`, never `/d`; and `/tmp` is always your `TEMP` directory, not a
`C:\tmp` that may also be there.
An argument to a Windows program reaches it as written, and it has no `/c` or `/tmp`;
cash warns when it sees one and names the `C:/` spelling (an MSYS program, such as Git's
`usr/bin` tools, reads it itself). `/dev/null` is the null device; a bare `nul` is the
device too, as every Windows program sees it.

`/dev/tcp/HOST/PORT` and `/dev/udp/HOST/PORT` open a socket, as in Bash, in a
redirection only: `exec 3<>/dev/tcp/example.com/80` makes descriptor 3 a connection to
write to (`>&3`) and read from (`<&3`), `cat </dev/tcp/host/port` reads one, and `exec
3>&-` closes it. `PORT` may be a service name (`http`, `https`, `ssh`). A connection that
fails is reported in Bash's two lines, `connect: Connection refused` and the
redirection's own; as an argument to a program the name is a file that does not exist.

A command run by a Unix system path, `/bin/rm` or `/usr/bin/printf`, is cash's own
command of that name when no such file is there, as a script written for Unix expects:
`/bin/mv a b`, `/usr/bin/env python3`, `[ -x /bin/rm ]`, and the same after `exec`,
`xargs`, `timeout` and `env`. `/bin/sh` and `/bin/bash` are cash. A name cash has no
command for is not found.

## Git Bash's spelling: `shopt -s unixpaths`

Some scripts need paths spelled as Git Bash spells them. rbenv, pyenv, nvm and asdf look
for a file from `$PWD` up, cutting `dir=${dir%/*}` until it is empty or `dirname` until it
is `/`, which `C:/...` never reaches; bats' `load` takes a path for absolute only when it
starts with `/`. With `shopt -s unixpaths`, off unless turned on:

- `pwd`, `$PWD`, `$OLDPWD`, `$HOME`, `~`, `dirs`, `$DIRSTACK`, the prompt's `\w`, `which`,
  `type -P`, `command -v`, `hash`, a glob from an absolute path, `realpath`, `readlink -f`
  and `mktemp` give `/c/...`; `pwd -W` still gives `C:/...`. A path the script wrote
  stays as written: `$BASH_SOURCE`, the `$0` of a script run by its path, `find`'s
  output.
- A Windows program that is not an MSYS one is handed `C:/...` for a whole-word `/c/...`
  argument and for the value of `--opt=/c/...` or `NAME=/c/...`, as Git Bash hands it;
  `HOME`, `PWD` and `OLDPWD` reach it as `C:/...` too. Nothing else is rewritten: a list
  (`/c/a:/c/b`), a bare `/c` (`cmd /c`) and a root alone (`/a/`) go as written, and
  `//c/x` gives the program a literal `/c/x`. An MSYS program and cash get `/c/...` as
  written.
- A cash started from it, `bash script.sh` too, starts with the option on
  (`CASH_UNIXPATHS=1` in its environment); `shopt -u unixpaths` turns it off for this
  shell and the ones it starts.

Turning the option on or off spells `$PWD`, `$OLDPWD` and `$HOME` again at once.

## Backslashes

Backslash is Bash's escape character, so an unquoted `cd C:\Users\me` reaches `cd` as
`C:Usersme` in a script. Quote it, and it works:

    cd "C:\Users\me"
    cd 'C:\Program Files'
    dir=$(some-tool.exe --print-path)   # C:\foo\bar from a Windows tool
    cd "$dir"

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
translated: `GOPATH` or `PYTHONPATH` pass through as you wrote them. `$Path` is
`$PATH`, as in Windows, and `Path=x` sets it; an all-lower-case `path` is a script's
own variable, as in Bash.

An entry keeps the spelling it was given. The ones inherited from Windows read `/c/...`,
but `PATH="$PWD/bin:$PATH"` puts `C:/src/bin` in, since `$PWD` is `C:/...`. Programs are
still found there, as cash knows a drive in an entry; but a script that splits `PATH`
on `:` (`IFS=: read -ra`, `${PATH%%:*}`, awk with `RS=:`) gets `C` as an entry of its
own. Such a script adds a directory in the Unix spelling:

    PATH="$(winpath -u "$PWD")/bin:$PATH"

Spelling every `C:/` entry `/c/` when `PATH` is assigned was turned down: the idiom that
adds a directory once, `case ":$PATH:" in *":$HOME/bin:"*)`, would then never find what
it added, as `$HOME` is `C:/...`.

## Long paths, globbing, links

- Paths of any length work in cash's own file operations, whatever the long-path
  setting. Windows starts no program in a directory over 258 characters; cash starts
  it in the directory's 8.3 short name instead.
- Globbing ignores case, as the file system does: `*.sh` matches `Setup.SH`.
  `shopt -u nocaseglob` makes it case-sensitive.
- `ln -s` makes a real symbolic link when Windows allows it (Developer Mode or an
  elevated shell), a junction for a directory otherwise, and for a file a copy, with a
  warning, as Git Bash's `ln` copies it.
