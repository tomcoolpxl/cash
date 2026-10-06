---
see: locale uname vars
---
## Description

`getconf NAME` prints a configuration value by its POSIX name, and `getconf -a` lists
every name it knows with its value, in glibc's two-column layout. A name it does not
know is `getconf: Unrecognized variable 'NAME'`, status 1.

## The values Windows has

- `_NPROCESSORS_ONLN` and `_NPROCESSORS_CONF`: the logical processors.
- `PAGESIZE` and `PAGE_SIZE`: the memory page size, 4096.
- `ARG_MAX`: 32767, the longest command line Windows passes to a program.
- `NAME_MAX`: 255. `PATH_MAX`: 260, or 32767 when Windows' long paths are on.
- `PATH`: the shell's `PATH` as `$PATH` reads, not glibc's fixed `/bin:/usr/bin`.
- `LONG_BIT` 64, `WORD_BIT` 32, `CHAR_BIT` 8, and the `limits.h` constants (`INT_MAX`,
  `LONG_MAX`, `UINT_MAX`, ...) for a 64-bit `long`, as Git for Windows' `getconf`
  answers them.
- `HOST_NAME_MAX` 64, `LINE_MAX` 2048, `OPEN_MAX` 8192, `PIPE_BUF` 4096.

`getconf NAME PATH` takes a path after `NAME_MAX`, `PATH_MAX`, `PIPE_BUF`,
`FILESIZEBITS` and `LINK_MAX`, and answers the same for every path. `-v SPEC` is
accepted and ignored.

## Examples

```
jobs=$(getconf _NPROCESSORS_ONLN); make -j "$jobs"
[ "$(getconf LONG_BIT)" = 64 ] && echo 64-bit
```
