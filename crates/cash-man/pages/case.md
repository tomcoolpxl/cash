---
name: case
summary: Run the commands of the first pattern a word matches.
kind: Bash keywords
see: if [[ shopt
---

## Syntax

    case WORD in
        PATTERN [| PATTERN]...) COMMANDS ;;
        ...
    esac

    ;;      ends the clause
    ;&      goes on into the next clause's commands
    ;;&     goes on testing the next patterns

## Examples

Choosing by the shape of a word:

    for f in notes.txt photo.jpg run.sh; do
        case $f in
            *.txt)       echo "$f: text" ;;
            *.jpg|*.png) echo "$f: picture" ;;
            *)           echo "$f: other" ;;
        esac
    done

A yes-or-no answer, in any case:

    answer=Yes
    case $answer in
        [yY]|[yY][eE][sS]) echo agreed ;;
        *) echo declined ;;
    esac

## How it works

The WORD is expanded once, as a double-quoted word is, and matched against each
PATTERN in turn, as `[[ WORD == PATTERN ]]` matches: `*`, `?` and `[...]` are
wildcards, and quoted text in a pattern is matched as it is. The commands after the
first pattern that matches run, and `case` ends, unless the clause ends in `;&` (the
next clause's commands run too) or `;;&` (the patterns after it are tried as well).
Its status is that of the last command run, or 0 when no pattern matched.

`shopt -s nocasematch` makes the patterns match in any case, and `shopt -s extglob`
allows `@(...)`, `!(...)` and the other extended patterns.
