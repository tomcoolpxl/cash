---
names: chown chgrp
summary: Change a file's owner: nothing on Windows, said once, status 0.
summary chgrp: Change a file's group: nothing on Windows, said once, status 0.
kind: Coreutils written for Windows
see: chmod id differences
---

## Syntax

    chown [OPTION]... OWNER[:GROUP] FILE...
    chgrp [OPTION]... GROUP FILE...

    chown root:root FILE    nothing changes, said once, status 0
    chgrp -f staff FILE     the same, quietly

## Examples

An install script's line runs on, and a file that is not there is still an error:

<!-- parse only: Git Bash's chown fails for root, where cash's changes nothing -->

    printf 'x\n' > run.sh
    chown -f root:root run.sh && echo "carried on"
    chown -f root nosuch.sh || echo "nosuch.sh is not there"

## How it works

`chown OWNER[:GROUP] FILE...` and `chgrp GROUP FILE...` change nothing on Windows. A
Windows file's owner can be set only with privileges a normal user does not have, and
Windows has no POSIX groups. So both say once, on standard error, that nothing was
changed, and return 0, so an install script's `chown root:root /opt/app` carries on. `-f`
keeps them quiet; `-v` names each file whose ownership was kept.

A file that is not there is still an error, status 1, as in GNU's. GNU's other options
(`-R`, `-h`, `--from`, `--reference`, ...) are taken; with nothing changed, they choose
nothing. A `chown` on `PATH` is reached with `enable -n chown`.

## Options

    -f, --silent, --quiet    Say nothing, the files that are not there too.
    -v, --verbose            Name each file whose ownership was kept.
    -c, --changes            Taken; nothing changes to report.
    -R, --recursive          Taken; nothing changes below either.
    --reference=RFILE        Taken; RFILE must be there.
    -h, -H, -L, -P, --from, --dereference    Taken; they choose nothing.

Exit status: 0, or 1 when a FILE is not there.

## Different in cash

A Windows owner is not what a Unix one is. Who may read or write a file is its access
list's to say, not the owner's or a mode's: being owner only lets an account change
that list. A file's group is never consulted at all. To set an owner anyway, Windows'
own tools do it: `sudo icacls FILE /setowner USER` gives a file to another account,
and `takeown /f FILE` takes it for yourself. To let an account read or write a file,
change the list: `icacls FILE /grant USER:M`.
