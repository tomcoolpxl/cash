---
name: continue
summary: Go on to the next iteration of a loop.
kind: Bash builtins
see: break
---

## Syntax

    continue [N]

    continue      on to the next round of the loop
    continue 2    on to the next round of the loop around this one

## Examples

Skip the comment lines:

    while read -r line; do
      case $line in '#'*) continue ;; esac
      echo "$line"
    done < settings.conf

The odd numbers only:

    for i in $(seq 6); do
      (( i % 2 == 0 )) && continue
      echo "$i"
    done

On to the next round of the outer loop:

    for dir in a b; do
      for n in 1 2 3; do
        [ $n = 2 ] && continue 2
        echo "$dir$n"
      done
    done

## How it works

`continue` skips the rest of the loop's body and starts its next round: a `for` takes
its next word, a `while` or `until` tests its condition again. `continue N` goes on
with the Nth loop out, leaving the inner ones. Outside a loop it does nothing.

`break` leaves the loop instead.

## Options

    N    Which loop, counting out from this one: 1 without.

Exit status: 0, or not 0 for an N that is not 1 or more.
