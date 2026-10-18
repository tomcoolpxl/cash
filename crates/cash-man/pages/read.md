---
name: read
summary: Read a line and split it into variables.
kind: Bash builtins
see: mapfile printf vars
---

## Syntax

    read [-ers] [-a ARRAY] [-d DELIM] [-n N] [-p PROMPT] [-t SECS] [NAME]...

    read -r LINE              a line into LINE, backslashes kept
    read -r A B REST          its words into A and B, the rest into REST
    IFS= read -r LINE         the line as it is, spaces at its ends kept
    read -rp 'Name? ' NAME    ask, and read the answer

## Examples

The first line of a file:

    read -r first < notes.txt
    echo "$first"

A line's words into variables, the last one taking what is left:

    read -r a rest <<< 'one two three'
    echo "[$a] [$rest]"

Fields split on another character: `IFS` set for `read` alone:

    IFS=: read -r user pass uid <<< 'root:x:0'
    echo "$uid"

Each line of a file, in a loop:

    while IFS= read -r line; do echo "> $line"; done < pairs.txt

Each line split at `=`:

    while IFS='=' read -r key value; do
        echo "$key -> $value"
    done < config.ini

The words of a line into an array:

    read -ra words <<< 'a b c'
    echo "${#words[@]} words, the second: ${words[1]}"

Ask for an answer, and for a password without showing it:

<!-- parse only: waits for the keyboard -->
    read -rp 'Name? ' name
    read -rsp 'Password: ' pass; echo

## How it works

`read` reads one line from standard input, splits it into words at the characters in
`IFS` (by default spaces, tabs and newlines), and gives each NAME a word in turn; the
last NAME gets the rest of the line, spaces between its words kept. Spaces and tabs
at the ends of the line are dropped while `IFS` holds them, so `IFS= read -r line`
keeps a line exactly as it is. Without a NAME the whole line goes to `REPLY`,
unsplit, its spaces kept.

Without `-r`, a backslash quotes the next character and a backslash at the end of a
line joins the next line on; `-r` keeps backslashes as they are, which is what almost
every script wants, Windows paths above all.

`read` returns 0 when it read a whole line and 1 at the end of the input. A last line
with no newline after it is still put in NAME, but `read` returns 1, so a loop stops
before it: `while read -r l || [ -n "$l" ]` reads it too.

In a pipeline, `read` runs in a subshell, and its variables are gone when the
pipeline ends: `echo x | read v` leaves `v` unset. Use a here-string
(`read v <<< "$text"`), a redirection (`< file`), or `{ ...; }` after the pipe:

    echo 'a b' | { read -r x y; echo "$y"; }

## Options

    -r            Keep backslashes; do not join lines.
    -a ARRAY      Put the words in the indexed array ARRAY, from index 0.
    -d DELIM      End the line at the first character of DELIM, not a
                  newline; -d '' ends it at a NUL byte.
    -n N          Stop after N characters, or at the end of the line.
    -N N          Read exactly N characters; a newline is one of them.
    -p PROMPT     Show PROMPT first, on standard error, when reading a
                  terminal.
    -s            Do not show what is typed: for passwords.
    -t SECS       Give up after SECS seconds (a fraction is allowed),
                  status above 128; -t 0 asks whether input is waiting.
    -u FD         Read from descriptor FD, not standard input.
    -e            Read a terminal with line editing: arrows, history.
    -E            The same as -e.
    -i TEXT       With -e, start the line with TEXT to edit.

Exit status: 0 when a line was read; 1 at the end of the input; above 128 when `-t`
ran out; 2 for a bad option.

## Advanced

### Reading a file whole

`mapfile -t lines < file` reads every line into an array at once, faster than a
`while read` loop; `$(< file)` reads the file into a string.

### Null-separated names

`find . -print0 | while IFS= read -r -d '' f; do ...; done` reads file names that
may hold any character, newlines too.

## Different in cash

- A line ending in CRLF loses its `\r` too, so `while read -r l` over a file a
  Windows program wrote gives the values it would on Linux; Git Bash keeps the `\r`
  at the end of each line. With `-d` or `-N`, it stays.
- `-E` takes Bash's spelling, but cash's line editor does not yet complete names.
