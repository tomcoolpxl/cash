---
names: chown chgrp
see: chmod id differences
---
## Description

`chown OWNER[:GROUP] FILE...` and `chgrp GROUP FILE...` change nothing on Windows. A
Windows file's owner can be set only with privileges a normal user does not have, and
Windows has no POSIX groups. So both say once, on standard error, that nothing was
changed, and return 0, so an install script's `chown root:root /opt/app` carries on. `-f`
keeps them quiet; `-v` names each file whose ownership was kept.

A file that is not there is still an error, status 1, as in GNU's. GNU's other options
(`-R`, `-h`, `--from`, `--reference`, …) are taken; with nothing changed, they choose
nothing. A `chown` on `PATH` is reached with `enable -n chown`.

## Owners on Windows

A Windows owner is not what a Unix one is. Who may read or write a file is its access
list's to say, not the owner's or a mode's: being owner only lets an account change
that list. A file's group is never consulted at all. To set an owner anyway, Windows'
own tools do it: `sudo icacls FILE /setowner USER` gives a file to another account,
and `takeown /f FILE` takes it for yourself. To let an account read or write a file,
change the list: `icacls FILE /grant USER:M`.

## Examples

```
chown root:root /opt/app/run.sh     # nothing changed, status 0
chgrp -f staff notes.txt            # the same, quietly
```
