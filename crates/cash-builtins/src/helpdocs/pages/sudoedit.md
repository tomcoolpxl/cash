---
see: sudo elevation
spec: D45
---
## Description

`sudoedit FILE...` (also `sudo -e`) edits files you may not write, as Unix's `sudoedit`
does. Each file is copied to your temporary folder and opened there in `$SUDO_EDITOR`,
`$VISUAL` or `$EDITOR`, the first that is set (Notepad without one), running as you,
not elevated. When the editor ends, each copy you changed is written back by an elevated
cash, so the file keeps its owner and access list.

An editor that returns at once (`code`) must be told to wait: `EDITOR='code --wait'`.

## Example

```
sudoedit C:/Windows/System32/drivers/etc/hosts
```
