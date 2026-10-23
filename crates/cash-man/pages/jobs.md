---
names: jobs fg bg
summary: List the shell's jobs.
summary fg: Bring a job to the foreground.
summary bg: Resume a stopped job in the background.
kind: Bash builtins
see: kill wait disown detach job-control
---

## Syntax

    jobs [-lprs] [JOB]...
    fg [JOB]
    bg [JOB]...

    COMMAND &       run it in the background, as a job
    jobs            list the jobs
    jobs -p         their process ids
    fg %1           bring job 1 to the foreground
    bg %1           go on with a stopped job 1 in the background

## Examples

Start jobs, and list them:

    sleep 30 & sleep 31 &
    jobs
    kill %1 %2

Only their process ids, for `kill` or `wait`:

    sleep 30 &
    [ "$(jobs -p)" = "$!" ] && echo same
    kill %1

The jobs still running, and how one ended:

    sleep 30 &
    jobs -r | wc -l
    kill %1; wait %1; echo "status $?"

## How it works

A command ended with `&` runs in the background, as a job, while the shell goes on;
each job gets a number. `jobs` lists them: the number, `+` for the current job and
`-` for the one before, the state (Running, Stopped, Done) and the command.

A job is named `%1` by its number, `%%` or `%+` for the current one, `%-` for the one
before, and `%name` or `%?text` by its command. `fg` brings a job to the foreground,
where it has the terminal until it ends or Ctrl-Z stops it; `bg` lets a stopped job go
on in the background. `wait` waits for jobs to end, and `kill` ends them.

In a script, `fg` and `bg` need job control: they say `no job control` until `set -m`.

A script that ends while jobs of its own run waits for them first: a subshell, a
function or one of cash's commands run with `&` is part of cash and cannot outlive it,
where under Bash it goes on after the script, so its work is done either way. A
program alone (`server &`) is not waited for. At the prompt, `exit` ends the jobs.

## Options

For `jobs`:

    -l          With each job's process id.
    -p          Only the process ids.
    -r          Only running jobs.
    -s          Only stopped jobs.
    -n          Only jobs whose state changed since last told.

Exit status: 0, or 1 when a JOB is not there (`fg` and `bg` also when there is no
job control).

## Different in cash

- Each job runs in a Windows job object of its own, so killing it ends its whole tree.
  See `help job-control` for Ctrl-C, Ctrl-Z and `kill`.
- Ctrl-Z stops a foreground job by suspending its threads; `fg` and `bg` resume
  it. A full-screen program is asked to redraw when it comes back.
- Under `fg`, Ctrl-C reaches a job started with `&` as a Ctrl-Break, and a second
  Ctrl-C ends its tree.
- A background job that runs shell code (`{ ...; } &`, a loop) runs on a thread of
  cash's own process; `jobs -p` shows a number of cash's own for it.
