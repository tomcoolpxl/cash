---
name: caller
summary: Show the line, function and file of a function's caller.
kind: Bash builtins
see: trap return
---

## Syntax

    caller [N]

    caller      the line and file the function was called from
    caller 0    the line, function and file of that call
    caller 1    the same, one call further out

## Examples

Where a function was called from, in a script:

    printf '%s\n' 'where() { caller 0; }' 'outer() { where; }' outer > calls.sh
    bash calls.sh

Every call, out to the script itself:

    printf '%s\n' 'trace() {' '  local n=0' \
        '  while caller $n; do n=$((n + 1)); done' '}' \
        'inner() { trace; }' 'outer() { inner; }' outer > trace.sh
    bash trace.sh

## How it works

In a function, `caller` prints the line number and the file of the call that started
the current function. `caller N` prints the line, the function and the file of the
call N frames out, 0 being the current function's caller. Out of frames, it prints
nothing and fails, which ends a loop that walks the calls. A function that reports an
error can name where it was called from: `echo "error at line $(caller)"`.

## Options

    N    How many calls out: 0 is the call to this function.

Exit status: 0; 1 outside a function, or past the outermost call.
