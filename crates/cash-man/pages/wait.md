---
name: wait
summary: Wait for background jobs to end, and give their status.
kind: Bash builtins
see: jobs kill job-control
---

## Syntax

    wait [-fn] [-p VAR] [ID]...

    wait            wait for every background job
    wait $!         wait for the last one started, and take its status
    wait %1         wait for job 1
    wait -n         wait for whichever ends first

## Examples

Run things side by side, then wait for them all:

    (sleep 1; echo first) &
    (echo second) &
    wait
    echo 'both done'

A job's exit status, through `wait`:

    (exit 3) &
    wait $!
    echo "status $?"

Whichever ends first, with `-n`:

    (sleep 2; exit 1) &
    (exit 7) &
    wait -n
    echo "first to end: $?"
    wait

## How it works

`wait` waits for the background jobs named, by process id (`$!` is the last one
started) or job (`%1`), and returns the last one's exit status. Without an ID it waits
for every job and returns 0.

`wait -n` waits for the next job to end, whichever it is, and returns its status;
`-p VAR` puts the id of the one that ended in VAR. A script that starts jobs and ends
without waiting leaves them running.

## Options

    -n         Wait for the next job to end, and return its status.
    -p VAR     Put the id of the job that ended in VAR (with -n).
    -f         Wait until each job has ended, not only stopped.

Exit status: the last job's status; 127 when an ID is no job of this shell.
