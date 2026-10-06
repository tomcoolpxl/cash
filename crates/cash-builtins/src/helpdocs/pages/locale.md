---
see: getconf iconv vars
---
## Description

`locale` prints the locale as the environment sets it, in glibc's shape: `LANG`,
`LANGUAGE`, each `LC_*` category and `LC_ALL`. A category set in the environment is
printed bare, `LC_TIME=C`; one that follows `LC_ALL` or `LANG` is quoted,
`LC_TIME="C.UTF-8"`. With nothing set, every category is `"C.UTF-8"`: the one locale
cash has.

## The locales by name

`locale -a` lists `C`, `C.UTF-8`, `POSIX` and the Windows locales as `ll_CC.UTF-8`
names: the user's regional format, the system's locale, the two display languages, and
`en_US.UTF-8`, which scripts expect to find. `locale -m` lists `UTF-8`.

`-u`, `-s` and `-f` print the user's display language, the system's and the user's
regional format as `ll_CC`, as the Cygwin `locale` of Git for Windows does; `-U` attaches
`.UTF-8`.

There are no locale definitions to look inside, so `locale -k NAME`, `-c` and a bare
`NAME` are refused, status 1.

## Examples

```
locale                     # LANG=, LC_CTYPE="C.UTF-8", ...
locale -a | grep en_US     # en_US.UTF-8
LC_ALL=C sort file         # as on Linux: the variable is what matters
```
