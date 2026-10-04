---
title: Elevation: sudo, su, elevate and UAC
summary: Running commands as Administrator or as another user, which tool does it, and what escapes cash's job.
see: sudo su sudoedit elevate detach id job-control
spec: D6 D42 D45
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

cash elevates nothing itself; it chooses what runs and hands it to one of these, the
first that is there:

- gsudo (`scoop install gsudo`, `winget install gerardog.gsudo`): output in this
  terminal, a credentials cache (`sudo -v` opens it, `sudo -k` closes it, and `sudo -n`
  then runs without asking), and running as other users;
- Windows' own `sudo`, in recent Windows 11 once it is turned on in Settings (For
  developers): inline, the output is here; in its new-window mode it stays in that
  window, and `sudo config --enable normal` in an elevated shell changes that;
- UAC's request, as `elevate` makes it: a new window, not waited for. As another user
  without gsudo, Windows' `runas` in a new window.

`sudo -l` says which one this machine uses, and `cash doctor` reports it too. When the
command is started but not waited for, `sudo` says so: its status says only that the
command started.

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
folder on a mapped drive, gsudo maps the drives for the command, and Windows' sudo is
given the folder's network path (`\\server\share\...`).

## What escapes cash's job

Everything cash starts lives in its job object and ends with it (D6). An elevated
program cannot: Windows will not let a normal process put an elevated one in its job or
end it (D42). So an elevated program, or a shell as another user, outlives cash, and is
not stopped by Ctrl-C to the job or by `kill %1` as a job of yours would be. `elevate`
warns about it; `-q` drops the warning.

## Other accounts

Running as another account needs that account's password (gsudo asks for it, or Windows'
`runas` in a new window), and that account must be able to read the program. cash
installed under `~/scoop` is readable only by you and the administrators, so `su USER`
and `sudo -u USER` refuse at once, before any password, and say so. `scoop install -g
cash`, or an install under Program Files, makes it readable to every account; `sudo -l`
says whether other accounts can run it.
