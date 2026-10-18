---
name: break
summary: Leave a `for`, `while`, `until` or `select` loop.
kind: Bash builtins
see: continue return
---

## Syntax

    break [N]

    break      leave the loop
    break 2    leave this loop and the one around it

## Examples

Stop at the first file that matches:

    for f in a.md b.md notes.txt pairs.txt; do
      if grep -q cat "$f"; then echo "first with a cat: $f"; break; fi
    done

Read up to a line:

    while read -r line; do
      [ "$line" = END ] && break
      echo "$line"
    done < report.txt

Leave two loops at once:

    for i in 1 2 3; do
      for j in 1 2 3; do
        [ $((i * j)) -eq 4 ] && break 2
        echo "$i x $j"
      done
    done

## How it works

`break` leaves the innermost `for`, `while`, `until` or `select` loop at once, and the
shell goes on after its `done`. `break N` leaves N loops, from the innermost out; an N
larger than the loops there are leaves them all. Outside a loop it does nothing.

`continue` skips to the loop's next round instead; `return` leaves a function, loops
and all.

## Options

    N    How many loops to leave, 1 or more: 1 without.

Exit status: 0, or not 0 for an N that is not 1 or more.
