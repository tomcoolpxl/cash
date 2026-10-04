---
see: sudo su detach elevation
spec: D42 D45 D6
---
## Description

`elevate COMMAND` asks UAC to start a command elevated, in a new window, and returns
without waiting for it. It needs no other tool, which makes it the way to elevate on a
machine with neither gsudo nor Windows' `sudo`. For output in this terminal, use `sudo`.

## Windows notes

The command starts in the shell's folder and is found as the shell would find it, but
from your own environment: UAC passes on none of the shell's variables. An elevated
program cannot be put in cash's job object, so it outlives cash and cash cannot end it
(D42). `elevate` says so on standard error; `-q` keeps quiet.

## Example

```
elevate -q regedit
```
