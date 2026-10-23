---
name: tput
summary: Write a terminal capability as the VT sequence xterm-256color uses.
kind: Terminal and clipboard
see: clear keys
---

## Syntax

    tput CAPABILITY [PARAMETER]...
    tput -S

    tput bold         bold from here on
    tput setaf 1      red text
    tput sgr0         back to plain text
    tput cup 5 10     the cursor to row 5, column 10
    tput cols         the window's width

## Examples

The sequences a script writes for bold, a colour and plain text again:

    tput bold | od -An -c
    tput setaf 1 | od -An -c
    tput sgr0 | od -An -c

Bold in a message:

<!-- parse only: draws on the reader's terminal -->

    bold=$(tput bold); normal=$(tput sgr0)
    echo "${bold}warning${normal}"
    tput cup 0 0; tput el
    cols=$(tput cols)

## How it works

`tput` writes terminal capabilities by their terminfo names, as ncurses' `tput` does for
`xterm-256color`: `tput bold`, `tput setaf 2`, `tput cup 5 10`, `tput sgr0`. Numbers
such as `cols`, `lines` and `colors` are printed; a boolean such as `am` prints nothing
and succeeds when the terminal has it, failing with status 1 when it has not, as does a
string the terminal lacks. An unknown name is status 4. The words after a string
capability are its parameters; what follows the numbers is the next capability, so
`tput cup 5 10 bold` writes both. `-S` reads one capability with its parameters per line
from standard input, `-x` keeps the scrollback when clearing, and `clear`, `init`,
`reset` and `longname` work as commands.

## Options

    -S            Capabilities from standard input, one a line.
    -T TYPE       The terminal type: xterm-, vt- or ms-terminal-like.
    -x            With clear: keep the scrollback.
    clear, init, reset, longname    Commands, as ncurses' tput has them.

Exit status: 0; 1 for a capability the terminal lacks; 3 for a terminal type it does
not know; 4 for a capability name it does not know.

## Different in cash

There is no terminfo database: the capabilities are those of `xterm-256color`, built
in, which Windows Terminal and ConPTY understand. `TERM` is not consulted, since the
terminal cash runs in is VT whatever it says, except that `TERM=dumb` (or `-T dumb`) is
a dumb terminal, as in ncurses: no escape codes, only a bell, a carriage return and a
line feed, which Emacs' shell and CI set it for. A `-T` naming a terminal that is not
dumb, xterm-, VT- or ms-terminal-like is refused with status 3. `cols` and `lines` are
the console window's, or an exported `COLUMNS` or `LINES`; 80 and 24 when there is no
console. `tput reset` also puts back the console modes cash relies on, as `reset` does.
