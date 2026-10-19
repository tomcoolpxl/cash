---
name: detach
summary: Start a command that outlives the shell.
kind: Windows commands
see: disown nohup start job-control
---

## Syntax

    detach COMMAND [ARG]...

    detach code .       an editor that stays open when cash exits
    detach cash -c 'CMD > log 2>&1'    with its output kept

## Examples

An editor, and a server whose output goes to a file:

<!-- parse only: starts programs on the reader's machine -->

    detach code .
    detach cash -c './server.exe --port 8080 > server.log 2>&1'

## How it works

`detach COMMAND` starts a command that outlives the shell: outside cash's job object,
with no console, and not waited for. It prints the new process's id and returns.

## Options

None: the first argument is the command.

Exit status: 0 when the command started; 127 when it is not found.

## Different in cash

Everything else cash starts lives in its job object and ends when cash does; `&`,
`disown` and `nohup` do not change that. `detach` is the one way out. The command runs
in the shell's directory with its exported variables, and is found as the shell would find
it. It gets none of the shell's streams, so its output is lost; a redirection on the
`detach` line applies to `detach`'s own message. To keep the output, detach a cash that
redirects it, as in the second example.

Allowing it has a cost, accepted: for one program to leave the session's job, the job
must let any program ask to leave it.
