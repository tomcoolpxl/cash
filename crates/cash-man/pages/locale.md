---
name: locale
summary: Show the locale: the `LANG` and `LC_*` view, and the locales by name.
kind: Coreutils written for Windows
see: getconf iconv vars
---

## Syntax

    locale [OPTION]

    locale       LANG and each LC_* category, as the environment sets them
    locale -a    the locales there are, by name
    locale -m    the character sets

## Examples

The view, and the locales by name:

<!-- parse only: Git Bash's locale names MSYS2's locales -->

    locale
    locale -a | grep en_US
    LC_ALL=C sort file

## How it works

`locale` prints the locale as the environment sets it, in glibc's shape: `LANG`,
`LANGUAGE`, each `LC_*` category and `LC_ALL`. A category set in the environment is
printed bare, `LC_TIME=C`; one that follows `LC_ALL` or `LANG` is quoted,
`LC_TIME="C.UTF-8"`. With nothing set, every category is `"C.UTF-8"`: the one locale
cash has.

`locale -a` lists `C`, `C.UTF-8`, `POSIX` and the Windows locales as `ll_CC.UTF-8`
names: the user's regional format, the system's locale, the two display languages, and
`en_US.UTF-8`, which scripts expect to find. `locale -m` lists `UTF-8`.

`-u`, `-s` and `-f` print the user's display language, the system's and the user's
regional format as `ll_CC`, as the Cygwin `locale` of Git for Windows does; `-U` attaches
`.UTF-8`.

There are no locale definitions to look inside, so `locale -k NAME`, `-c` and a bare
`NAME` are refused, status 1.

## Options

    -a, --all-locales      The locales by name.
    -m, --charmaps         The character sets.
    -u, --user             The user's display language.
    -s, --system           The system's locale.
    -f, --format           The user's regional format.
    -U, --utf              With .UTF-8 after the name, for -u, -s, -f.

Exit status: 0; 1 for `-k`, `-c` or a NAME.
