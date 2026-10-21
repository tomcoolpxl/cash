---
name: {
summary: Group commands, in the shell itself, as one command.
kind: Bash keywords
see: function if
---

## Syntax

    { COMMANDS; }

    { echo a; echo b; } > out
                            both lines into out
    test -d dir || { echo "no dir" >&2; exit 1; }
                            two commands on one condition

## Examples

One redirection for several commands:

    { echo first; echo second; } | sort -r

Variables set inside stay set:

    { x=1; y=2; }
    echo "$x $y"

## How it works

The commands run in order, in the shell itself, as if they were one: a redirection or a
pipe written after the `}` applies to them all, and what they change (variables, the
directory) stays changed after. `( COMMANDS )` groups them too, but in a subshell, which
changes nothing outside it.

`{` and `}` are words of their own: a space is needed after `{`, and the last command
must end with `;` or a new line before `}`. The status is that of the last command.
