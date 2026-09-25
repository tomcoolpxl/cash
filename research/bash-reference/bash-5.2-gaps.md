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

The current working tree now fixes five of the original release-feature gaps:

- `%Q` precision truncates the original argument before quoting and preserves width;
- `unset 'a[@]'` and `unset 'a[*]'` remove the literal keys from associative arrays,
  while the same spellings clear indexed arrays without discarding their type;
- disabling `globskipdots` lets matching globs produce `.` and `..`;
- `local -p` and `local` display the `local -` option snapshot.
- `${array[@]@k}` preserves separate key/value words for indexed and associative arrays.

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
`read`, as it does in Bash. This is the useful editing surface rather than an embedding
of GNU Readline: custom `bind` keymaps, history navigation, and Readline completion are
not supplied inside the builtin. `-E` therefore selects the same editor as `-e` instead
of enabling Bash completion.

The implementation was also checked directly against GNU Bash's `builtins/read.def` and
against Git for Windows Bash 5.3.15 for behavior that predates 5.3. The audit fixed:

- `-i` taking effect only with `-e` or `-E`;
- repeated and mixed `-n`/`-N` options, with the final count winning while any `-N`
  keeps delimiter suppression enabled;
- zero-length reads succeeding without consuming input or printing a prompt;
- `$TMOUT`, invalid/non-finite timeouts, and positive timeouts on regular files;
- ordinary NUL skipping, redirected control-byte preservation, and backslash handling
  for newline, NUL, and custom delimiters;
- indexed and associative element targets such as `read 'a[i]'`, and `read -a`
  rejecting an associative target after consuming the input record;
- Ctrl-C returning status 130.

## Confirmed Bash 5.2 release-feature gaps

| Feature | Reproduction and observation | Cash evidence | Scope estimate |
|---|---|---|---|
| `varredir_close` | Bash 5.2 added automatic closing for `{fd}` redirections. `shopt -s varredir_close` succeeds in cash, but `{fd}>out` is parsed as a command (`command not found: {fd}`); the feature cannot run. | [options.rs](../../crates/cash-core/src/options.rs) and [namedoptions.rs](../../crates/cash-core/src/namedoptions.rs) expose the flag, but no execution path reads it. | Large relative to the others: variable-FD grammar, descriptor lifetime, and `exec` exception. |

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

The 5.2 release also introduced `READLINE_ARGUMENT` for `bind -x` numeric
arguments. There is no implementation of that variable in `cash-core` or
`cash-interactive`; it needs an interactive key-binding test before its exact
behavior is marked verified. The `noexpand_translation` option and indexed
subscript single-expansion rules similarly need focused behavioral tests. The
option fields exist, but the relevant code paths do not read them. That is
strong evidence of incomplete behavior, not a full observed mismatch yet.

## Already addressed or not yet established

The earlier [35-probe comparison](README.md) now matches on all its selected
cases, including `wait` status/options, `local -` restoration, pattern
replacement, mapfile callbacks, `%n`, and startup behavior. That suite is
narrow and does not negate the gaps above. Additional focused regressions now cover
the corrected 5.2 `@k` word boundaries and `%Q` precision behavior.

Other 5.2 `NEWS` entries—here-document quoting, `ulimit` operand parsing,
`command -p` hash behavior, startup-file `$0`, nameref/unset edge cases,
completion with `globstar`, and newer Readline bindings—need dedicated probes
before being labelled pass or fail. Build-time features such as the alternate
array implementation and loadable-builtin defaults are not equivalent to
ordinary script-language conformance. POSIX-mode changes need their own pass.

GNU Readline completion for `read -E`, history navigation, and custom keymap integration
remain separate work. The option itself and the core editor behavior are implemented.
The [Rustyline evaluation](../rustyline-evaluation.md) concludes that it should remain a
source reference rather than another Cash backend: it cannot replace the descriptor-aware
`read` editor and would duplicate the existing main-prompt integration.
Variable-FD redirection deserves a separate design because it crosses the parser,
expansion, descriptor-lifetime, and `exec` paths.

The remaining work is broken into probes, implementation phases, CI coverage, and explicit
non-applicable items in the [Bash 5.2 remaining compatibility plan](bash-5.2-remaining-plan.md).
