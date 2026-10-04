---
names: id groups
see: whoami logname su elevation
spec: D48
---
## Description

`id` prints who you are: `id -un` your name, `id -u` your user number, `id -G` and
`groups` the groups you belong to.

## Windows notes

Windows identifies an account by a SID, not a number. `id -u`, `$UID` and `$EUID` give
the SID's last part, the RID, which is stable for the account, and 0 in an elevated
shell, so `[ "$EUID" -eq 0 ]` and `[ "$(id -u)" -eq 0 ]` both mean "running as
Administrator". Names are `DOMAIN\user` where Windows gives one.

## Examples

```
[ "$(id -u)" -eq 0 ] || { echo "run me elevated: sudo $0"; exit 1; }
```
