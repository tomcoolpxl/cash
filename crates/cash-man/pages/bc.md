---
name: bc
summary: Calculate with arbitrary precision, in POSIX bc's language.
kind: Text and file tools
see: let expr
---

## Syntax

    bc [OPTION]... [FILE]...

    echo 'scale=4; 22/7' | bc    3.1428
    echo '2^100' | bc            every digit
    echo 's(1)' | bc -l          the math library: sine, cosine, ...

## Examples

Decimals, big numbers, the math library and another base:

<!-- parse only: Git Bash has no bc to compare with -->

    echo 'scale=4; 22/7' | bc
    echo '2^100' | bc
    echo 's(1)' | bc -l
    echo 'obase=16; 255' | bc

## How it works

`bc` calculates with numbers of any size and precision, in POSIX bc's language:
`scale` sets the decimal places, `ibase` and `obase` the bases, `-l` loads the math
library (`s`, `c`, `a`, `l`, `e`, `j`). Neither Windows nor Git for Windows ships one.

### Differences from GNU bc

- POSIX only, and no `dc`. GNU's extensions (`print`, `read()`, `else`, `&&`, `||`, `!`,
  `#` comments, `last`, `halt`, multi-letter names, ...) are errors that name the
  extension, so a script written for GNU bc fails with a reason. A one-line `define f(x)
  { return (x); }` is the one extension accepted.
- After an error, bc exits 1, where GNU bc exits 0, so `set -e` and `|| die` catch a
  failed calculation.
- Values below one print as GNU's do: `.33`, `-.5`. Output otherwise matches GNU bc's,
  long-number wrapping included.
## Options

    -l, --mathlib        The math library, and scale 20.
    -i, --interactive    Go on after an error, and exit 0, as at a prompt.
    -q, --quiet          Accepted: there is no banner.
    -s, --standard       Accepted: this bc is POSIX bc.
    -w, --warn           Accepted: extensions are errors that say so.

Exit status: 0, or 1 after an error.

