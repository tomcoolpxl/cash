---
names: pbcopy pbpaste
see: start crlf
---
## Description

`pbcopy` reads standard input to its end and puts it on the clipboard as text. `pbpaste`
writes the clipboard's text to standard output, and nothing when the clipboard holds no
text (an image, files, or nothing at all). They are macOS's names, so a script written
there runs; Windows has no command of its own for either.

## Windows notes

A Windows program expects CRLF on the clipboard and puts CRLF there, while a script works
in LF. `pbcopy` makes every lone LF a CRLF and `pbpaste` makes every CRLF an LF, so `cat
file | pbcopy` pastes into Notepad as lines, and `pbpaste | wc -l` counts them. The input
is read as UTF-8: a leading byte order mark is dropped and an invalid byte becomes U+FFFD.
The clipboard holds UTF-16, which every program reads; text an old program left in the
ANSI code page is read back through that code page. An empty input empties the
clipboard, as on macOS.

macOS's options are accepted: `-pboard general|ruler|find|font` names a pasteboard, and
Windows has one, so every name is the clipboard; `pbpaste -Prefer txt|rtf|ps` asks for a
type, and only text exists here. `-h` prints the usage.

Another program may hold the clipboard open for a moment (a clipboard history, a
password manager): opening is retried for a fifth of a second, then `the clipboard is in
use by another program` fails with status 1.

## Examples

```
pbpaste | sort -u | pbcopy      # the clipboard's lines, sorted
git rev-parse HEAD | pbcopy
cat ~/.ssh/id_ed25519.pub | pbcopy
pbpaste > notes.txt
```
