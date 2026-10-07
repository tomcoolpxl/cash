---
names: zip
see: unzip tar gzip
---
## Description

`zip ARCHIVE FILE...` adds files to a zip archive, making it when there is none and
replacing members already in it; `-r` takes folders whole. `-u` adds only what is new or
changed, `-f` replaces only what changed, `-d` deletes members and `-FS` makes the
archive hold exactly the files named. `.zip` is added to an archive's name that has no
suffix. `-` as the archive writes it to standard output; `-` as a file reads standard
input.

The options, messages and exit statuses are Info-ZIP zip 3.0's. That takes in `-0` to
`-9`, `-j`, `-D`, `-X`, `-y`, `-m`, `-q`, `-v`, `-x` and `-i` patterns, `-@`, `-t` and
`-tt` dates, `-n` suffixes, `-Z store|deflate|bzip2`, `-P` and `-e`, `-z` and `-c`
comments, `-o`, `-T`, `-sf`, `-MM` and `-O`. Stored archives without extra fields (`-0
-X`) are zip's byte for byte.

It is all in Rust, in cash. What cash writes, Windows' Explorer, `tar.exe`, 7-Zip and
Info-ZIP's unzip read, with Unix modes and times.

## Windows notes

- Windows has no zip command of its own; Explorer's "Compress to ZIP file" and
  `Compress-Archive` make archives without Unix modes.
- An archive records Unix modes from the Unix face (group bits the owner's), your
  account's id in the `ux` field, and times in the `UT` field; `-X` leaves the last two
  out. A drive and a leading `/` are left out of a member's name.
- Deflate is miniz_oxide's, so a compressed member's bytes, and sometimes its size, are
  not zip's own. Text is told from binary over the whole file.
- A name with characters beyond ASCII is stored as UTF-8 and marked so.
- `-F`, `-FF`, `-s`, `-U`, `-A`, `-J`, `-l`, `-ll`, `-R`, `-DF`, the archive-bit options
  and the logs are refused.

## Examples

```
zip -r site.zip site/            # a folder, whole
zip -u site.zip site/index.html  # only if changed
zip -d site.zip 'site/tmp/*'     # members out
zip -rq - src | ssh host 'cat > src.zip'
zip -9 -x '*.log' logs.zip logs/*
```
