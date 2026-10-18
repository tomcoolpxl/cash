---
name: awk
summary: Scan and process text with the AWK language.
kind: Text and file tools
see: sed grep cut sort crlf
---

## Syntax

    awk [-F SEP] [-v VAR=VALUE] 'PROGRAM' [FILE]...
    awk [-F SEP] -f PROGFILE [FILE]...

    awk '{ print $1 }' FILE          the first field of every line
    awk -F: '{ print $2 }' FILE      fields split at :, not at blanks
    awk '/error/' FILE               the lines that match, as grep
    awk 'NR > 1' FILE                every line but the first
    awk '{ s += $2 } END { print s }' FILE    a column's sum

## Examples

The first word of each line:

    awk '{ print $1 }' pairs.txt

Fields split at another character, here `=`:

    awk -F= '{ print $1 }' config.ini

Lines that match a pattern, with their line numbers:

    awk '/cat/ { print NR ": " $0 }' notes.txt

The number of fields on each line, and the last one:

    awk '{ print NF, $NF }' pairs.txt

Add up a column, and print the total at the end:

    printf '%s\n' 'a 3' 'b 4' 'c 5' | awk '{ s += $2 } END { print s }'

Count each word, with an array, and say how often `cat` came:

    awk '{ for (i = 1; i <= NF; i++) n[$i]++ }
         END { print n["cat"] }' notes.txt

A value from the shell, with `-v`:

    awk -v min=2 'NF >= min' pairs.txt

Reformat with `printf`:

    awk -F= '{ printf "%-8s %s\n", $1, $2 }' config.ini

## How it works

`awk` reads its input a record at a time, a line by default, and splits it into
fields: `$1`, `$2`, ... up to `$NF`, with `$0` the whole line. By default fields are
separated by runs of blanks, leading blanks ignored; `-F` sets another separator, a
character or a regular expression.

A program is a list of `PATTERN { ACTION }` pairs. For each record, every pattern
that matches has its action run. A pattern alone prints the matching lines; an
action alone runs for every line. `BEGIN { ... }` runs before the first line, `END
{ ... }` after the last.

Patterns are regular expressions (`/re/`), comparisons (`$3 > 100`, `$1 == "x"`), or
both joined with `&&`, `||` and `!`. Actions are C-like: variables need no
declaration, start empty (0 as a number), and are numbers or strings as used;
arrays are indexed by strings.

Quote the program in single quotes, so the shell leaves `$1` alone.

### Variables awk sets

    NR        the number of the current record, from 1
    NF        the number of fields in it
    FNR       NR within the current file
    FILENAME  the current file's name
    FS, OFS   the field separator read, and the one print writes
    RS, ORS   the record separator read, and the one print writes

### Functions

    length(s)        the length of s, or of $0
    substr(s, i, n)  n characters of s from position i (from 1)
    index(s, t)      where t starts in s, or 0
    split(s, a, re)  split s into the array a; returns the count
    sub(re, r, s)    replace the first match in s ($0 if none)
    gsub(re, r, s)   replace every match
    match(s, re)     where re matches, setting RSTART and RLENGTH
    sprintf(f, ...)  printf into a string
    tolower(s), toupper(s), int(x), sqrt(x), rand(), system(cmd)

## Options

    -F SEP          Split fields at SEP, a character or a regular
                    expression; -F '\t' for tabs.
    -v VAR=VALUE    Set VAR before the program starts; repeatable.
    -f PROGFILE     Read the program from a file; repeatable.
    --              The end of the options.

Exit status: 0; what `exit N` gives; 1 for an error in the program; 2 for a file
that cannot be read.

## Advanced

### gawk's additions

cash's awk takes much of gawk's: `gensub`, `asort` and `asorti`, arrays of arrays
(`a[i][j]`), `IGNORECASE`, `**`, `match(s, re, arr)` with the groups in `arr`, and
gawk's error messages.

### Pipes and commands

`print | "sort"` writes to a command, `"date" | getline d` reads from one, and
`system("cmd")` runs one; `close("sort")` ends it, so a second use starts again.

## Different in cash

- It is the POSIX awk of posixutils-rs, carried inside cash and grown towards gawk.
- `system()` and pipes run their command with cash, `cash -c`, so they speak Bash,
  not `cmd`.
- `/dev/stdin`, `/dev/stdout`, `/dev/stderr` and `/dev/null` work as in gawk.
- A CRLF line is read without its CR and written back with it, so `$NF == "x"` works
  on a Windows file and an edited CRLF file stays CRLF; gawk keeps the CR in the last
  field. A program that names `\r` (`sub(/\r$/, "")`), or `CASH_EOL=lf` exported,
  makes the CR ordinary data, as on Linux. `printf` writes exactly what it is given.
