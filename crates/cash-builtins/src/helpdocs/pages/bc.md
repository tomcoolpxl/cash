---
see: let expr
---
## Description

`bc` calculates with numbers of any size and precision, in POSIX bc's language:
`scale` sets the decimal places, `ibase` and `obase` the bases, `-l` loads the math
library (`s`, `c`, `a`, `l`, `e`, `j`). Neither Windows nor Git for Windows ships one.

## Differences from GNU bc

- POSIX only, and no `dc`. GNU's extensions (`print`, `read()`, `else`, `&&`, `||`, `!`,
  `#` comments, `last`, `halt`, multi-letter names, ...) are errors that name the
  extension, so a script written for GNU bc fails with a reason. A one-line `define f(x)
  { return (x); }` is the one extension accepted.
- After an error, bc exits 1, where GNU bc exits 0, so `set -e` and `|| die` catch a
  failed calculation.
- Values below one print as GNU's do: `.33`, `-.5`. Output otherwise matches GNU bc's,
  long-number wrapping included.

## Examples

```
echo 'scale=4; 22/7' | bc
echo '2^100' | bc
echo 's(1)' | bc -l
```
