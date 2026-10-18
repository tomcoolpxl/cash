---
name: eval
summary: Run its arguments as shell code.
kind: Bash builtins
see: source declare printf
---

## Syntax

    eval [ARG]...

    eval "$code"           run the text in $code as commands
    eval "x_$i=value"      set a variable whose name is made at run time

## Examples

Run a command kept in a variable:

    cmd='echo one; echo two'
    eval "$cmd"

A variable whose name is made at run time:

    for i in 1 2; do eval "item_$i=value$i"; done
    echo "$item_1 $item_2"

Read the value of a variable named by another, here safer with `${!name}`:

    name=greeting; greeting=hello
    eval "echo \$$name"
    echo "${!name}"

## How it works

`eval` joins its arguments with spaces and runs the result as shell code, in the
current shell: variables it sets and functions it defines stay. The text is expanded
twice, once as `eval`'s arguments and once as code, which is what makes it useful and
what makes it dangerous.

Never `eval` text that came from outside, a file name or input: `eval "echo $name"`
runs whatever `$name` holds. For a variable's variable, `${!name}` and `declare -n`
are safer; for a command with arguments, an array: `cmd=(ls -l); "${cmd[@]}"`.

`printf '%q'` quotes a value so that `eval` reads it back as it was.

Exit status: the code's, or 0 when there is none.
