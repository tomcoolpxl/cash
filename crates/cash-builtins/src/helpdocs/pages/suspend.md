---
see: job-control kill
---
## Description

`suspend` stops the shell until its parent resumes it, as Ctrl-Z stops a program. Windows
has no stop signal, so cash cannot be stopped and says so, with status 1:

```
cash: suspend: cannot suspend: Windows has no stop signal
```

This is the shape of Bash's own refusal for a login shell (`cannot suspend a login
shell`), so a script that checks the status sees a builtin that declined, not a command
that was not found. `-f` is accepted and changes nothing.

## What stops a program

Ctrl-Z while a program runs suspends its threads, and `fg` resumes it (see `help
job-control`); that is a program's stop, not the shell's.
