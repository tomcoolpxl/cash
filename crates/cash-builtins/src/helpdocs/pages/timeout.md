---
see: sleep kill
---
## Description

`timeout DURATION COMMAND` runs a command and stops it if it is still running after
DURATION (`10`, `1.5m`, `2h`): GNU's `timeout`, from uutils. It exits 124 when time ran
out.

## Windows notes

Windows' `timeout.exe` is another command, which pauses a batch file (`timeout /t 5`);
inside cash, `timeout` is this one, and `sleep 5` is the pause. `timeout sh -c ...` and
`timeout bash ...` run cash, and an MSYS2 program gets its arguments as it splits them.
