---
see: awk dos2unix unix2dos crlf
---
## Description

`sed` reads text line by line, runs a script of editing commands on each line, and
writes the result: `s/old/new/` substitutes, `d` deletes, `p` prints, `a`, `i` and `c`
add or change lines. It is the uutils sed, carried inside cash, so it is the same on
every machine whether or not Git or MSYS2 is installed.

## Line endings

Windows files end their lines with CRLF. By default sed matches a CRLF line without its
CR and writes it back with it, so `s/foo$/bar/` works on a Windows file and an edited
CRLF file stays CRLF; an LF file stays LF. Everything sed writes for a line (`a`, `i`,
`c`, `=`, `l`, `w` files) ends as that line does.

The CR is ordinary data again when you ask for it:

- the script names a carriage return (`\r`, `\x0D`), so `sed 's/\r$//'` strips it as it
  does on Linux;
- `-b` (`--binary`) for one run, as GNU sed's Windows builds have it;
- `CASH_EOL=lf` exported in the environment, for every run of sed and awk.

To convert a file between CRLF and LF, use `dos2unix` and `unix2dos`.

## Examples

```
sed -i 's/version = .*/version = "1.2"/' Cargo.toml
sed -n '/^\[profile/,/^\[/p' Cargo.toml
git log --oneline | sed 's/^\([0-9a-f]*\) /\1: /'
```

## Differences from GNU sed

The script language and options are GNU sed's. Line endings are the one deliberate
difference: on Linux a CR is always data, and `$` does not match before it.
