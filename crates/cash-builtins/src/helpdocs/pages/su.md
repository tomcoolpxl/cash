---
see: sudo elevation id
---
## Description

`su` starts a shell as another user or elevated, as Unix's `su` does. Windows has no
root account and no root password, so `su`, `su -` and `su root` give this account,
elevated; `su USER` gives that account, at its usual level, after its password.

- `-`, `-l`, `--login`: a login shell, started in the account's home folder.
- `-c COMMAND`: run a command instead of a shell.
- `-s SHELL`: that shell instead of cash, found as the shell would run it
  (`su -s pwsh`).
- `-m`, `-p`, `--preserve-environment`: the shell's exported variables go with it, as
  `sudo -E` passes them.
- Words after USER are the shell's arguments.

## Windows notes

- The shell is always a new cash (or the `-s` shell): Windows raises no process that is
  already running, so the shell you typed `su` in stays as it was. In a shell that is
  already elevated, `su` starts the new shell here, without asking.
- `su` goes through what `sudo` uses (cash's own elevation; gsudo or `runas` as another
  account; see `help elevation`),
  with the same checks: files created elevated are yours when you approve with your own
  account, and `su USER` is refused at once when USER cannot read `cash.exe`
  (`scoop install -g cash` installs it where every account can).
- The new shell is outside cash's job object and outlives this one.
- Tab completes the user from the local accounts.

## Examples

```
su                      # an elevated cash
su -                    # an elevated login shell, in your home folder
su -c 'net stop spooler'
su alice                # a cash as alice
```
