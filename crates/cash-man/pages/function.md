---
name: function
summary: Define a function: a name for commands, run as a command is.
kind: Bash keywords
see: return local declare unset
---

## Syntax

    function NAME [()] COMMANDS or NAME () COMMANDS

    NAME () COMPOUND-COMMAND [REDIRECTIONS]
    function NAME [()] COMPOUND-COMMAND [REDIRECTIONS]

    greet() { echo "hello, $1"; }
                            define greet
    greet world             run it, with world as $1
    declare -f greet        show its definition
    unset -f greet          remove it

## Examples

A function with arguments and a status:

    is_even() { (( $1 % 2 == 0 )); }
    for n in 3 4; do
        if is_even "$n"; then echo "$n even"; else echo "$n odd"; fi
    done

Variables of its own, and a value handed back through its output:

    total() {
        local sum=0 n
        for n in "$@"; do (( sum += n )); done
        echo "$sum"
    }
    echo "sum: $(total 1 2 3)"

## How it works

A function is defined once and then run by its name, as a command is, in the shell
itself: it sees and changes the shell's variables, unless they are declared `local`. Its
arguments are its positional parameters, `$1`, `$2`, ... and `$@`, and `$0` stays the
script's. Its status is that of the last command it ran, or the one `return` gives.

The body is a compound command, most often `{ ...; }`; a body in `( ... )` runs in a
subshell, so nothing it changes outlasts it. Redirections written after the body apply
each time it runs. A function is found before a builtin or a program of its name; `command
NAME` runs the other.
