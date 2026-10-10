---
see: sudo su detach elevation
---
## Description

`elevate COMMAND` asks UAC to start a command elevated, in a new window, and returns
without waiting for it, for a program that should have a window of its own. For output
in this terminal, and the command's status, use `sudo`.

## Windows notes

- The command starts in the shell's folder and is found as the shell would run it (a
  script runs through cash), but from your own environment: UAC passes on none of the
  shell's variables.
- It is not waited for, so `elevate`'s status says only that the command started, not
  how it ended.
- An elevated program cannot be put in cash's job object, so it outlives cash and cash
  cannot end it. `elevate` says so on standard error; `-q` keeps quiet.

## Example

```
elevate -q regedit
```
