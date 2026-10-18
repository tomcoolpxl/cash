---
name: env
summary: Run a command in a changed environment, or print the environment.
kind: Bundled coreutils, from uutils
see: export printenv vars paths
---

## Syntax

    env [OPTION]... [NAME=VALUE]... [COMMAND [ARG]...]

    env                     print the environment
    env NAME=VALUE COMMAND  run COMMAND with NAME set
    env -u NAME COMMAND     run it without NAME
    env -i COMMAND          run it with an empty environment
    #!/usr/bin/env bash     a script's first line: find bash on PATH

## Examples

Run a command with a variable set for it alone:

    env GREETING=hi printenv GREETING
    echo "after: [${GREETING-}]"

Without a variable:

    export DROPME=1
    env -u DROPME printenv DROPME || echo 'DROPME is gone'

The environment, filtered:

    export MARKER=found
    env | grep '^MARKER='

## How it works

`env` with no COMMAND writes the environment, one `NAME=value` a line: what a program
started now would get, the exported variables. With a COMMAND, it runs it in a changed
copy of that environment: `NAME=VALUE` words set variables, `-u NAME` removes one, and
`-i` starts from nothing. The shell's own variables are not changed.

`NAME=VALUE command` without `env` does the same in the shell; `env` is for where a
shell's syntax is not at hand, as the first line of a script: `#!/usr/bin/env bash`
runs the `bash` found on `PATH`, wherever it is installed.

## Options

    -i, --ignore-environment  Start with an empty environment.
    -u, --unset=NAME          Remove NAME.
    -C, --chdir=DIR           Run COMMAND in DIR.
    -0, --null                End each line of the listing with a NUL byte.
    -S, --split-string=S      Split S into words: a #! line's arguments.
    -v, --debug               Say what is done.

Exit status: COMMAND's; 125 when `env` fails; 126 when COMMAND cannot be run; 127 when
it is not found.

## Different in cash

- `#!/usr/bin/env bash` and the like work in cash without a `/usr/bin/env` file: cash
  reads the line and finds the program on `PATH`.
- The listing shows `PATH` in its Windows form, with semicolons, as a program gets it;
  `$PATH` in the shell is `/c/...:/c/...` (see `help paths`).
