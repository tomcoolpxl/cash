---
names: clear reset
summary: Clear the terminal screen and its scrollback.
summary reset: Reset the terminal and the console modes cash relies on.
kind: Terminal and clipboard
see: keys
draft: yes
---

## How it works

`clear` clears the screen and, unless `-x`, the scrollback. `reset` also puts the
console back as cash needs it: cooked input, VT output and the UTF-8 code page, which a
program that died in raw mode can leave wrong and no escape sequence can fix.

## Different in cash

Both write the sequences ncurses writes for `xterm-256color`, without terminfo: Windows
Terminal and ConPTY speak VT. A terminal type that is not xterm- or VT-like (`-T`) is
refused.

`reset` hides `C:\Windows\System32\reset.exe`, the Remote Desktop `reset session`
command; give its path to run it. Ctrl-L at the prompt also clears the screen.
