---
see: su sudoedit elevate elevation id
spec: D42 D45
---
## Description

`sudo COMMAND` runs a command elevated, in this terminal, as a Unix `sudo` does:
`sudo winget upgrade --all`, `sudo tee -a C:/Windows/System32/drivers/etc/hosts`.
`sudo -i` gives an elevated login shell, `sudo -s` an elevated shell here, `sudo -u
USER COMMAND` runs it as another account, and `NAME=value` words before the command set
variables for it. `sudo -l` says how this machine elevates.

## Windows notes

cash elevates nothing itself: gsudo does, where it is installed, else Windows' own
`sudo`, else UAC's request in a new window (see `help elevation`). cash chooses what
runs, as the shell would: `sudo ls` is cash's `ls` in an elevated cash, `sudo bash` is
cash, and a script runs through cash. A function is not run. Already elevated, the
command runs here.

Windows' own `sudo.exe` is in System32; inside cash, `sudo` is this builtin, which may
use it. An elevated program is outside cash's job object and outlives cash (D42).
