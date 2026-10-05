---
see: sudo elevation
---
## Description

`sudoedit FILE...` (also `sudo -e FILE...`) edits files you may not write, as Unix's
`sudoedit` does.

- Each file is copied to your temporary folder, under its own name, and opened there in
  `$SUDO_EDITOR`, `$VISUAL` or `$EDITOR`, the first that is set (Notepad without one).
  The editor runs as you, not elevated, and is waited for.
- When it ends, each copy you changed is written back into its file by an elevated cash,
  so the file keeps its owner and access list. A file that did not exist is created; a
  file you did not change is left alone. The copies are then removed.
- `-u USER` writes the files as that account, and `-n` fails at once when approval would
  be asked for, as for `sudo`.

An editor that returns at once (`code`) must be told to wait: `EDITOR='code --wait'`.

## Examples

```
sudoedit C:/Windows/System32/drivers/etc/hosts
EDITOR='code --wait' sudoedit C:/ProgramData/ssh/sshd_config
```
