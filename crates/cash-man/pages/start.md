---
name: start
summary: Open a file, directory or URL with its default program.
kind: Windows commands
see: detach elevate
draft: yes
---

## Examples

    start .                    # this directory in Explorer
    start "$(winpath -w ~/Downloads)"
    start https://github.com/tomcoolpxl/cash

## How it works

`start TARGET` opens a file, a directory or a URL with the program Windows has for it, as
double-clicking it in Explorer would: `start report.pdf`, `start .`, `start
https://example.com`. It is the Windows `xdg-open` or `open`. It does not wait for the
program.

## Different in cash

`start` is also a command built into `cmd.exe`, with other options (`start "" /wait
prog`); inside cash, the name is this builtin. `cmd /c start ...` still reaches cmd's.
