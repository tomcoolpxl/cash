---
see: jobs wait pgrep job-control
---
## Description

`kill [-SIGNAL] TARGET...` sends a signal to jobs (`%1`) or processes (pids). `kill -l`
lists the signals. Without a signal it sends `TERM`, as Bash does.

## Windows notes

Windows has no signals; cash delivers each one as the nearest thing Windows has.

- `kill %1` reaches the job's whole process tree, through its job object; `kill 1234`
  reaches that process alone.
- `TERM`, `HUP`, `INT` and `QUIT` ask: a program with a window gets `WM_CLOSE`, as from
  its close button, and is terminated if it still runs five seconds later; a background
  job gets a Ctrl-Break; any other console program is terminated at once.
- `KILL` (`-9`) terminates at once.
- `STOP` and `CONT` suspend and resume the program's threads.
- A program a signal ended exits with 128 plus the signal's number: 143 for `TERM`, 137
  for `KILL`.
- A pid from `$!` keeps naming its job after it ended, so `kill "$pid"` answers "No such
  process" rather than hit another program that got the number since.
- `kill 0` reaches the jobs cash started; `kill -1` is refused.
- `kill -TERM $$` is handled by the shell as Bash handles it: a trap runs, or a script
  ends with 143.

## Examples

```
sleep 100 & kill %1
kill -STOP %1; kill -CONT %1
trap 'kill $(jobs -p)' EXIT
```
