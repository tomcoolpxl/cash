---
name: awk
summary: Scan and process text with the AWK language.
kind: Text and file tools
see: sed crlf
draft: yes
---

## Examples

    awk -F, '$3 > 100 { print $1 }' sales.csv
    ps -ef | awk 'NR > 1 { n[$1]++ } END { for (u in n) print u, n[u] }'
    awk '{ sub(/\r$/, "") } 1' crlf.txt > lf.txt

## How it works

`awk` runs an AWK program over its input, a record (line) at a time, split into fields
`$1`, `$2`, ... by `FS`. It is the POSIX awk of posixutils-rs, carried inside cash and
grown towards gawk: arrays of arrays, `gensub`, `asort` and `asorti`, `IGNORECASE`, `**`,
`match(s, r, arr)`, and gawk's error messages.

## Different in cash

- `system()` and pipes (`"cmd" | getline`, `print | "sort"`) run their command with
  cash, `cash -c`, so they speak Bash, not `cmd`.
- `/dev/stdin`, `/dev/stdout`, `/dev/stderr` and `/dev/null` work as in gawk.

### Line endings

By default awk reads a CRLF line without its CR and writes it back with it, so
`$NF == "x"` works on a Windows file and an edited CRLF file stays CRLF. A program
that names `\r` (`sub(/\r$/, "")`), or `CASH_EOL=lf` exported, makes the CR ordinary data,
as on Linux. `printf` writes exactly what it is given.
