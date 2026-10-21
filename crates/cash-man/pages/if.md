---
name: if
summary: Run commands when a command succeeds.
kind: Bash keywords
see: [[ test case while
---

## Syntax

    if COMMANDS; then COMMANDS;
    [elif COMMANDS; then COMMANDS;]...
    [else COMMANDS;]
    fi

## Examples

A test of a number:

    n=7
    if (( n > 5 )); then
        echo big
    elif (( n > 2 )); then
        echo middling
    else
        echo small
    fi

Any command can be the condition; its status decides:

    if grep -q root <<< 'root:x:0:0'; then echo found; fi

## How it works

The commands after `if` run, and the status of the last one decides: 0 runs the
commands after `then`; anything else tries each `elif` in turn, and then the commands
after `else`. The status of `if` is that of the last command it ran, or 0 when no
condition succeeded and there is no `else`.

The condition is most often a test, `[[ ... ]]`, `[ ... ]` or `(( ... ))`, but any
command will do, and its output is not looked at, only its status. A condition does not
stop a script under `set -e`.
