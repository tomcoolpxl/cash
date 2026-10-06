# Changes from uutils diffutils

This crate is cash's copy of [uutils/diffutils](https://github.com/uutils/diffutils)
at revision `7aa5409c2ed723c7b9768ef4cd10f4176cf78f45` (crate `diffutils` 0.6.0,
2026-10-05). Upstream's `diff` had the five output formats and `-q -s -t --tabsize
--width`, each option a word of its own; cash's `diff` has GNU diffutils 3.12's
interface, checked byte for byte against it by `crates/cash/tests/oracle/diff_cases.sh`
and `cmp_cases.sh`. Every change is listed here.

## Kept from upstream

- `src/cmp.rs`: the comparison itself (`cmp`, `prepare_reader`, the `-l`/`-b` output
  formatting, `report_eof`, `report_difference`), with the patches below.
- `src/utils.rs`: `do_expand_tabs`, `do_write_line` and their tests.
- `src/bin/diffutils.rs`: the multi-call program (`diffutils diff ...`), rewritten
  around the library's entry points.
- `LICENSE-MIT`, `LICENSE-APACHE`, `COPYRIGHT`.

## Rewritten

- `src/params.rs`: a parser over `src/getopt.rs` (new), a `getopt_long`-style reader:
  bundled short options (`-ruN`), an argument attached or in the next word (`-U3`,
  `-U 3`, `--unified=3`), unambiguous prefixes of long options, operands anywhere,
  `--`, and getopt's own messages (`unrecognized option '--foo'`, `invalid option --
  'z'`, `option requires an argument -- 'L'`, the ambiguity list). Options added:
  `-r -N -x -i -E -Z -b -w -B -I --strip-trailing-cr -L/--label -T -a --color -v`
  and GNU's `-NUM`; `-d`, `--horizon-lines` and `--speed-large-files` are accepted and
  ignored; the GNU options cash's diff lacks (`-p -F -n -l -D -S -X --from-file
  --to-file --ifdef --*-format --palette --no-dereference --ignore-file-name-case
  --unidirectional-new-file --suppress-blank-empty`) are refused by name. GNU's
  messages for the rest: `missing operand after 'a'`, `extra operand 'c'`, `invalid
  context length 'x'`, `conflicting output style options`, `too many file label
  options`, `invalid width`, `invalid tabsize`, `invalid color`, each followed by
  `Try 'diff --help' for more information.` where GNU prints it.
- `src/engine.rs` (new): the script of changes, in the shape of GNU's `analyze.c`:
  the lines are told apart by equivalence class, the lines whose class has no member
  in the other file are set aside as changed (GNU's `discard_confusing_lines`), and
  the `similar` crate's Myers in linear space (as GNU's `diffseq.h`) runs on the rest,
  so two files of 100,000 lines that differ throughout take a quarter of a second
  (upstream's `diff` crate keeps every step's vector of the shortest edit script over
  every line, and took minutes and 17 GB on them). A line is its bytes with its newline, so an unterminated last line differs from the
  same text with one; the comparison options act on a key computed from each line
  (`-i` lowers case, `-E` expands tabs, `-Z` strips trailing white space, `-b`
  collapses runs and strips the end, `-w` removes all white space, the newline
  included for `-b` and `-w`), never on the line printed. A change whose lines are all
  blank (`-B`, blank to the other options) or all match an `-I` regex is marked
  ignorable; hunks are joined as GNU 3.12 joins them (fewer than `2 * context + 1`
  common lines between two changes, fewer than `context` before an ignorable one) and a
  hunk of ignorable changes alone is left out. `-I` takes the regex crate's syntax.
- `src/normal_diff.rs`, `src/unified_diff.rs`, `src/context_diff.rs`, `src/ed_diff.rs`,
  `src/side_diff.rs`: the printers, rewritten over the engine in the shape of GNU's
  `normal.c`, `context.c`, `ed.c` and `side.c`: GNU's hunk ranges (`3,0` for no line,
  `*** 3 ****` for an insertion point), `\ No newline at end of file` after a line
  without one, `-T`, `-t`, the labels, GNU's default `--color` palette (bold headers,
  cyan line numbers, red deletions, green insertions; in the context format whole
  sections), and for `-y` GNU's columns: the half width and column-two offset from
  `-W` and the tab size, tabs for the padding unless `-t`, `|`, `<`, `>`, `(`, and `/`
  or `\` when one side lacks its newline. A character's width is its Unicode width; a
  control character or a byte that starts no character prints but takes no column.
- `src/diff.rs`: the driver, in the shape of GNU's `diff.c` and `dir.c`: `diff DIR
  FILE`, two directories compared entry by entry in byte order with `Only in`, `Common
  subdirectories` and `File x is a regular file while file y is a directory`, `-r`,
  `-N` (an absent file is empty, with the epoch in its header; an absent directory is
  walked as empty with `-r`), `-x`, the `diff -r a/x b/x` line before each pair of
  files with the option words as typed (shell-quoted where needed), `Binary files a
  and b differ` for a file holding a NUL byte unless `-a`, `Files a and b differ` and
  `Files a and b are identical` with the labels when given, `-` for standard input,
  `cannot compare '-' to a directory`, and the exit status 0, 1 or 2. Output is
  gathered and written once, after any error, as GNU's buffered stdout orders it.
- `src/lib.rs`: `run_diff` and `run_cmp` take the tool's argv and return its status,
  for cash's bundled-command registry; no `std::process::exit` anywhere in the library.

## Patches to upstream's cmp

- The command line is read by `src/getopt.rs`: `-i4`, `-n 5`, `-ls`, long-option
  prefixes, and GNU's messages with the `Try 'cmp --help' for more information.` line:
  `missing operand after 'cmp'`, `extra operand '3'`, `invalid --ignore-initial value
  'x'`, `invalid --bytes value 'x'`, `options -l and -s are incompatible`.
- A positional SKIP wins over `-i`, as in GNU cmp 3.12 (upstream had `-i` win).
- `k` is accepted as a suffix for 1024, as `K` is (GNU's `xstrtoumax`).
- A directory is refused as `cmp: d: Is a directory`; Windows would open it and fail
  the read with `Access is denied`.
- `--help` prints GNU's option table and returns; `-v`/`--version` print
  `cmp (cash): GNU diffutils 3.12's options, from uutils diffutils`.
- `cmp: EOF on 'x' ...` quotes the name as GNU does: `‘x’` when the locale names UTF-8,
  `'x'` otherwise (`utils::quote`).
- `is_posix_locale`, which chooses `char` over `byte` in the message, is cash's rule:
  only a locale named `C` or `POSIX` is POSIX; none named is UTF-8, as it is for the
  rest of cash (GNU takes none named as `C`).
- The `/dev/null` check on standard output (Unix only) is gone with the Unix code.

## Patches to upstream's utils

- `get_modification_time` became `modification_time`/`format_time`: the time is shown
  in the zone an exported `TZ` names (an IANA name, `UTC`, a POSIX `JST-9`), else
  Windows' own, and an absent file (`-N`) shows the epoch rather than the present.
- `format_failure_to_read_input_file` says what GNU says for the error (`No such file
  or directory`, `Permission denied`, `Is a directory`) where Windows says `The system
  cannot find the file specified. (os error 2)`.
- `quote`, `locale_is_posix` and `error_words` are new.

## Removed

- `src/macros.rs` (a test macro) and `tests/integration.rs`: upstream's integration
  tests drove the old option parser and expected Windows' error words; the oracle
  scripts under GNU diffutils replace them. The crate's unit tests remain, under the
  workspace's lints.
- The benchmarks, the fuzz targets and the upstream CI files.
