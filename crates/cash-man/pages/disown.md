---
name: disown
summary: Remove jobs from the job table, so the shell no longer waits for them.
kind: Bash builtins
see: detach jobs nohup job-control
---

## Syntax

    disown [-ahr] [%JOB...]

    disown        forget the current job
    disown %2     forget job 2
    disown -a     forget every job
    disown -h %1  keep job 1, but do not pass it a hangup

## Examples

A job the shell no longer lists:

    sleep 2 &
    disown
    jobs | wc -l

Every job at once:

    sleep 2 & sleep 2 &
    disown -a
    jobs | wc -l

## How it works

`disown` removes a job from the job table: `jobs` no longer lists it, `wait` no longer
waits for it, and `kill %N` no longer finds it; it runs on. Without a job it takes the
current one. `-a` takes every job, `-r` only the running ones, and `-h` leaves the job
in the table but marks it so the shell sends it no hangup when the shell exits.

## Options

    -a    Every job.
    -r    Only the running jobs.
    -h    Keep the job, but send it no hangup on exit.

Exit status: 0, or 1 for a job that is not there.

## Different in cash

On Linux a disowned job also outlives the shell. On Windows every program cash starts is
in cash's job object, which no process can leave once in it, so a disowned job still ends
when cash is closed. To start a program that outlives cash, use `detach`.
