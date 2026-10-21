---
name: [[
summary: Test strings, numbers and files, with patterns and regular expressions.
kind: Bash keywords
see: test if case ((
---

## Syntax

    [[ EXPRESSION ]]

    [[ -f FILE ]]           FILE is a file (-d a directory, -e there at all)
    [[ STR == PATTERN ]]    STR matches the wildcard PATTERN (!= does not)
    [[ STR =~ REGEX ]]      STR holds a match of REGEX, its groups in BASH_REMATCH
    [[ A -lt B ]]           numbers compared (-eq -ne -le -gt -ge)
    [[ -z STR ]]            STR is empty (-n not empty)
    [[ E1 && E2 ]]          both (|| either, ! not, ( ) to group)

## Examples

A pattern, and a regular expression with its groups:

    file=report-2026.txt
    [[ $file == *.txt ]] && echo text
    if [[ $file =~ ([a-z]+)-([0-9]+) ]]; then
        echo "${BASH_REMATCH[1]} of ${BASH_REMATCH[2]}"
    fi

No quotes are needed around a variable, empty or holding spaces:

    empty=; spaced='a b'
    [[ -z $empty && $spaced == 'a b' ]] && echo both

## How it works

`[[ ]]` is part of the shell's grammar, not a command: the words inside are not split on
spaces and no wildcards are expanded, so `$var` needs no quotes. Its status is 0 when the
expression is true, 1 when it is false, and 2 when it cannot be read, as a regular
expression that does not compile.

On the right of `==` and `!=` the word is a pattern, as in `case`, unless it is quoted;
on the right of `=~` it is a POSIX extended regular expression, a quoted part of it
matched as written. `<` and `>` compare strings in the locale's order. The file tests
are those of `test`, and `-v NAME` asks whether a variable is set.
