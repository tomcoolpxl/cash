---
name: sed
summary: Edit text line by line: substitute, delete, print, insert.
kind: Text tool (built in)
see: awk grep tr dos2unix crlf
---

## Syntax

    sed [options] 'script' [file...]      the result goes to the screen
    sed -i [options] 'script' file...     the files are changed themselves
    command | sed 'script'                edit what another command prints

A script is one or more commands, each optionally preceded by an address that says
which lines it applies to: `10d` deletes line 10, `/^#/d` deletes comment lines, `d`
alone deletes every line.

## Examples

Replace the first "cat" on each line, or every one:

    sed 's/cat/dog/' notes.txt
    sed 's/cat/dog/g' notes.txt

Change a file itself, keeping a backup as `config.ini.bak`:

    sed -i.bak 's/http:/https:/g' config.ini

Print only lines 10 to 20, or only the lines that match:

    sed -n '10,20p' app.log
    sed -n '/ERROR/p' app.log

Delete empty lines, or comment lines:

    sed '/^$/d' list.txt
    sed '/^#/d' settings.conf

Paths contain `/`, so use another delimiter:

    sed 's|C:/old|C:/new|g' paths.txt

Swap two words with groups (`-E` for extended regular expressions):

    sed -E 's/([a-z]+) ([a-z]+)/\2 \1/' pairs.txt

Several edits in one run:

    sed -e 's/colour/color/g' -e '/^$/d' essay.txt

Add a line at the top of a file:

    sed -i '1i # generated, do not edit' build.cfg

## How it works

`sed` reads its input a line at a time into the pattern space, runs the whole script on
it, and prints what is left; then the next line. A command runs only on the lines its
address selects:

    5         line 5
    $         the last line
    /re/      lines matching the regular expression re
    5,10      lines 5 to 10
    /a/,/b/   from a line matching a to the next line matching b

Regular expressions are basic by default: `\(`, `\)`, `\+` and `\{n\}` are special, and
plain `(` is a character. With `-E` it is the other way round.

## Options

    -n, --quiet          Print only what the script prints with p.
    -e script            Add a script. Several -e run in order.
    -f file              Read the script from a file.
    -E, -r               Extended regular expressions.
    -i[SUFFIX]           Edit files in place. With SUFFIX, keep the original as
                         file+SUFFIX: -i.bak.
    -s, --separate       Treat files as separate: $ and line numbers per file.
    -z, --null-data      Lines are separated by NUL, not newline.
    -b, --binary         Treat carriage returns as ordinary data.

## Advanced

Commands beside `s`: `d` delete, `p` print, `a text` append after, `i text` insert
before, `c text` change, `y/abc/xyz/` translate characters, `q` quit, `=` print the line
number. `N` appends the next line to the pattern space, so a script can work across
lines; `h`, `g` and `x` copy between the pattern space and a hold space.

The `s` command takes flags after the last delimiter: `g` all matches, a number (`2`)
the nth only, `i` ignore case, `p` print if a substitution was made.

## Different in cash

- **Line endings.** Windows files end lines with CRLF. `sed` matches such a line without
  its CR and writes it back with it, so `s/foo$/bar/` works on a Windows file and an
  edited CRLF file stays CRLF. The CR is data again when the script names it
  (`s/\r$//`), with `-b`, or with `CASH_EOL=lf` exported.
- It is uutils' sed carried inside cash: the same on every machine, Git or not. The
  script language and options are GNU sed's.
