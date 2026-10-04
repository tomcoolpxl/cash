---
see: sudo su detach elevation
spec: D42 D45 D6
---
## Description

`elevate COMMAND` asks UAC to start a command elevated, in a new window, and returns
without waiting for it. It needs no other tool, which makes it the way to elevate on a
machine with neither gsudo nor Windows' `sudo`, and it is what `sudo` and `su` fall back
to there. For output in this terminal, use `sudo`.

## Windows notes

- The command starts in the shell's folder and is found as the shell would run it (a
  script runs through cash), but from your own environment: UAC passes on none of the
  shell's variables.
- It is not waited for, so `elevate`'s status says only that the command started, not
  how it ended.
- An elevated program cannot be put in cash's job object, so it outlives cash and cash
  cannot end it (D42). `elevate` says so on standard error; `-q` keeps quiet.

## Example

```
elevate -q regedit
```
