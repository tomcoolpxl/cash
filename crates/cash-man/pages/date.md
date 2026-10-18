---
name: date
summary: Print or compute dates and times.
kind: Bundled coreutils, from uutils
see: touch printf sleep
---

## Syntax

    date [OPTION]... [+FORMAT]

    date                      now, in the usual form
    date +%Y-%m-%d            now, in a form of your own
    date -d 'DATE' +FORMAT    another date
    date -u                   in UTC
    date -r FILE              a file's modification time
    date +%s                  seconds since 1970

## Examples

A date of your own, in a form of your own:

    date -d '2024-03-15 14:30' '+%Y-%m-%d %H:%M'
    date -d 2024-03-15 '+%A %d %B %Y'

Seconds since 1970, and back:

    date -u -d '2024-01-01 00:00' +%s
    date -u -d @1704067200 '+%F %T'

Arithmetic on dates:

    date -d '2024-03-15 + 10 days' +%F
    date -d '2024-03-01 - 1 day' +%F

A file's modification time:

    touch -d '2023-07-04 12:00' notes.txt
    date -r notes.txt '+%F %H:%M'

A name for a file made now:

<!-- parse only: the name is now's -->
    log="backup-$(date +%Y%m%d-%H%M%S).tar.gz"

## How it works

`date` writes the time now, or the one `-d` describes or `-r` reads from a file, in
the form `+FORMAT` gives, or its own (`Mon Mar 15 14:30:00 CET 2024`) without one.

In FORMAT, `%` and a letter stand for a part of the date and the rest is written as it
is: `%Y` the year, `%m` the month, `%d` the day, `%H:%M:%S` the time, `%s` seconds
since 1970, `%A` the day's name, `%F` the same as `%Y-%m-%d`, `%T` as `%H:%M:%S`.

`-d` reads dates as GNU's `date` does: `2024-03-15`, `'2024-03-15 14:30'`,
`@1704067200` (seconds since 1970), `yesterday`, `tomorrow`, `'next monday'`, `'3 days
ago'`, `'2024-03-15 + 10 days'`.

### The format letters

    %Y  year, 2024           %y  year, 24             %m  month, 03
    %d  day, 15              %e  day, space padded    %j  day of the year
    %H  hour, 00-23          %I  hour, 01-12          %M  minute
    %S  second               %N  nanoseconds          %p  AM or PM
    %A  Friday               %a  Fri                  %B  March
    %b  Mar                  %u  weekday, 1 Monday    %V  ISO week number
    %s  seconds since 1970   %z  +0100                %Z  CET
    %F  %Y-%m-%d             %T  %H:%M:%S             %D  %m/%d/%y
    %n  a newline            %t  a tab                %%  a %

## Options

    -d, --date=STRING      The time STRING describes, not now.
    -r, --reference=FILE   FILE's modification time.
    -u, --utc              In UTC.
    -I[FMT], --iso-8601[=FMT]  ISO 8601: date, hours, minutes, seconds, ns.
    -R, --rfc-email        RFC 5322, as mail dates are written.
    --rfc-3339=FMT         RFC 3339: date, seconds or ns.
    -f, --file=FILE        One date a line of FILE.
    -s, --set=STRING       Set the clock, with the right to.

Exit status: 0 on success; 1 when a date cannot be read.

## Different in cash

- It is uutils' `date`, carried inside cash, so it is the same on every machine, with
  or without Git or MSYS2.
- The local zone is Windows', unless an exported `TZ` names one (`TZ=UTC date`).
