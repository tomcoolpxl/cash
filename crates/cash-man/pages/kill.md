---
name: kill
summary: Send a signal to jobs or processes.
kind: Bash builtins
see: jobs wait trap pgrep job-control
---

## Syntax

    kill [-s SIGNAL | -SIGNAL] TARGET...
    kill -l [SIGNAL]

    kill %1            end job 1 (with TERM)
    kill 1234          end process 1234
    kill -9 %1         end it at once (KILL)
    kill -STOP %1      suspend it; kill -CONT %1 resumes it
    kill -l            list the signals

## Examples

End a job started with `&`, and see how it ended:

    sleep 30 &
    kill %1
    wait %1; echo "status $?"

By process id, with `$!`:

    sleep 30 &
    pid=$!
    kill "$pid"; wait "$pid"; echo "status $?"

At once, with KILL:

    sleep 30 &
    kill -9 $!; wait $!; echo "status $?"

A signal's number, and the name of a status:

    kill -l TERM
    kill -l 143

## How it works

`kill` sends a signal to each TARGET: a job (`%1`, `%%` the current one, `%-` the one
before, `%name` the one whose command starts with name) or a process id. Without a
signal it sends `TERM`, which asks a program to end; `KILL` (`-9`) ends it at once
and cannot be caught; `INT` is what Ctrl-C sends; `STOP` and `CONT` suspend and resume.

A signal is named (`-TERM`, `-s TERM`, `-SIGTERM`) or numbered (`-15`). A program a
signal ended exits with 128 plus the signal's number: 143 for `TERM`, 137 for `KILL`.

`kill -l` lists the signals; with a number, or a status above 128, it names the
signal.

## Options

    -s SIGNAL    The signal to send, by name.
    -n NUM       The signal to send, by number.
    -SIGNAL      The same, shorter: -9, -KILL, -SIGKILL.
    -l [SIG]     List the signals, or name one.

Exit status: 0 when a signal was sent to every TARGET; 1 otherwise.

## Advanced

### Ending what a script started

`trap 'kill $(jobs -p) 2>/dev/null' EXIT` ends a script's background jobs when it
ends.

## Different in cash

Windows has no signals; cash delivers each one as the nearest thing Windows has.

- `kill %1` reaches the job's whole process tree, through its job object; `kill 1234`
  reaches that process alone.
- `TERM`, `HUP`, `INT` and `QUIT` ask: a program with a window gets `WM_CLOSE`, as from
  its close button, and is terminated if it still runs five seconds later; a background
  job gets a Ctrl-Break; any other console program is terminated at once.
- `KILL` (`-9`) terminates at once.
- `STOP` and `CONT` suspend and resume the program's threads.
- `kill -l` lists Linux's 31 signals by Linux's numbers (`USR1` is 10; Git Bash says
  30). Windows has no way to send a program the others, `USR1`, `PIPE`, `ALRM` and the
  rest: `kill -USR1 1234` says so and fails, sending nothing. Sent to the shell itself,
  they run its trap (see `trap`).
- A pid from `$!` keeps naming its job after it ended, so `kill "$pid"` answers "No such
  process" rather than hit another program that got the number since.
- `kill 0` reaches the jobs cash started; `kill -1` is refused.
- `kill -TERM $$` is handled by the shell as Bash handles it: a trap runs, or a script
  ends with 143.
