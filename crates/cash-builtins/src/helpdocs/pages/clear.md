---
names: clear reset
see: keys
spec: D55 D41 D68
---
## Description

`clear` clears the screen and, unless `-x`, the scrollback. `reset` also puts the
console back as cash needs it: cooked input, VT output and the UTF-8 code page, which a
program that died in raw mode can leave wrong and no escape sequence can fix.

## Windows notes

Both write the sequences ncurses writes for `xterm-256color`, without terminfo: Windows
Terminal and ConPTY speak VT. A terminal type that is not xterm- or VT-like (`-T`) is
refused.

`reset` hides `C:\Windows\System32\reset.exe`, the Remote Desktop `reset session`
command; give its path to run it. Ctrl-L at the prompt also clears the screen.
