# Bash 5.2 remaining compatibility plan

This plan turns the unverified Bash 5.2 NEWS items into bounded work. It uses the
currently checked-out GNU Bash 5.3 source as the implementation reference because every
5.2 behavior is still present there. Behavior probes should also run against an actual
Bash 5.2 executable in Linux GitHub Actions before cash claims the item as compatible.

The rule for every item is: add the smallest discriminating Bash/cash probe first, record
the observed result in [bash-5.2-gaps.md](bash-5.2-gaps.md), then either add a regression
for an existing pass or implement the mismatch. Native Windows behavior gets a Windows
integration test; portable language behavior also goes into the Linux differential suite.

## Phase 1: small, portable shell semantics

These are the best next targets because they affect scripts and do not require a new
interactive editor or descriptor model.

| Order | Bash 5.2 item | First probes | Likely implementation area | Completion criterion |
|---:|---|---|---|---|
| 1 | Here-document `$'...'` and `$"..."` handling | Unquoted and quoted delimiters; ANSI escapes; translated strings; the same forms inside `${...}` and `$(...)`; a command substitution that finishes parsing with a pending here-document | `cash-parser/src/word.rs`, tokenizer pending-heredoc handling, and `cash-core/src/expansion.rs` | Output and fatal/warning behavior match Bash 5.2 for LF and CRLF input. |
| 2 | `ulimit` trailing operand belongs to the last selected option | `ulimit -S -n 64`, `ulimit -n -S 64`, multiple resource flags with one operand, extra operands, `unlimited`, and invalid numbers | Unix `ulimit.rs` option parsing and the Windows compatibility parser in `ulimit_win.rs` | Both platforms parse and reject the same command shapes. Windows may continue to accept valid settings without applying nonexistent POSIX limits. |
| 3 | `command -p` bypasses the command hash | Hash a fake executable, change the standard-path executable, compare execution and `-v`/`-V`; repeat with a shell function and builtin of the same name | `command.rs`, lookup options, and `SimpleCommand` path resolution | `command -p name` and its descriptions never use a hashed path, while ordinary `command name` still may. |
| 4 | Startup-file `$0` | Run a non-interactive shell with `BASH_ENV` that prints `$0`, using `-c`, stdin, and a named script; verify the body sees its normal `$0` afterward | `cash-shell/src/entry.rs` and init-script call frames | During the startup file `$0` is that file's name, then the caller's `$0` is restored. |
| 5 | Empty-word descriptor duplication closes the target | `3>&$empty`, `3>&$empty-`, input equivalents, explicit `-`, invalid descriptor text, and `set -e` status | Redirection expansion/execution in `cash-core/src/interp.rs` | An empty expanded word follows Bash 5.2's close semantics without weakening error handling for other words. |
| 6 | Invalid parameter transformations are fatal in non-interactive shells | Every invalid `${v@X}` shape in top level, subshell, command substitution, sourced file, and interactive input | Parser/word expansion error classification | Non-interactive shells stop at the same boundary and interactive shells recover with the same nonzero status. |

## Phase 2: array and nameref edge semantics

Treat these as one workstream because they share subscript parsing and repeated expansion.
Start with a table-driven oracle file rather than fixing examples independently.

1. Probe `unset` operands containing command substitutions, arithmetic, quotes, `@`, and
   `*`, with `assoc_expand_once` both on and off. The 5.2 rule is that `unset` first tries
   the argument as an array subscript without parsing or expanding that subscript.
2. Probe indexed subscripts with visible side effects in assignments, arithmetic,
   parameter expansion, `printf`, `read`, `test -v`, and `wait`. Count evaluations so a
   value match cannot hide a double expansion.
3. Cover associative `@`/`*` keys in assignment, lookup, `test -v`, and `unset`. The
   already-fixed `unset 'a[@]'`/`unset 'a[*]'` behavior remains the baseline regression.
4. Cover `declare -n ref='v[@]'` and `ref='v[*]'` under `set -u` when `v` is unset,
   empty, indexed, and associative. Include chained and function-local namerefs.
5. Audit `printf`, `test`, `read`, and `wait` for the 5.2 `assoc_expand_once` changes only
   after the shared subscript evaluator is understood; avoid four separate parsers.

This phase is complete when each NEWS item has a focused regression and a subscript with
a side effect is evaluated exactly as many times as Bash 5.2. Existing broad nameref tests
are useful coverage, but do not replace these discriminating probes.

## Phase 3: completion and interactive editing

Do this in layers so `read -E` does not turn into an accidental GNU Readline port.
Rustyline has been evaluated in [rustyline-evaluation.md](../rustyline-evaluation.md) and
should not be added as another backend. It does not replace the descriptor-aware compact
`read` editor, while the main-prompt integration would duplicate Cash's existing Reedline
adapters. Use its source as a reference for focused editing behavior instead.

1. **Globstar completion:** add completion-engine tests with `globstar` on and off for
   `**`, hidden directories, symlink/junction loops, and Windows separators. Reuse cash's
   existing glob walker and keep junction traversal bounded.
2. **`read -E` completion:** connect Tab in the compact `read` editor to cash's existing
   completion engine. Preserve `-e` as editing without Bash-default completion and test
   editable `-i` text, quoting, spaces, and redirected-input fallback.
3. **History navigation:** add Up/Down and the common Ctrl-P/Ctrl-N behavior to the compact
   editor with an injected in-memory history in tests. Do not read or write the user's
   history from test cases.
4. **Custom keymaps and `bind -x`:** first design a small key-sequence registry shared by
   the main prompt and `read -e`/`-E`; then implement `READLINE_LINE`, `READLINE_POINT`,
   and Bash 5.2's `READLINE_ARGUMENT`. Keep this in Cash's backend-independent interface
   and current Reedline adapter rather than adding another editor backend. This is
   medium-to-large work and should not block the first three layers.
5. **Other Readline 8.2 commands:** evaluate `spell-correct-word`,
   `vi-edit-and-execute-command`, `fetch-history`, and `vi-undo` individually. Implement
   only commands that map cleanly to the selected editor backend; document library ABI,
   color variables, locale refresh, and shared-termcap configuration as non-shell build
   details.

Each interactive layer needs ConPTY coverage on Windows and a non-terminal regression to
prove redirected `read` behavior is unchanged.

## Phase 4: variable file-descriptor redirection

This remains a separate design and implementation project. The parser currently models a
numeric optional `IoFd`; Bash's `{name}>file` syntax needs a distinct AST target so it is
not confused with a brace group or ordinary word.

The design must specify, and then test, all of the following before implementation:

- allocation of a free descriptor at or above 10 and assignment of its number to the
  named variable;
- input, output, append, read/write, duplication, move, and close forms;
- expansion and error behavior for readonly, invalid, array-element, and nameref targets;
- descriptor ownership across functions, sourced files, subshells, pipelines, and command
  substitutions;
- the `exec` builtin exception, where the descriptor intentionally survives;
- `shopt -s varredir_close`, including cleanup on success, expansion failure, redirection
  failure, `return`, `break`, traps, and recovered interactive errors;
- interaction with Cash's Win32 handles and child inheritance list, without leaking a
  handle to unrelated children.

Deliver this as an AST/design change, descriptor-lifetime implementation, then behavioral
matrix. It should not be mixed into the smaller Phase 1 redirection fix.

## Phase 5: POSIX-mode pass

Extract the Bash 5.2 POSIX-mode changes into a dedicated differential file and run every
case twice, normally and with `set -o posix`. Group results by startup, expansion,
redirection, special-builtin failure, and builtin formatting. The initial named targets
are `ulimit` 512-byte units and `printf`'s `L` modifier, followed by the 5.2 POSIX changes
listed in `CHANGES` rather than a broad attempt to emulate an unspecified `/bin/sh`.

Windows-specific impossibilities should be recorded as deliberate platform behavior. For
example, Windows has no POSIX resource-limit API, and Cash's process suspension model is
already a documented Win32 divergence. Portable parser, expansion, exit-status, and
startup behavior still belongs in this pass.

## Items to classify as non-applicable or conditional

The audit should close these explicitly rather than leaving them as vague gaps:

- malloc alignment, the internal timer framework, Readline ABI state, and shared termcap
  selection are implementation/build details; observable timeout behavior is what Cash
  tests;
- the alternate array implementation is a Bash configure-time performance choice;
- `BASH_LOADABLES_PATH`, loadable-builtin defaults, and `enable name` dynamic loading are
  not applicable while native Bash C-ABI modules are deliberately unsupported on Windows;
- `--enable-translatable-strings` is a configure option. `noexpand_translations` becomes
  applicable only if Cash supplies `$"..."` message-catalog translation; quoting syntax
  still needs to parse correctly before then;
- Readline display colors, locale-refresh internals, and library version constants are
  backend details unless Cash exposes a corresponding user-visible contract.

## CI and completion criteria

Add one Linux GitHub Actions job that runs the focused probes against Bash 5.2 and Cash,
and keep native Windows tests for path, CRLF, ConPTY, and handle behavior. Do not use the
developer's HOME or history file in either job. A NEWS item leaves this plan only when the
gap audit records one of: verified compatible, fixed with regression, deliberate Windows
divergence, or non-applicable build detail. “Needs investigation” is not a final state.
