---
names: mktemp realpath readlink
see: paths
spec: D3 D48
---
## Description

- `mktemp` makes a temporary file (or folder, with `-d`) and prints its name.
- `realpath PATH` prints the absolute path with links and `..` resolved.
- `readlink LINK` prints a link's target; `readlink -f` the resolved path.

## Windows notes

These three build paths themselves, and Windows answers in backslashes, so cash writes
their output in its own spelling, `C:/Users/me/AppData/Local/Temp/tmp.AbCdEf`: a
`d=$(mktemp -d)` that held backslashes would lose them to the first `xargs` or unquoted
`echo -e` downstream (D3). `/tmp` is your `TEMP` folder.

## Examples

```
dir=$(mktemp -d); trap 'rm -rf "$dir"' EXIT
here=$(realpath "$(dirname "$0")")
```
