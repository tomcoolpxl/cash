---
name: tee
summary: Copy standard input to files and to standard output.
kind: Bundled coreutils, from uutils
see: cat
---

## Syntax

    tee [OPTION]... [FILE]...

    COMMAND | tee FILE          see the output and keep it in FILE
    COMMAND | tee -a FILE       add it to FILE
    COMMAND | tee A B > /dev/null    write it to two files only

## Examples

See the output and keep it:

    seq 3 | tee out.txt
    cat out.txt

Add to a file rather than replace it:

    echo first > log.txt
    echo second | tee -a log.txt > /dev/null
    cat log.txt

Several files at once:

    echo same | tee one.txt two.txt > /dev/null
    cat one.txt two.txt

Keep a copy of the middle of a pipeline:

    printf '%s\n' b a c | tee unsorted.txt | sort

## How it works

`tee` reads its standard input and writes it both to its standard output and to each
FILE, as a T-joint in a pipe. Each FILE is emptied first, unless `-a` adds to it.

A common use is a file only root may write, from a normal shell: `echo x | sudo tee
/etc/file > /dev/null`, where `sudo echo x > /etc/file` would open the file without
the rights.

## Options

    -a, --append              Add to the FILEs; do not empty them.
    -i, --ignore-interrupts   Go on when Ctrl-C comes.
    -p                        Go on when writing to a pipe fails.
    --output-error[=MODE]     What to do when a write fails: warn, exit, ...

Exit status: 0 on success; 1 when a FILE cannot be written.

## Different in cash

- It is cash's own copy of uutils' `tee`, carried inside cash, so it is the same on
  every machine, with or without Git or MSYS2.
- `>(...)` as a FILE is written as the pipe it is: `cmd | tee >(grep x) > all.txt`.
