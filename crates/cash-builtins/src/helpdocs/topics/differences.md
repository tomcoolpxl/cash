---
title: Where cash differs from Bash and Git Bash
summary: Every place cash knowingly behaves unlike Bash 5.3 or Git Bash, each with its reason.
see: paths crlf job-control elevation vars config
spec: D2 D3 D5 D16 D20 D49 D70
---
## Why there are differences at all

cash runs unmodified Bash scripts (D2), but it runs them on Windows, as a native
program, not on an emulated Unix as Git Bash does. Where Windows has no equivalent, or
where Bash's behaviour would break Windows files or programs, cash differs, on purpose
and in the fewest places it can. The full table with its history is section 4 of
`spec.md`; the numbers below are its rows.

## Paths and names

- `pwd` prints `C:/src`, not `/c/src`: printed paths become arguments to Windows
  programs (D3, row 1). `$PATH` alone reads `/c/...`, because its entries are separated
  by colons (D5, row 2).
- Globbing and `find -name` ignore case, as the file system does; `shopt -u nocaseglob`
  turns that off (D16, row 3).
- Variable names ignore case: `$Path` is `$PATH` (D31, row 6).
- At the prompt, an unquoted `C:\...` keeps its backslashes (`shopt winpaths`; off in
  scripts) (D53, row 30).
- `sh`, `bash` and `cash` always run cash, however they are started: otherwise `bash`
  is WSL's launcher and the script goes on under Linux (D7, row 17). Started as `sh`,
  cash runs in POSIX mode, as Bash does.
- `$SHELL` names cash: `make`, `npm` and editors read it to choose a shell (row 21).
- `which ls` prints `C:/.../cash.exe/ls`, a path to no file that cash runs as `ls`: a
  builtin has no file, and scripts run what `which` prints (D58, row 36).
- `uname -s` is `Windows_NT` and `$OSTYPE` is `windows`, not `MINGW*` or `msys`: cash is
  not MSYS (row 25).
- In a folder whose path is over 258 characters, programs start in its 8.3 short name
  (D29, row 45).

## Line endings

- `\r\n` ends a line in `$(...)`, `read`, `mapfile` and here-documents: Python, .NET and
  `cmd` write CRLF (D20, row 4).
- The bundled `sed` and `awk` keep CRLF files CRLF and match their lines without the CR,
  so `sed 's/.$//'` removes the last visible character, not the CR; naming `\r` or
  `CASH_EOL=lf` gives Linux's behaviour (D49, row 26). `rev` keeps a CRLF line's CR at
  its end (row 33).

## Files and permissions

- `test -x` needs an execute permission and an executable kind of file (an extension in
  `PATHEXT`, or a `#!` line): Windows grants execute on every file (D23, row 7).
- `chmod` changes only the read-only attribute; `chmod -x` and the other bits Windows
  cannot store warn or say nothing, and return 0 (D23, D34, rows 8 and 18).
- `id`, `$UID` and `$EUID` give the account's RID, and 0 in an elevated shell, so
  `[ "$EUID" -eq 0 ]` means "running as Administrator" (row 20).
- Descriptors above 2 reach builtins and cash's own tools, not Windows programs (D26,
  row 5). `/dev/stdout` and `/dev/fd/N` share the descriptor rather than reopen the
  file (D7, row 39).
- `[ -s file ]` is false for App Execution Aliases, which are empty files (D46, row 11).
- A bundled tool cannot delete the shell's current folder: it runs as a process in that
  folder, and Windows keeps a process's folder (D48, row 13).

## Processes, jobs and signals

- Elevated and `detach`ed programs, and GUI programs on an orderly exit, outlive cash;
  everything else it started does not (D6, D42, D45, row 12). `disown` forgets a job
  but cannot take it out of cash's job object (row 23).
- `sudo`, `su` and `sudoedit` are cash's own, over gsudo, Windows' `sudo` or UAC: there
  is no root account, `root` means this account elevated, and an elevated or other
  user's shell is always a new cash, as Windows raises no running process (D45; `help
  elevation`).
- `kill -STOP` and Ctrl-Z suspend a program's threads; Windows has no `SIGSTOP` (D19,
  row 9).
- `kill -TERM` ends a console program at once, and gives a program with a window five
  seconds after asking it to close (D21, row 31). `kill 0` reaches the jobs cash
  started, and `kill -1` is refused (D22, rows 15 and 16).
- `$!` of a background job that starts no program is a number of cash's own (4n + 1),
  since the job runs on a thread, not a forked process (D70, row 14).
- A `CHLD` trap runs once per child process cash reaps; builtins and bundled tools run
  none (D64, row 38).
- A program on `PATH` whose reader went away ends as it chooses, not with 141; cash's
  own tools end with 141 (D71, row 22).
- `time` and `times` count a child that is still running (row 42).
- `pgrep`, `pkill`, `pidof` and `killall` match names without case and with `.exe`
  optional, and never signal the shell or system processes (D54, row 32).
- `>(...)` hands a builtin a named pipe and a program a temp file read as it is
  written (D17, row 10).

## Tools

- `bc` is POSIX bc: GNU bc's extensions are errors that say so, and a failed
  calculation exits 1 (D56, row 34).
- `ping` is Linux's, not `ping.exe` (D57, row 35); `find`, `sort`, `timeout`, `more`
  and others also hide Windows programs of the same name (`help` marks them).
- `fuser`, `lsof` and `ss` show what Windows' tables hold: no queue sizes or working
  folders, and `lsof`'s FD is 0 to 2 for the standard handles and a handle value, not a
  descriptor, for the rest (D50, D51, rows 28 and 29). Unelevated, `lsof` lists the files of your own processes; `sudo lsof` lists all.
- `ls` colours when it writes to a terminal, with no alias needed (row 24). `stat` is
  cash's own, and `which` knows about builtins (rows 19 and 36).
- `clear` and `reset` ignore terminfo; `getopt -s csh` is refused (D55, row 33).
- `awk`'s `system()` and pipes, and sed's `e`, run cash, not `/bin/sh`.
- A bundled tool is a process of its own: a crash cannot take the shell with it, at
  the cost of a process start per call.

## The shell itself

- Arithmetic, `unset`, `read`, `printf -v` and `declare` never run a `$(...)` hidden in an
  array subscript inside a value: Bash does, a well-known injection hole (rows 27 and
  37).
- Functions nest at most 500 deep when `FUNCNEST` is unset; Bash crashes somewhere past
  600 (row 41).
- History is not cut while `HISTSIZE` and `HISTFILESIZE` are unset, and lives in
  `~/.cash_history` (row 43).
- `\v` and `\V` in a prompt give cash's version; `$BASH_VERSION` is Bash's (row 44).
- Tab completes a typed `*` or `?` as a glob: no Windows file name can hold them (D40,
  row 40).
- `exec -a NAME` and `exec -l` are refused for programs other than cash: Windows cannot
  give a program an `argv[0]` of its own.

## Not there

- vi editing mode is reedline's: Readline's vi keymaps, without its `vi-insert`/
  `vi-command` names for `bind -m`.
