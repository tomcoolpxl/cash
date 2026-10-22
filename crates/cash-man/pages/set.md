---
name: set
summary: Set shell options and positional parameters, or list variables.
kind: Bash builtins
see: shopt declare unset trap
---

## Syntax

    set [-abefhkmnprtuvxBCHP] [-o OPTION] [--] [ARG]...
    set [+abefhkmnptuvxBCHP] [+o OPTION] [--] [ARG]...

    set -e           stop the script at the first command that fails
    set -u           an unset variable is an error
    set -o pipefail  a pipeline fails when any of its commands does
    set -x           write each command before it runs
    set -- A B C     make $1, $2, $3 A, B and C
    set +x           turn an option off again: + for off

## Examples

Stop at the first failure, the start of many scripts:

    set -euo pipefail
    echo running
    false
    echo never

An unset variable as an error, not an empty word:

    set -u
    echo "${UNSET_VAR:-a default is fine}"
    echo "$UNSET_VAR"
    echo never

Set the positional parameters, and look at them:

    set -- one 'two words' three
    echo "$# arguments; the second is: $2"
    for a in "$@"; do echo "[$a]"; done

Trace what runs, to standard error:

    set -x
    name=world
    echo "hello $name" 2>/dev/null
    set +x

Which options are on:

    set -o errexit
    set -o | grep -E '^(errexit|nounset) '

## How it works

`set` does three things. With options, it turns shell options on (`-x`, `-o xtrace`)
or off (`+x`, `+o xtrace`). With words after the options, or after `--`, it makes them
the positional parameters, `$1` on, in place of the script's arguments. With nothing,
it lists every variable and function.

The options a script most often wants:

- `-e` (`errexit`): a command that fails ends the script, except where its status is
  tested (`if`, `while`, `&&`, `||`, `!`).
- `-u` (`nounset`): using a variable that is not set is an error.
- `-o pipefail`: a pipeline's status is the last command's that failed, not the last
  command's.
- `-x` (`xtrace`): each command is written to standard error, after expansion, with
  `PS4` (`+ `) before it.

`set -o` lists the options and whether each is on; `set +o` writes them as commands
that set them again.

## Options

    -e, -o errexit     Exit when a command fails.
    -u, -o nounset     An unset variable is an error.
    -o pipefail        A pipeline fails when any command in it does.
    -x, -o xtrace      Write each command before running it.
    -v, -o verbose     Write each line as it is read.
    -n, -o noexec      Read commands without running them.
    -f, -o noglob      No * ? [ ] filename expansion.
    -C, -o noclobber   > does not overwrite a file; >| does.
    -a, -o allexport   Export every variable that is set.
    -b, -o notify      Tell at once when a background job ends.
    -m, -o monitor     Job control: jobs, fg, bg in a script.
    -h, -o hashall     Remember where commands were found.
    -o posix           Behave as POSIX says where Bash differs.
    -o vi, -o emacs    The line editor's keys.
    -r                 Restrict the shell, for good (there is no +r).
    --                 The end of the options: the rest are $1 and on.

Exit status: 0, or 1 or 2 for an option that is not one.

## Advanced

### errexit's exceptions

`set -e` does not stop a script for a command in an `if` or `while` test, before
`&&` or `||`, after `!`, or in a function called from any of those: `f || true` runs
`f` with `-e` off. Test what matters explicitly.

### The restricted shell

`set -r`, or a shell started with `-r` or as `rbash` (after its startup files), refuses
`cd` and the other ways to change directory, a command named with a `/`, `.` of a
path, `exec` of a command, writing a file with `>`, `>>`, `<>` or `&>`, `enable` of a
builtin, `command -p`, and `hash -p` or `history` with a path; `SHELL`, `PATH`,
`HISTFILE`, `ENV` and `BASH_ENV` are read only. cash's tools, `rm`, `cp` and the
rest, are its own commands, not programs found in `PATH`, so a startup file that is to
confine a user turns them off with `enable -n` before the limits begin.

### A script's own arguments, kept

`set --` replaces them; save them first with `args=("$@")`.
