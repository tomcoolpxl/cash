---
name: bind
summary: Show or change the line editor's key bindings.
kind: Bash builtins
see: keys abbr
---

## Syntax

    bind [-lpsvPSVX] [-m KEYMAP] [-f FILE] [-q NAME] [-u NAME] [-r KEYSEQ]
         [-x KEYSEQ:COMMAND] [KEYSEQ:FUNCTION]

    bind -p                    the bindings, as lines bind reads back
    bind '"\C-t": kill-line'   Ctrl-T kills to the end of the line
    bind -x '"\C-g": git st'   Ctrl-G runs a command
    bind -l                    the editing functions there are

## Examples

A key bound to a function, and one to a command, in `~/.bashrc`:

<!-- parse only: changes the reader's line editor -->

    bind '"\C-t": kill-line'
    bind -x '"\C-g": git status'
    bind -p | grep kill-line
    bind -q kill-line

## How it works

`bind` shows and changes how the prompt's line editor reads keys, with readline's
syntax: `"KEYSEQ": FUNCTION` binds a key sequence to an editing function, `-x
"KEYSEQ": COMMAND` to a shell command, run with `READLINE_LINE` and `READLINE_POINT`
holding the line and the cursor. `-p` and `-P` list the bindings, `-l` the functions,
`-v` and `-V` the variables, `-s`, `-S` and `-X` the macros and commands; `-q` says
which keys run a function, `-u` unbinds them and `-r` unbinds a sequence. `-f FILE`
reads bindings from a file in `inputrc`'s syntax.

## Options

    -p, -P        The bindings: as bind reads them, or in words.
    -l            The editing functions.
    -v, -V        The variables.
    -s, -S        The macros.
    -X            The commands bound with -x.
    -x SEQ:CMD    Bind a sequence to a shell command.
    -q NAME       The keys that run NAME.
    -u NAME       Unbind every key that runs NAME.
    -r SEQ        Unbind SEQ.
    -f FILE       Read bindings from FILE.
    -m KEYMAP     The keymap to act on: emacs, vi.

Exit status: 0, or 1 for a name or a sequence it does not know.

## Different in cash

- The line editor is cash's own, not readline: the functions it has are the ones `bind
  -l` lists, and a key bound to another does nothing. `help keys` lists what the keys
  do.
