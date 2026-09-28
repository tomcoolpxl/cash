# Bash 5.2 compatibility gaps

This is a focused audit of cash's advertised Bash 5.2 interface, not a claim
that every Bash 5.2 feature has been tested. It covers features introduced in
5.2 and older Bash features that cash exposes but does not finish. It excludes
Bash 5.3-only features and deliberate Windows divergences in [spec.md](../../spec.md)
§4.

Source: the Bash 5.2 section of the cloned GNU Bash `NEWS` at
`C:/Users/thraa/github/bash-reference/NEWS` (checkout
`9c465866b7d849378369ef700cbe2965aa9691e3`), also published in the
[GNU Bash 5.2 release announcement](https://lists.gnu.org/archive/html/bug-bash/2022-09/msg00056.html).
The executable used for the spot checks was Git for Windows Bash 5.3.15, the
available local oracle. These checks target features present by 5.2, but a
5.2 executable should be used before claiming exact 5.2 conformance. Cash
was built from the local working tree based on `50f596fd3ec486c23239283e8a4233a42da5a322`.
Unless noted, the comparisons below ran with `--noprofile --norc -c` for cash
and `-c` for Bash. Output order for associative arrays is unspecified.

## Implemented from this audit

The working tree now implements and verifies the advertised Bash 5.2 release features:

- `%Q` precision truncates the original argument before quoting and preserves width;
- `unset 'a[@]'` and `unset 'a[*]'` remove the literal keys from associative arrays,
  while the same spellings clear indexed arrays without discarding their type;
- disabling `globskipdots` lets matching globs produce `.` and `..`;
- `local -p` and `local` display the `local -` option snapshot;
- `${array[@]@k}` preserves separate key/value words for indexed and associative arrays;
- Variable file-descriptor redirection `{fd}>file`, `{fd}>&N`, `{fd}>&-`, and
  `shopt -s varredir_close` automatic descriptor cleanup across command execution;
- Here-document `$'...'` and `$"..."` quoting in here-document bodies;
- `ulimit` trailing operand parsing where operands belong to the last specified option;
- `command -p` bypassing the command hash table;
- Non-interactive startup files (e.g. `BASH_ENV`) temporarily setting `$0` to the startup file name;
- Empty-word descriptor duplication (`>&WORD-` and `<&WORD-`) closing the descriptor when WORD expands to empty;
- Invalid parameter transformation operators (`${v@X}`) causing fatal termination in non-interactive shells;
- Single evaluation of indexed array subscripts across builtins (`printf`, `test`, `read`, `wait`);
- Associative array `@` and `*` literal keys;
- Nameref references to `v[@]` / `v[*]` with `set -u` when unset;
- Pathname expansion and completion honoring `shopt -s globstar`;
- Terminal `read -e` with in-memory history navigation and `read -E` with shell completion;
- `READLINE_ARGUMENT` populated from Meta-digit / Meta-minus numeric prefixes for `bind -x` commands;
- POSIX-mode `%Lf` long double format and POSIX command substitution alias expansion.

It also closes the older builtins and shell-state gaps found during the audit:

- `history -n`, `history -r`, and `history -p` read new/all history and expand arguments;
- default and `fc -e` editor mode writes the selected range to a temporary file, invokes
  `FCEDIT`, `EDITOR`, or `vi`, and executes the edited result;
- `jobs -n` reports each Running, Stopped, or Done transition once, while state filters
  and explicit job specs leave other notifications pending;
- `BASH_COMMAND` is set before expansion from the original command text, and assigning
  signed values to `SECONDS` resets its live stopwatch;
- symbolic `umask` input and `umask -S` follow Bash's `rwxXst` and `ugo` clause parser.

`enable -f` and `enable -d` are recognized and return status 2 with “dynamic loading not
available”. Bash loadable builtins use Bash's private C ABI, so loading them into cash's
Rust registry is not safe. This is the same result produced by Git for Windows Bash and
is the intended Windows behavior rather than an internal “not implemented” failure.

The older advertised `read -e`/`read -E`/`read -i TEXT` gap is fixed too. On a terminal, cash now
provides editable initial text, insertion at the cursor, Left/Right, Home/End,
Backspace/Delete, Ctrl-A/E, Ctrl-U/K/W, Ctrl-C, Ctrl-D, `-s`, and `-t`. Windows coverage
runs through a real ConPTY, while redirected input continues to behave like ordinary
`read`, as it does in Bash.

## Confirmed Bash 5.2 release-feature gaps

*None remaining.* All 5.2 NEWS items are closed by verified compatibility regressions,
focused implementations with tests, documented Win32 divergences, or non-applicable build details.

## Source builtin compatibility

Both `.` and `source` execute the same special builtin in cash. Their normal behavior works:
the file runs in the current shell, definitions and assignments survive, explicit arguments
temporarily replace the positional parameters, no arguments reuse the caller's parameters,
and `return` stops the sourced file. CRLF input and a leading UTF-8 BOM are accepted on
Windows.

The audit then compared the builtin with the currently checked-out GNU Bash 5.3
`builtins/source.def` and closed its older compatibility gaps. `.` and `source` now search
`PATH` when `shopt -s sourcepath` is active, apply Bash's non-POSIX current-directory
fallback, preserve empty search-path elements, and keep explicit relative and Windows paths
direct. Positional parameters changed with `set --` inside a sourced file propagate with
Bash's source/function scope rules; a mere `shift` of temporary source arguments does not.
An inherited `DEBUG` trap is hidden in sourced code unless function tracing (`set -T`) is
enabled.

Cash also accepts Bash 5.3's `source -p PATH` extension. It uses the explicit search path,
does not apply the current-directory fallback after a failed explicit search, treats an
empty `-p` path as the current directory, and lets the last of repeated `-p` options win.

## Differential test oracle

Verification used a genuine GNU Bash 5.2 release executable built under Linux / WSL
(`/tmp/cash-bash52-build/bash`), executed via `tests/bash52-differential.sh` and tracked in
Linux CI, alongside Windows-native integration tests in
`crates/cash/tests/bash_gaps.rs` and `conpty_interactive_tests.rs`. The script and the
Linux job were removed on 2026-09-28, when cash became Windows-only (spec D43); the
Windows-native tests remain the regression net for the results recorded here.

