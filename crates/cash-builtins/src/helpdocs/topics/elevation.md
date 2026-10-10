---
title: Elevation: sudo, su, elevate and UAC
summary: Running commands as Administrator or as another user, which tool does it, and what escapes cash's job.
see: sudo su sudoedit elevate detach id job-control
---
## Windows has no root

An Administrator account runs most programs unelevated; elevating a program asks UAC
for approval. There is no root account and no root password, so `root` in `su root` and
`sudo -u root` means "this account, elevated". `id -u`, `$UID` and `$EUID` are 0 in an
elevated shell, so `[ "$EUID" -eq 0 ]` still means "running elevated".

Windows raises no program that is already running: an elevated shell, or one as another
user, is always a new cash, and the shell you typed the command in stays as it was.

## Which command

- `sudo COMMAND`: the command elevated, in this terminal, with its output here, as on
  Unix. `sudo -i` gives an elevated login shell, `sudo -s` an elevated shell in this
  folder; `sudo -u USER COMMAND` runs it as another account, at that account's usual
  level.
- `su`: a new elevated cash (`su -` a login shell in your home folder), or `su USER` a
  cash as that account; `su -c COMMAND` runs one command.
- `sudoedit FILE`: edit a file you may not write. Your editor runs unelevated, on a copy;
  only writing the copy back is elevated.
- `elevate COMMAND`: ask UAC to start a program elevated, in a new window, and do not wait
  for it. It needs no other tool.

## What does the elevating

cash does it itself, with no other tool. `sudo` asks UAC, each time, for an elevated cash,
which attaches to this terminal and takes the command's redirections and pipes as they
are: the command reads the keys you type, prints here, gets your Ctrl-C, and its status
comes back. Declining the prompt runs nothing, and `sudo` says so.

As another account (`sudo -u USER`, `su USER`), cash asks USER's password at the
console, not shown, and runs the command as USER the same way, at that account's
usual level. There is no credentials cache: UAC, or the password, is asked each time.

`sudo -l` says how this machine elevates, and `cash doctor` reports it too.

As with gsudo and Windows' own `sudo` in its inline mode, another program of yours
running unelevated on the same console could type into the elevated command; UAC on the
same desktop is a convenience, not a security boundary.

What runs is what the shell would run: `sudo ls` is cash's `ls`, run by an elevated cash;
`sudo bash` is cash, not WSL's `bash.exe`; a script or a batch file runs through cash. A
function is not run, as a Unix `sudo` runs none. In a shell that is already elevated,
the command simply runs here.

## Who owns what it makes

An elevated program normally makes its files Administrators'. cash runs the elevated
command so that what it creates is yours, when you approved with your own account. When
another account approved (a standard user typing an administrator's password), the
command runs as that account: `sudo` says once `running as DOMAIN\ADMIN, not you; files
and ~ are theirs`.

## Network drives

Drive letters are mapped per logon, and an elevated process has none of yours. In a
folder on a mapped drive, the elevated command starts in the folder's network path
(`\\server\share\...`).

## What escapes cash's job

Everything cash starts lives in its job object and ends with it. An elevated
program cannot: Windows will not let a normal process put an elevated one in its job or
end it. `sudo`'s elevated cash watches the shell that asked and ends the command when
that shell ends; Ctrl-C reaches the command through the terminal. A program `elevate`
starts, or a shell as another user, outlives cash, and is not stopped by Ctrl-C to the
job or by `kill %1` as a job of yours would be. `elevate` warns about it; `-q` drops the
warning.

## Other accounts

Running as another account needs that account's password, which cash asks at the
console, and that account must be able to read the program. cash
installed under `~/scoop` is readable only by you and the administrators, so `su USER`
and `sudo -u USER` refuse at once, before any password, and say so. `scoop install -g
cash`, or an install under Program Files, makes it readable to every account; `sudo -l`
says whether other accounts can run it.
