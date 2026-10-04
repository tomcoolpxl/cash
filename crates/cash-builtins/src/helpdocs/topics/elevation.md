---
title: Elevation: sudo, su, elevate and UAC
summary: Running commands as Administrator or as another user, which tool does it, and what escapes cash's job.
see: sudo su sudoedit elevate detach id job-control
spec: D6 D42 D45
---
## Windows has no root

An Administrator account runs most programs unelevated; elevating a program asks UAC
for approval, and Windows raises no program that is already running. There is no root
account and no root password. `id -u`, `$UID` and `$EUID` are 0 in an elevated shell, so
`[ "$EUID" -eq 0 ]` still means "running elevated".

## Which command

- `sudo COMMAND`: the command elevated, in this terminal, with its output here, as on
  Unix. `sudo -i` and `sudo -s` give an elevated shell; `sudo -u USER COMMAND` runs it as
  another account.
- `su`: a new elevated cash (`su -` a login shell in your home folder), or `su USER` a
  cash as that account; `su -c COMMAND` runs one command.
- `sudoedit FILE`: edit a file you may not write. Your editor runs unelevated, on a copy;
  only writing the copy back is elevated.
- `elevate COMMAND`: ask UAC to start a program elevated, in a new window, and do not wait
  for it. It needs no other tool.

## What does the elevating

cash elevates nothing itself; it chooses what runs and hands it to one of these, the
first that is there:

- gsudo (`scoop install gsudo`, `winget install gerardog.gsudo`): output in this
  terminal, a credentials cache (`sudo -v`, `sudo -k`), and running as other users;
- Windows' own `sudo`, in recent Windows 11 once it is turned on in Settings (For
  developers): in its new-window mode the output stays in that window;
- UAC's request, as `elevate` makes it: a new window, not waited for.

`sudo -l` says which one this machine uses, and `cash doctor` reports it too.

What runs is what the shell would run: `sudo ls` is cash's `ls`, run by an elevated cash;
`sudo bash` is cash, not WSL's `bash.exe`; a script or a batch file runs through cash. A
function is not run, as a Unix `sudo` runs none. In a shell that is already elevated,
the command simply runs here.

## What escapes cash's job

Everything cash starts lives in its job object and ends with it (D6). An elevated
program cannot: Windows will not let a normal process put an elevated one in its job or
end it (D42). So an elevated program, or a shell as another user, outlives cash, and is
not stopped by Ctrl-C to the job or by `kill %1` as a job of yours would be. `elevate`
warns about it; `-q` drops the warning.

## Other accounts

Running as another account needs that account's password (gsudo asks for it, or Windows'
`runas` in a new window), and that account must be able to read `cash.exe`: a per-user
Scoop install is in your profile, which other accounts cannot read; `scoop install -g
cash` installs it for every account.
