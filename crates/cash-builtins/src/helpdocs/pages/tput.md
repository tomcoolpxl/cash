---
see: clear keys
---
## Description

`tput` writes terminal capabilities by their terminfo names, as ncurses' `tput` does for
`xterm-256color`: `tput bold`, `tput setaf 2`, `tput cup 5 10`, `tput sgr0`. Numbers
such as `cols`, `lines` and `colors` are printed; a boolean such as `am` prints nothing
and succeeds when the terminal has it, failing with status 1 when it has not, as does a
string the terminal lacks. An unknown name is status 4. The words after a string
capability are its parameters; what follows the numbers is the next capability, so
`tput cup 5 10 bold` writes both. `-S` reads one capability with its parameters per line
from standard input, `-x` keeps the scrollback when clearing, and `clear`, `init`,
`reset` and `longname` work as commands.

## Windows notes

There is no terminfo database: the capabilities are those of `xterm-256color`, built
in, which Windows Terminal and ConPTY understand. `TERM` is not consulted, since the
terminal cash runs in is VT whatever it says; a `-T` naming a terminal that is not
xterm-, VT- or ms-terminal-like is refused with status 3. `cols` and `lines` are the
console window's, or an exported `COLUMNS` or `LINES`; 80 and 24 when there is no
console. `tput reset` also puts back the console modes cash relies on, as `reset` does.

## Examples

```
bold=$(tput bold); normal=$(tput sgr0)        # colours and attributes for a script
echo "${bold}warning${normal}"
tput setaf 1; echo red; tput op                # a colour, then the default colours
cols=$(tput cols)                              # the window's width
tput cup 0 0; tput el                          # the cursor to the top left, clear the line
tput -S <<EOF                                  # several at once
clear
setaf 4
bold
EOF
```
