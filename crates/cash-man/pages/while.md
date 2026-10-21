---
names: while until
summary: Run commands while a command succeeds.
summary until: Run commands until a command succeeds.
kind: Bash keywords
see: for if break continue read
---

## Syntax

    while COMMANDS; do COMMANDS; done
    until COMMANDS; do COMMANDS; done

    while read -r line; do ...; done < file
                            each line of a file
    until ping -n 1 host; do sleep 5; done
                            wait for a machine to answer

## Examples

Counting down:

    n=3
    while (( n > 0 )); do
        echo "$n"
        (( n-- ))
    done

Each line of some text:

    printf '%s\n' one two three | while read -r line; do
        echo "line: $line"
    done

The same test, turned round:

    n=0
    until (( n == 3 )); do (( n++ )); done
    echo "$n"

## How it works

`while` runs the commands after it, and when the status of the last one is 0 runs the
commands after `do`, and goes round again; it ends on the first status that is not 0.
`until` is the same with the test turned round: it goes round while the status is not 0.

`break` leaves the loop and `continue` goes back to its test. The status of the loop
is that of the last command of its body, or 0 when the body never ran.

A loop at the end of a pipeline runs in a subshell of its own, so the variables it sets
are gone after it, as in Bash; `shopt -s lastpipe` keeps it in the shell when job
control is off, as in a script.
