---
see: sudo elevation id
spec: D42 D45
---
## Description

`su` starts a shell as another user or elevated, as Unix's `su` does. Windows has no
root account and no root password, so `su`, `su -` and `su root` give this account
elevated; `su USER` gives that account, at its usual level, after its password. `-`, `-l`
or `--login` start a login shell in the account's home folder, `-c COMMAND` runs one
command instead of a shell, and `-s SHELL` another shell (`su -s pwsh`).

## Windows notes

The shell is a new cash: Windows raises no process that is already running, so the one
you typed `su` in stays as it was. `su` elevates through what `sudo` uses (see `help
elevation`), and the new shell is outside cash's job object (D42). Another account must
be able to read `cash.exe`: `scoop install -g cash` installs it for every account.
