---
name: timeout
summary: Run a command with a time limit.
kind: Bundled coreutils, from uutils
see: sleep kill
draft: yes
---

## How it works

`timeout DURATION COMMAND` runs a command and stops it if it is still running after
DURATION (`10`, `1.5m`, `2h`): GNU's `timeout`, from uutils. It exits 124 when time ran
out.

## Different in cash

Windows' `timeout.exe` is another command, which pauses a batch file (`timeout /t 5`);
inside cash, `timeout` is this one, and `sleep 5` is the pause. `timeout sh -c ...` and
`timeout bash ...` run cash, and an MSYS2 program gets its arguments as it splits them.
