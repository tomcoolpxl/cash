---
name: sed
summary: Edit text line by line: substitute, delete, print, insert.
kind: Text tool (built in)
see: awk grep tr dos2unix crlf
---

## Syntax

    sed 'script' [file...]
    sed -i 'script' file...        edit the files themselves

The script is one or more commands; the most used is `s/old/new/`.

## Examples

Replace the first "cat" on each line with "dog":

    sed 's/cat/dog/' notes.txt

Replace every one:

    sed 's/cat/dog/g' notes.txt

Change a file itself, keeping a backup as `config.ini.bak`:

    sed -i.bak 's/http:/https:/g' config.ini

Print only lines 10 to 20:

    sed -n '10,20p' app.log

Delete empty lines:

    sed '/^$/d' list.txt

Paths contain `/`, so use another delimiter:

    sed 's|C:/old|C:/new|g' paths.txt

## Options

    -i[SUFFIX]   edit files in place; keep a backup with SUFFIX
    -n           print only what the script prints (with p)
    -E           extended regular expressions: ( ) + ? | without backslashes
    -e script    add a script; give several -e for several edits

## Different in cash

- Windows line endings just work: `$` matches before a CRLF's CR, and an edited CRLF
  file stays CRLF. `-b` makes the CR ordinary data again.
- It is built in, the same on every machine, with or without Git installed.
