---
name: timeout
summary: Run a command with a time limit.
kind: Bundled coreutils, from uutils
see: sleep kill wait
---

## Syntax

    timeout [OPTION]... DURATION COMMAND [ARG]...

    timeout 10 COMMAND        stop COMMAND after 10 seconds
    timeout 1.5m COMMAND      after a minute and a half
    timeout -k 5 30 COMMAND   stop at 30 seconds, kill 5 later

## Examples

At most a second; 124 says time ran out:

    timeout 1 sleep 5
    echo "status $?"

A command that ends in time keeps its own status:

    timeout 5 sh -c 'echo done; exit 3'
    echo "status $?"

Telling a slow command from a failed one:

    timeout 0.5 sleep 2
    if [ $? -eq 124 ]; then echo "it took too long"; fi

## How it works

`timeout DURATION COMMAND` runs COMMAND, and if it still runs after DURATION, stops it
and exits 124. When COMMAND ends in time, `timeout` exits with its status. DURATION is
a number, decimals allowed, with a unit: `s` seconds (the default), `m` minutes, `h`
hours, `d` days; 0 is no limit.

On Unix it stops the command with a signal, TERM unless `-s` names another, and `-k`
sends KILL that long after if the command is still there. `--preserve-status` exits
with the command's own status even when time ran out: 143 for TERM.

## Options

    -s, --signal=SIGNAL         The signal: TERM, INT, KILL, or a number.
    -k, --kill-after=DURATION   Kill it this long after, if still running.
    -p, --preserve-status       The command's status, timed out or not.
    -f, --foreground            Leave the command in the foreground.
    -v, --verbose               Say on standard error when it stops one.

Exit status: 124 when time ran out; 125 when `timeout` itself fails; 126 when COMMAND
cannot run; 127 when it is not found; else COMMAND's status.

## Different in cash

- Windows' `timeout.exe` is another command, which pauses a batch file (`timeout /t
  5`); inside cash, `timeout` is this one, and `sleep 5` is the pause.
- Windows has no signals to send another program: when time runs out, the command is
  ended, whatever `-s` names, and a `trap` in it does not run.
- `timeout sh -c ...` and `timeout bash ...` run cash, and an MSYS2 program gets its
  arguments as it splits them.
