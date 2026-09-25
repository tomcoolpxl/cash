# Rustyline adoption evaluation

## Decision

Do not adopt Rustyline as a Cash backend. The licence is compatible and the Windows build
works, but it would add a second full prompt integration without replacing either of the
hard parts: Cash would still need its descriptor-aware `read` editor, and Rustyline needs
an indirect bridge for `bind -x` plus Windows terminal-hardening changes. Reedline is
already integrated with Cash history, completion, highlighting, prompts, edit-buffer
mutation, and host commands. Replacing or accompanying it with Rustyline would increase
maintenance without closing a Bash 5.2 gap.

Use the cloned Rustyline source as a behavioral and implementation reference. Port small,
well-defined editing behavior to Cash where useful, with attribution when copied code is
substantial. Do not copy the whole source tree and do not add it as a dependency.

Rustyline is not a drop-in implementation of Bash's `read -e` or `read -E`. Keep Cash's
compact `read` editor and add completion, history, and shared key bindings to it. The
public Rustyline editor reads the process standard streams and does not expose the raw
reader, renderer, keymap, kill ring, or edit loop needed to preserve `read -u`, `-t`,
`-d`, `-n`, `-N`, and `-s` semantics.

## Source and validation

The source was cloned to `C:/Users/thraa/github/rustyline-reference` and inspected at
master commit `05eb9200f178989ad504718179f11890cfe362c9`. Tags were fetched separately;
the integration target should be the latest release tag, currently
[`v18.0.1`](https://github.com/kkawakam/rustyline/releases/tag/v18.0.1), rather than the
moving master branch. The release is documented at
[`docs.rs/rustyline`](https://docs.rs/rustyline/18.0.1/rustyline/).

Local Windows validation on Rust 1.98.1:

- `cargo test --lib --no-default-features --features custom-bindings,with-file-history`:
  183 passed, 0 failed, 1 ignored on the exact `v18.0.1` tag;
- a compile-only Cash bridge probe used `ConditionalEventHandler`, `EventContext`,
  `RepeatCount`, `Cmd::AcceptLine`, multi-key binding support, and
  `readline_with_initial`; it compiled successfully;
- the minimal Windows dependency set with `custom-bindings` is `bitflags`,
  `clipboard-win`, `libc`, `log`, `memchr`, `radix_trie`, `unicode-segmentation`,
  `unicode-width`, and `windows-sys`. Most are already in Cash's lockfile.

Rustyline declares its MSRV policy as the latest stable Rust at each release and does not
put `rust-version` in the 18.0.1 manifest. Cash must therefore test the pinned version
with its supported toolchain in CI instead of assuming that Cargo will reject an
incompatible update. Use an exact version until that test exists.

## What Rustyline supplies

| Cash need | Rustyline 18.0.1 | Integration work |
|---|---|---|
| Emacs editing | Built in, including numeric arguments, kill ring, yank-pop, undo, word movement, and history search | Translate Cash's `InputFunction` names to `Cmd` values. |
| Vi editing | Built in with insert, command, replace, motions, redo, and counts | Add mode selection and verify Bash binding names individually. |
| Completion | Public `Completer` trait with circular/list modes and display versus replacement text | Adapt Cash's existing completion engine; do not use Rustyline's filesystem completer as shell policy. |
| Multiline parsing | Public `Validator` trait and literal-newline command | Adapt Cash parser completeness; Rustyline has no distinct continuation prompt. |
| History | Search plus a public custom `History` trait | Wrap Cash's history store so `HISTCONTROL`, `HISTSIZE`, `history`, and `fc` retain one source of truth. |
| Highlighting and hints | Public `Highlighter` and `Hinter` traits | Adapt the existing Cash implementations. |
| Initial buffer and cursor | `readline_with_initial(prompt, (left, right))` | Use it to resume after a bound shell command or history verification. |
| Custom keys | Single keys and multi-key `Event::KeySeq`; handlers receive line, byte cursor, repeat count, and sign | Translate Cash key sequences. Arbitrary terminal byte sequences need explicit compatibility tests. |
| Terminal cleanup | Raw-mode guard restores the original mode on normal return and unwinding | Patch the Windows setup panics described below and add an outer recovery test. |
| Windows input | Native `ReadConsoleInputW`, resize events, modifiers, UTF-16 pairs, clipboard paste, and Windows 10 ANSI output | Exercise in Windows Terminal and GitHub Actions ConPTY tests. Mintty/MSYS and PowerShell ISE are explicitly unsupported upstream. |

Rustyline does not provide Readline's `inputrc` parser, dynamically named Readline
functions, editable history entries, a continuation prompt, a right prompt, async stdin,
or a public general-purpose editing engine over caller-provided streams. These remain Cash
features or deliberate limits.

## `bind -x`, edit-buffer variables, and numeric arguments

Reedline has a direct host-command event. Rustyline does not: a custom handler returns
only a `Cmd`. Its public API is still sufficient for a bridge:

1. Register a `ConditionalEventHandler` for the bound sequence.
2. In `handle`, copy `EventContext::line()` and `EventContext::pos()` into shared pending
   state, together with `RepeatCount` and the positive/negative flag.
3. Store the Cash shell command in the same state and return `Cmd::AcceptLine`.
   `AcceptLine` submits even when the validator reports incomplete input.
4. When `Editor::readline*` returns, detect the pending command and return
   `ReadResult::BoundCommand` instead of treating the buffer as user input.
5. Expose the saved line and cursor through `InputBackend::get_read_buffer`, set
   `READLINE_LINE`, `READLINE_POINT`, and `READLINE_ARGUMENT`, execute the command, then
   accept changes through `set_read_buffer`.
6. On the next read, split the saved string at the byte cursor and call
   `readline_with_initial` to resume editing.

The compile-only probe established that all of the Rustyline API used by this bridge is
public in 18.0.1. The bridge still needs behavioral tests for an empty buffer, a cursor in
the middle of Unicode text, positive and negative numeric arguments, a command that
changes or unsets the edit-buffer variables, command failure, Ctrl-C, and repeated bound
commands before Enter.

Macros need their own adapter. Rustyline has multi-key bindings but no public action that
executes an arbitrary sequence of editing commands as a single binding. Cash should keep
its macro registry and either expand a macro into Rustyline key events or map the supported
macros to one command at registration time. Unsupported mappings must stay visible in
`bind -s`/`-S` and return an explicit error rather than silently changing behavior.

## Why it should not replace the `read` builtin editor

Rustyline's public `Editor` is tied to process stdin/stdout/stderr. Cash's `read` supports
an arbitrary input descriptor, regular files and pipes, fractional timeout deadlines,
custom and NUL delimiters, exact character counts, backslash processing, silent input,
and partial data on timeout. Routing those through Rustyline would either break existing
behavior or require copying its private terminal and editing internals.

The useful reusable pieces for `read -e`/`-E` are concepts and Cash-owned adapters:

- Cash completion for `read -E`;
- Cash history navigation for Up/Down and Ctrl-P/Ctrl-N;
- the shared Cash key-binding registry;
- common line-buffer operations where their semantics match.

Rustyline's `line_buffer` module is public, but its keymap, edit state, terminal traits,
kill ring, layout, and undo modules are private. Copying only `LineBuffer` therefore does
not provide a complete editor, while copying the private graph is effectively maintaining
a fork. The compact editor is the smaller and safer base for `read`.

## Windows robustness findings

The Windows backend is broadly suitable for native Cash: it uses Win32 console APIs
rather than an MSYS layer and restores input/output console modes with an RAII guard.
There is one blocker for Cash's robustness contract. During raw-mode setup,
`src/tty/windows.rs` uses `assert_ne!(SetConsoleMode(...), 0)` in two output-mode paths.
An unusual or closing console handle can therefore panic after the input mode has already
changed and before Rustyline returns its restoration guard.

Before Rustyline can become a production backend, either upstream or Cash's pinned copy
must:

1. replace both assertions with ordinary `ReadlineError::Io` propagation;
2. roll back the input mode if any later output-mode setup fails;
3. make restoration errors observable in tracing without panicking;
4. add a Cash-level outer console-mode guard around every editor invocation;
5. run a ConPTY fault test that closes or invalidates the output handle during setup and
   proves the shell reports the error and remains usable.

Rustyline's `Drop` guard intentionally ignores raw-mode restoration errors. That is fine
for unwinding safety only when Cash's outer guard and error logging remain in place.

## Dependency versus source absorption

Using the crate first has the lowest cost and gives a clean comparison with the current
Reedline backend. Configure it with `default-features = false` and enable only
`custom-bindings`; Cash already owns history and completion, so `with-file-history`,
`with-dirs`, `derive`, SQLite history, fuzzy search, and regex history search are not
needed initially.

Absorption is legally straightforward under the
[`MIT licence`](https://github.com/kkawakam/rustyline/blob/v18.0.1/LICENSE), provided the
copyright and permission notice remains with copied substantial portions. Technically it
would add about 14,500 Rust source lines plus a long-lived merge burden. If absorption is
chosen, use a dedicated `cash-line-editor` crate based on tag `v18.0.1`, preserve its git
history or record the exact upstream commit, add its licence to `NOTICE`, and keep Cash
adaptation in separate modules so future upstream fixes can be ported cleanly.

Do not copy isolated private files into `cash-interactive`: their dependencies form a
tightly coupled editor and terminal graph, and scattered copied files would be harder to
update and attribute than a clearly absorbed crate.

## What to reuse

Use Rustyline to check expected Emacs and vi editing behavior and as a source of focused
algorithms or test cases for numeric arguments, kill-ring operations, history search,
Unicode cursor movement, bracketed paste, and Windows key decoding. Implement those in
Cash's existing editor boundaries:

1. add completion and history navigation to the compact `read` editor;
2. finish the shared Cash key-binding registry and `READLINE_ARGUMENT`;
3. extend the current Reedline adapter where Reedline already has the required primitive;
4. port only small missing editing operations when that is cheaper than another backend;
5. keep ConPTY tests for terminal restoration and panic recovery.

Reconsider Rustyline only if Reedline is removed for an independent reason or Rustyline
later exposes caller-provided streams and a direct host-command yield. Neither condition
is true in 18.0.1.
