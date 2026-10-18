---
name: nohup
summary: Run a command immune to hangups, its output to `nohup.out`.
kind: Processes and network
see: detach disown job-control
---

## Syntax

    nohup COMMAND [ARG]...

    nohup COMMAND &          in the background, its output in nohup.out
    nohup COMMAND > F &      its output in F instead

## Examples

Output to a file of your own, and the command's status:

    nohup echo "done" > run.log
    echo "status $?"
    cat run.log

A command that is not there:

    nohup nosuch 2>/dev/null; echo "status $?"

## How it works

`nohup COMMAND` runs a command so that a hangup, the terminal closing, does not stop
it, and with GNU `nohup`'s streams: input from a terminal is replaced by an empty one,
output to a terminal goes to `nohup.out` in the current directory (or the home
directory), and errors to a terminal follow the output. Redirected streams stay as
they are. It is usually followed by `&`.

## Options

None: the first argument is the command.

Exit status: the command's; 126 when it cannot run; 127 when it is not found.

## Different in cash

On Windows a hangup is the console closing, and what cash started ends with it, so
`nohup cmd &` does not keep a program running after you close the window. Start such a
program with `detach` instead.
