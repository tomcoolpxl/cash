---
name: chmod
summary: Change a file's permissions, as far as Windows can express them.
kind: Coreutils written for Windows
see: test ls stat differences
---

## Syntax

    chmod [OPTION]... MODE FILE...

    chmod a-w FILE      make it read-only
    chmod u+w FILE      make it writable again
    chmod 444 FILE      read-only, by number
    chmod +x FILE       taken; Windows runs a file by its kind
    chmod -R u+w DIR    writable, all through a directory

## Examples

Make a file read-only, and writable again:

    chmod a-w notes.txt
    [ -w notes.txt ] || echo 'read-only'
    chmod u+w notes.txt
    [ -w notes.txt ] && echo 'writable'

By number:

    chmod 444 pairs.txt; [ -w pairs.txt ] || echo read-only
    chmod 644 pairs.txt; [ -w pairs.txt ] && echo writable

`+x` is taken and changes nothing, so a script made runnable this way runs:

    printf '#!/bin/sh\necho ran\n' > run.sh
    chmod +x run.sh && ./run.sh

Through a directory:

    mkdir -p tree/sub && touch tree/sub/f
    chmod -R a-w tree; [ -w tree/sub/f ] || echo 'read-only'
    chmod -R u+w tree

## How it works

A Windows file has no Unix mode: it has a read-only attribute and an access list. So
`chmod` sets the one thing a mode can say there, the read-only attribute, from the
owner's write bit: `chmod u-w`, `a-w` or `444` make a file read-only; `u+w`, `644` or
`755` make it writable again. `ls -l` and `stat` show the mode cash makes from the
attribute and the file's kind.

MODE is symbolic, `[ugoa][+-=][rwxXst]`, several joined by commas (`u+w,go-w`), or a
number in octal (`644`).

### What Windows cannot store

- `chmod +x`, and the execute bits of a number, change nothing and return 0, so
  `chmod +x deploy.sh && ./deploy.sh` works: whether a file runs is decided by its
  kind (an extension in `PATHEXT`, or a `#!` line) and its access list, as `test -x`
  decides it.
- `chmod -x`, `-r`, setuid, setgid and the sticky bit warn and return 0: revoking them
  would take deny entries in the access list that can lock you out of your own file.
  `-f` keeps quiet.
- Group and other bits (`chmod go-w ~/.ssh`) change nothing and say nothing: Windows
  has no such bits per file.

## Options

    -R, --recursive       Change directories and all in them.
    -v, --verbose         Say each file changed, and what it was.
    -c, --changes         Say each file whose mode changed.
    -f, --silent, --quiet Say nothing of what cannot be changed.
    --reference=RFILE     Use RFILE's mode.

Exit status: 0 on success, the warnings above included; 1 when a file cannot be
changed or is not there.

## Different in cash

- It is cash's own `chmod`, written for Windows: what it can change is the read-only
  attribute alone.
