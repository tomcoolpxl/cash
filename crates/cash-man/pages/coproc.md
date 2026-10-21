---
name: coproc
summary: Run a command in the background with pipes to and from it.
kind: Bash keywords
see: read wait jobs
---

## Syntax

    coproc [NAME] COMMAND [REDIRECTIONS]

    ${NAME[0]}              the descriptor to read its output from
    ${NAME[1]}              the descriptor to write its input to
    $NAME_PID               its process id

## Examples

A command that answers each line written to it:

    coproc UP { tr a-z A-Z; }
    echo hello >&"${UP[1]}"
    exec {UP[1]}>&-
    read -r line <&"${UP[0]}"
    echo "$line"
    wait

## How it works

`coproc` starts the command in the background, as `&` does, with two pipes: what the
shell writes to `${NAME[1]}` is the command's standard input, and what the command writes
is read from `${NAME[0]}`. NAME is `COPROC` when it is not given; a simple command takes
no name, as in Bash, so give a name only with a compound command such as `{ ...; }`.

Close the input (`exec {NAME[1]}>&-`) for a command that reads to the end of its input
before it answers, and `wait` for it when it is done. One coprocess runs at a time.
