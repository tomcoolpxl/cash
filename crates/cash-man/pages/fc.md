---
name: fc
summary: List, edit and run commands from the history.
kind: Bash builtins
see: history keys
---

## Syntax

    fc [-e EDITOR] [-lnr] [FIRST [LAST]]
    fc -s [OLD=NEW] [COMMAND]

    fc -l           the last 16 commands, numbered
    fc -l -5        the last five
    fc              the last command, in an editor; what is saved runs
    fc -s           the last command, again
    fc -s a=b       the last command, again, a made b

## Examples

At the prompt, the history listed, and a command run again changed:

<!-- parse only: works on the reader's own history -->

    fc -l -5
    fc -s make=cargo
    fc -e nano -3

## How it works

`fc` lists, edits and reruns commands from the history. `fc -l` lists them, numbered,
the last 16 or from FIRST to LAST; `-n` leaves the numbers out, `-r` reverses. Without
`-l`, `fc` writes the commands from FIRST to LAST (the last one without) to a file,
opens it in an editor, and runs what is saved. `fc -s OLD=NEW` runs the last command
again, OLD replaced by NEW, without an editor; a COMMAND names the last command that
starts with it. FIRST and LAST are numbers, negative ones counting back, or the start of
a command. The editor is `-e EDITOR`, else `$FCEDIT`, else `$EDITOR`, else `vi`.

## Options

    -l             List, do not run.
    -n             No numbers in the list.
    -r             Newest first.
    -e EDITOR      The editor: $FCEDIT, $EDITOR, vi without.
    -s [OLD=NEW]   Run the command again, OLD made NEW.

Exit status: the last command run's; 1 for a range with no command in it.

## Different in cash

Windows' `fc.exe` compares files; inside cash, `fc` is this builtin. To compare files,
use cash's `diff` or `cmp`, or `"$SYSTEMROOT/System32/fc.exe"`.

Ctrl-X Ctrl-E at the prompt edits the current line the same way, with `$VISUAL` first.
A script keeps no history, so there `fc` has nothing to list.
