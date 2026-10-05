---
see: uniq crlf
---
## Description

`sort` sorts lines: `-n` numerically, `-h` by human sizes (`2K`, `1G`), `-r` reversed,
`-k` by a field, `-t` with a field separator, `-u` without repeats. It is GNU's `sort`,
from uutils.

## Windows notes

Windows' `sort.exe` takes `/R` and `/+n` and no Unix options; inside cash, `sort` is
this one. `sort` sees a CRLF file's CR as part of each line, as GNU sort does: run
`dos2unix` first if the CR gets in the way.
