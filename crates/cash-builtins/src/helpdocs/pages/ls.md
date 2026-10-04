---
see: dir vdir tree stat dircolors
spec: D48 D67
---
## Description

`ls` lists folder contents, GNU's options and lsd's additions: colours by kind of file,
`--icons`, `--tree` and `--depth`, `--attributes`, `--group-directories-first`, and
GNU's sorts (`-t`, `-S`, `-X`, `-v`, `-U`, `--sort`). It is cash's own, written for
Windows.

## Windows notes

- It colours when it writes to a terminal, with no alias needed (a pipe or a file gets
  plain names), from `LS_COLORS` or `dircolors`' defaults. In `-l`, every column is
  coloured, as lsd colours it.
- `-l`'s `w` is the access list's answer: whether you may write the file (or create
  files in the folder). `x` is the file's kind, as `test -x` decides it.
- Files that are both hidden and system (`NTUSER.DAT`'s logs, the old `My Documents`
  junctions) are left out without `-a` or `-A`, as Explorer leaves them out. Hidden
  alone is listed, as a dotfile is not.
- `--attributes` shows the Windows attributes: archive, read-only, hidden, system.
- `--icons` draws Nerd Font icons when Windows Terminal draws the tab in a Nerd Font
  (`--icons-theme=fancy` draws them anywhere).

## Examples

```
ls -la --group-directories-first
ls --tree --depth=2 src
ls -lt --icons
```
