---
name: sleep
summary: Wait a number of seconds.
kind: Bundled coreutils, from uutils
see: wait timeout
---

## Syntax

    sleep NUMBER[SUFFIX]...

    sleep 2         two seconds
    sleep 0.5       half a second
    sleep 1m        a minute; h for hours, d for days

## Examples

Wait, then go on:

    sleep 0.1; echo 'after a tenth of a second'

Several times are added together:

    sleep 0.05 0.05; echo done

Try again until something works, a pause between tries:

    n=0
    until [ $n -ge 3 ]; do n=$((n + 1)); sleep 0.1; done
    echo "tried $n times"

## How it works

`sleep` waits as long as its arguments say and exits 0. A NUMBER is in seconds unless
a suffix says otherwise: `s` seconds, `m` minutes, `h` hours, `d` days. It may have
decimals, and several arguments are added: `sleep 1m 30s` is a minute and a half.

Ctrl-C ends it.

Exit status: 0, or 1 for an argument that is not a time.

## Different in cash

- It is uutils' `sleep`, carried inside cash: no program is started, so a short sleep
  in a loop costs no more than the wait.
