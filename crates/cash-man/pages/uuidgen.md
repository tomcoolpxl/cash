---
name: uuidgen
summary: Print a new UUID: random, time-based, or a hash of a name.
kind: Text and file tools
see: shuf
---

## Syntax

    uuidgen [OPTION]...

    uuidgen                     a random UUID
    uuidgen -t                  a time-based one
    uuidgen -7                  time-based, version 7: sorts by time
    uuidgen -s -n @dns -N NAME  the same UUID for the same name, every time

## Examples

A new one, of the form 8-4-4-4-12 hex digits:

<!-- parse only: Git Bash has no uuidgen to compare with -->

    uuidgen
    uuidgen -r | cut -c15
    uuidgen -C 3
    uuidgen -s -n @dns -N example.com

## How it works

`uuidgen` prints a new UUID, 128 bits written as 32 hex digits in groups of 8, 4, 4, 4
and 12. By default it is random (version 4, `-r`); `-t` makes one from the time and a
random node (version 1), `-6` and `-7` the newer time-based versions, which sort by
time. `-m` and `-s` make the UUID of a name in a namespace, with MD5 (version 3) or
SHA-1 (version 5): the same name gives the same UUID everywhere. `-n` names the
namespace, `@dns`, `@url`, `@oid` or `@x500`, or a UUID; `-N` the name, read as hex
with `-x`. `-C N` prints N of them.

## Options

    -r, --random          Random: version 4, the default.
    -t, --time            From the time: version 1.
    -6, --time-v6         From the time: version 6.
    -7, --time-v7         From the time: version 7.
    -m, --md5             Of a name, with MD5: version 3.
    -s, --sha1            Of a name, with SHA-1: version 5.
    -n, --namespace=NS    The namespace: @dns, @url, @oid, @x500 or a UUID.
    -N, --name=NAME       The name.
    -x, --hex             Read NAME as hex.
    -C, --count=N         N of them.

Exit status: 0, or 1 for options that do not go together.
