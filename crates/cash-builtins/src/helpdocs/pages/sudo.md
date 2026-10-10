---
see: su sudoedit elevate elevation id
---
## Description

`sudo COMMAND` runs a command elevated, in this terminal, as a Unix `sudo` does:
`sudo winget upgrade --all`, `echo '127.0.0.1 dev' | sudo tee -a
C:/Windows/System32/drivers/etc/hosts`.

- `-i`: a login shell in the account's home folder, or COMMAND run by one. `-s`: a shell
  in this folder, or COMMAND run by one. The shell is always a new cash: Windows raises
  no process that is already running.
- `-u USER`: as that account, at its usual level, not elevated; its password is asked
  for. `-u root` is plain elevation.
- `NAME=value` before the command: variables for the command. `-E`: the shell's
  exported variables go with it.
- `-n`: fail at once with `sudo: a password is required` when approval would be asked
  for, unless the shell is elevated.
- `-v` opens gsudo's credentials cache, which `-u` goes through; `-k` and `-K` close it
  (`-k COMMAND` closes it, then runs the command).
- `-l`: who you are, whether you are an administrator, what elevates here, the cache,
  and whether other accounts can run `cash.exe`.
- `-e FILE...` is `sudoedit`. `-h` shows the usage. Any other option is refused.

## What runs, and what elevates it

cash elevates the command itself, in this terminal: UAC asks each time, and the command
gets this terminal's keys, output and Ctrl-C, redirections and pipes work, and its status
comes back (see `help elevation`). No other tool is needed. With `-u`, gsudo runs the
command as that account where it is installed; else Windows' `runas` starts it in a new
window.

cash chooses what runs, as the shell would: `sudo ls` is cash's `ls` in an elevated cash,
`sudo bash` is cash (Windows' `sudo.exe` found WSL's `bash.exe`), and a script or a batch
file runs through cash. A function is not run, as a Unix `sudo` runs none. In a shell
that is already elevated, the command simply runs here.

## Windows notes

- Files the command creates are yours, not Administrators', when you approve with your
  own account. When another account approves (a standard user typing an
  administrator's password), sudo says once `sudo: running as DOMAIN\ADMIN, not you;
  files and ~ are theirs`.
- Declining UAC's prompt, or leaving it until it gives up, runs nothing: `sudo: not
  run: the request to run it as an administrator was declined`, status 1.
- Where the command is started but not waited for (`runas`, for `-u` without gsudo), sudo
  says so: the status says only that it started, not how it ended.
- In a folder on a mapped network drive, the elevated command still finds it, by the
  folder's network path; gsudo maps the drives. An elevated process has none of your
  drive letters.
- `sudo -u USER` is refused before any password is asked when that account cannot read
  the program: cash installed under `~/scoop` is readable only by you and the
  administrators. `scoop install -g cash`, or an install under Program Files, fixes it.
- Windows' own `sudo.exe` is in System32; inside cash, `sudo` is this builtin, which does
  not use it. An elevated program is outside cash's job object; the elevated cash that
  runs it ends it when the shell that asked ends.
- Tab completes the command after `sudo` and its arguments, and the user after `-u`.
  `cash doctor` says which tool elevates.
