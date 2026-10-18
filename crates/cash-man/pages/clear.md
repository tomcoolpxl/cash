---
names: clear reset
summary: Clear the terminal screen and its scrollback.
summary reset: Reset the terminal and the console modes cash relies on.
kind: Terminal and clipboard
see: keys
---

## Syntax

    clear [-x] [-T TYPE]
    reset

    clear       the screen and the scrollback
    clear -x    the screen only, the scrollback kept
    reset       clear, and fix a console a program left in raw mode

## Examples

Before a fresh screen of output:

<!-- parse only: clears the reader's screen -->

    clear
    clear -x; ls

After a program crashed and left the keys echoing nothing:

<!-- parse only: resets the reader's console -->

    reset

## How it works

`clear` clears the screen and, unless `-x`, the scrollback, by writing the escape
sequences for it. `reset` also puts the console back as cash needs it: cooked input,
VT output and the UTF-8 code page, which a program that died in raw mode can leave
wrong and no escape sequence can fix. Ctrl-L at the prompt clears the screen too.

## Options

    -x         Keep the scrollback.
    -T TYPE    The terminal type, instead of $TERM.

Exit status: 0, or 1 for a terminal type cash does not know.

## Different in cash

Both write the sequences ncurses writes for `xterm-256color`, without terminfo: Windows
Terminal and ConPTY speak VT. A terminal type that is not xterm- or VT-like (`-T`) is
refused.

`reset` hides `C:\Windows\System32\reset.exe`, the Remote Desktop `reset session`
command; give its path to run it.
