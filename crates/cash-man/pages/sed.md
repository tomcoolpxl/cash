---
name: sed
summary: Edit text line by line: substitute, delete, print, insert.
kind: Text tool (built in)
see: awk grep tr cut dos2unix crlf
---

## Syntax

    sed [OPTION...] SCRIPT [FILE...]
    sed [OPTION...] -e SCRIPT [-e SCRIPT...] [FILE...]
    sed [OPTION...] -f SCRIPT-FILE [FILE...]

    sed 's/OLD/NEW/' FILE          print FILE with the change
    sed -i 's/OLD/NEW/' FILE       change FILE itself
    COMMAND | sed 's/OLD/NEW/'     change what COMMAND prints

## Examples

Replace the first "cat" on each line, or every one, or only the second:

    sed 's/cat/dog/' notes.txt
    sed 's/cat/dog/g' notes.txt
    sed 's/cat/dog/2' notes.txt

Ignore case:

    sed 's/error/ERROR/gI' app.log

Change a file itself, keeping a backup as `config.ini.bak`:

    sed -i.bak 's/http:/https:/g' config.ini

Change several files at once:

    sed -i 's/2025/2026/g' *.md

Print only lines 10 to 20, only the matching lines, or only the last line:

    sed -n '10,20p' app.log
    sed -n '/ERROR/p' app.log
    sed -n '$p' app.log

Print from a start marker to an end marker:

    sed -n '/BEGIN/,/END/p' report.txt

Delete empty lines, comment lines, or trailing spaces:

    sed '/^$/d' list.txt
    sed '/^#/d' settings.conf
    sed 's/[[:space:]]*$//' code.py

Paths contain `/`, so use another delimiter:

    sed 's|C:/old|C:/new|g' paths.txt

Swap two words with groups:

    sed -E 's/([a-z]+) ([a-z]+)/\2 \1/' pairs.txt

Insert a line before line 1, append one after the last:

    sed -i '1i # generated, do not edit' build.cfg
    sed -i '$a # end' build.cfg

Number the lines:

    sed = notes.txt | sed 'N; s/\n/\t/'

Join all lines into one, separated by commas:

    sed ':a; N; $!ba; s/\n/,/g' list.txt

Convert a CRLF file to LF (or use dos2unix):

    sed 's/\r$//' windows.txt

## How it works

`sed` is a stream editor: it reads the input a line at a time into the pattern space,
runs the whole script on that line, and prints the pattern space unless `-n` is given.
Then it reads the next line and starts the script again. Files named on the command
line are read in order as one stream (unless `-s` or `-i`); with no FILE it reads its
standard input.

A script is a list of commands, separated by newlines or `;`. Each command may have an
address in front, which chooses the lines it runs on; without one it runs on every
line. `!` after an address inverts it: `/^#/!d` deletes every line that is not a
comment.

### Addresses

    5             line 5
    $             the last line
    /RE/          lines matching the regular expression RE
    \%RE%         the same, with % (or any character) as the delimiter
    5,10          lines 5 to 10
    /A/,/B/       from a line matching A to the next one matching B, repeatedly
    5,+3          line 5 and the 3 after it
    0,/RE/        from the start to the first match, which may be on line 1
    1~2           every second line from line 1 (odd lines)

### Regular expressions

Basic regular expressions are the default. In them `.`, `*`, `[...]`, `^` and `$` are
special; `\(…\)` groups, `\{n,m\}` repeats, `\+` and `\?` mean one-or-more and
zero-or-one, `\|` separates alternatives. With `-E` (extended) the same are written
`(…)`, `{n,m}`, `+`, `?`, `|`, and the backslashed forms are plain characters.

In the replacement of `s`, `&` is the whole match, `\1`…`\9` the groups, `\n` a
newline, and `\U`, `\L`, `\u`, `\l`, `\E` change case.

## Options

    -n, --quiet, --silent      Print only what the script prints.
    -e, --expression SCRIPT    Add SCRIPT. Several -e and -f join in order.
    -f, --script-file FILE     Add the script in FILE.
    -E, -r, --regexp-extended  Extended regular expressions.
    -i, --in-place[=SUFFIX]    Edit files in place. With SUFFIX, keep the
                               original as FILE+SUFFIX; a * in SUFFIX stands
                               for the file's name.
    -s, --separate             Files are separate streams: $, line numbers and
                               ranges start again in each.
    -z, --null-data            Lines end with NUL rather than newline.
    -l, --length N             Wrap the output of the l command at N characters.
    -u, --unbuffered           Read and write as little as possible at once.
    --posix                    Turn off GNU extensions.
    --sandbox                  Refuse the e, r and w commands.
    --follow-symlinks          With -i, edit the file a link points to.
    --debug                    Print the script and each step.
    -b, --binary               Carriage returns are data; only LF ends a line.

Exit status: 0 on success; 1 for a bad script or option; 2 when an input file could
not be read; 4 for an input or output error. `q N` and `Q N` exit with N.

## Advanced

### Commands

    s/RE/TEXT/FLAGS  substitute: flags g, N (the Nth), p, i/I, m/M, w FILE, e
    y/abc/xyz/       translate characters one for one
    d                delete the pattern space; start the next cycle
    D                delete up to the first newline; restart without reading
    p                print the pattern space
    P                print up to the first newline
    a TEXT           append TEXT after the line
    i TEXT           insert TEXT before the line
    c TEXT           change the selected lines to TEXT
    =                print the line number
    l                print the pattern space unambiguously (\t, \r, $ at end)
    n                print, and read the next line into the pattern space
    N                append the next line to the pattern space
    h, H             copy, append the pattern space to the hold space
    g, G             copy, append the hold space to the pattern space
    x                exchange the pattern and hold spaces
    :LABEL           a place for b and t to jump to
    b LABEL          branch to LABEL (to the end without one)
    t LABEL          branch if a substitution was made since the last line or t
    T LABEL          branch if none was
    q [N], Q [N]     quit, printing (q) or not (Q) the pattern space
    r FILE           append FILE's contents after the line
    w FILE           write the pattern space to FILE
    e [COMMAND]      run COMMAND, or the pattern space, as a command
    z                empty the pattern space
    F                print the input file's name

### Working across lines

The pattern space holds one line, but `N` appends the next one after a newline, so a
script can match across lines: `sed 'N; s/-\n//; P; D'` joins words split by a hyphen
at a line's end. The hold space keeps text between cycles: `sed -n '1!G; h; $p'`
prints a file backwards.

### Scripts in files

A long script reads better in a file, one command a line, with `#` comments:

    # tidy.sed
    s/[[:space:]]*$//
    /^$/d

    sed -f tidy.sed notes.txt

## Different in cash

- **Line endings.** Windows files end lines with CRLF. By default `sed` matches such a
  line without its CR and writes it back with it: `s/foo$/bar/` works on a Windows
  file, and an edited CRLF file stays CRLF; an LF file stays LF. Everything `sed`
  writes for a line (`a`, `i`, `c`, `=`, `l`, `w` files) ends as that line does. The CR
  is data again when the script names it (`\r`, `\x0D`), with `-b` for one run, or
  with `CASH_EOL=lf` exported, for every run of `sed` and `awk`.
- It is uutils' sed carried inside cash, so it is the same on every machine, with or
  without Git or MSYS2.
- The `e` command and the `e` flag of `s` run their command with cash, `cash -c`, as
  GNU sed runs `/bin/sh -c`.
- The script language and options are GNU sed's; `-U` adds uutils' own extensions,
  which GNU sed does not have.
