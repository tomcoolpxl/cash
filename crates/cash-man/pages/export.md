---
name: export
summary: Mark variables to be passed to the commands the shell runs.
kind: Bash builtins
see: env printenv declare unset vars paths
---

## Syntax

    export NAME=VALUE...

    export NAME=VALUE        set a variable and pass it to programs
    export NAME              pass an existing variable on
    export -n NAME           stop passing it, keep it in the shell
    export -p                list what is passed on
    NAME=VALUE COMMAND       pass a value to one command only

## Examples

Set a variable that the programs the shell starts see:

    export GREETING=hello
    printenv GREETING

A variable not exported stays in the shell:

    LOCAL=x
    printenv LOCAL || echo 'LOCAL is not passed on'

A value for one command only, the shell's own variables untouched:

    COLOUR=red printenv COLOUR
    echo "after: [${COLOUR-unset}]"

Stop passing a variable on, keeping its value in the shell:

    export V=1
    export -n V
    printenv V || echo 'V is no longer passed on'
    echo "V is still $V"

What is exported, as a line the shell can read back:

    export GREETING=hello
    export -p | grep GREETING

Put a directory in front of `PATH`, for this shell and what it starts:

<!-- parse only: changes PATH, and what follows would run from it -->
    export PATH="$HOME/bin:$PATH"

## How it works

Every program gets a copy of an environment: names and values the shell hands it as it
starts. `export` marks a shell variable to be in that copy; the shell's other variables
are its own. A change made after `export` is passed on too, and a variable exported
before it is set (`export NAME`, then `NAME=value`) is passed on once it has a value.

A program gets a copy, so nothing it does to its environment comes back: a script run as
`./setup.sh` cannot set a variable in the shell that ran it. `source setup.sh` (or
`. setup.sh`) runs it in the shell itself, and what it exports stays.

`NAME=VALUE command` passes NAME to that one command, and leaves the shell's own NAME
as it was. Variables already in the environment when cash starts (`PATH`, `HOME`,
`TEMP`, and the rest Windows sets) are exported from the start.

## Options

    -n            Stop passing each NAME on; it stays in the shell.
    -p            List the exported variables as declare -x lines; with -f,
                  the exported functions.
    -f            Each NAME is a function: pass it to child shells.

With no NAME, `export` lists as with `-p`. A NAME that is not a valid name (`1x`,
`a b`) is refused, and the rest are still exported.

Exit status: 0; 1 when a NAME is not a valid name, or with `-f`, no function.

## Advanced

### Functions and arrays

`export -f NAME` passes a function to child shells as an environment variable; Bash
and cash read it back as a function. An array is never passed on: `export arr` marks
it, and programs get nothing.

### Listing

`export -p` writes `declare -x NAME="value"` lines, quoted so that the shell reads them
back as they were. `env` and `printenv` show what a program would get instead,
`PATH` in its Windows form.

## Different in cash

- `$Path` reads `PATH`, as Windows' names ignore case, but `export Path=x` makes a
  second variable: export `PATH` as it is spelled (see `help vars`).- `PATH` reads Unix-style in the shell, `/c/tools:/c/Windows`, and programs get the
  Windows form, `C:\tools;C:\Windows`. No other variable is changed on the way: see
  `help paths`.
