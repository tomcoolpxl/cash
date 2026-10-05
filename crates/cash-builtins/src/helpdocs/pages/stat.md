---
see: ls chmod test
---
## Description

`stat FILE...` shows a file's size, blocks, links, times, owner and permissions, in GNU
stat's layout and with its `-c FORMAT` and `--printf` codes (`%s` size, `%y` modified
time, `%n` name, ...). `-f` shows the file system instead.

## Windows notes

uutils' `stat` is Unix-only, so cash carries its own, which shows a Windows file as Git
Bash's does: the mode is made up from the read-only attribute and the kind of file, the
inode is the file's index on its volume, and the device is the volume's serial number.

## Examples

```
stat -c '%s %n' *.zip
stat --printf '%y\n' build.log
```
