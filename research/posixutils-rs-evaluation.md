# `posixutils-rs` evaluation for a Cash `awk` command

## Recommendation

Use the `posixutils-rs` AWK implementation as the source candidate. It is a serious
base: its parser, bytecode interpreter, POSIX-oriented behavior, and test coverage are
substantial.

It is not a drop-in Windows component. Import only the AWK implementation and the small
pieces it actually needs into a private Cash crate, then replace its Unix regex, locale,
and process-host boundaries. Do not vendor the 140-utility workspace, create a tracking
fork, or depend on the repository as a whole.

## Source reviewed

- Repository: [`rustcoreutils/posixutils-rs`](https://github.com/rustcoreutils/posixutils-rs)
- Revision: `96bd8a372541cf7e4c427010a5414a40cf20c805`
- Revision date: 2026-09-25
- AWK crate/version: `posixutils-awk` 0.9.0
- License: MIT
- Rust: edition 2021, MSRV 1.84
- Local reference clone: `C:\Users\thraa\github\posixutils-rs-reference`

The AWK crate is about 14,147 lines of Rust. It contains a Pest parser, compiler,
bytecode VM, record/field engine, formatter, arrays, functions, I/O redirection,
`getline`, `system`, and the standard POSIX variables and builtins. The repository is
active, with roughly 4,700 commits and a current 0.9.0 release.

## Quality evidence

On Linux under WSL, at the reviewed revision:

- 292 unit tests passed;
- 111 AWK integration tests passed;
- `cargo fmt --all -- --check` passed;
- `cargo clippy --release --all-targets -p posixutils-awk -- -D warnings` passed.

Focused differential probes against GNU awk 5.4.1 also passed for:

- POSIX earliest/longest ERE selection (`a|aa` selects `aa`);
- repeated program sources and `-f -`;
- assignments interspersed with input operands, with the correct timing;
- multi-character `RS` as a documented extension;
- command `getline`, output pipes, `close()`, and `system()` status;
- runtime changes to `ARGC`/`ARGV` in the non-crashing cases;
- multibyte character counts and the ordinary field/record lifecycle.

These cover the compatibility boundaries that most directly affect ordinary scripts.

## Windows extraction boundary

The upstream project documents Linux and macOS as its platforms. A native MSVC Windows
build fails first on `LC_MESSAGES`. Bypassing that one constant exposes 155 compile
errors in the shared `plib`, because that library unconditionally builds many unrelated
Unix facilities.

That count exaggerates the AWK port. Runtime AWK uses only a narrow portion of `plib`:

- `diag::init_locale` and gettext startup;
- locale-aware upper/lowercase conversion;
- the POSIX regex wrapper.

Copying the AWK crate and replacing these three boundaries avoids porting user/group,
TTY, umask, SCCS, temporary-file, and other irrelevant Unix modules.

The real Windows work is:

1. **Regex:** upstream delegates to libc `regcomp`/`regexec`. MSVC Windows has no POSIX
   regex API. Cash needs a Rust ERE adapter that preserves earliest/longest matching.
   AWK does not use capture groups for regex replacement, which keeps this bounded, but
   bracket classes, anchors, empty matches, multibyte offsets, and `REG_NOTBOL` behavior
   need differential tests.
2. **Command execution:** `system()`, `command | getline`, and `print | command` use
   libc `system`, `popen`, and `pclose`. Replace them with one Cash command-host
   abstraction that launches `cash -c`, supports persistent read/write pipes, reports
   status, and participates in Cash job and Ctrl-C ownership.
3. **Locale:** replace libc `localeconv` and shared locale helpers with Cash's deliberate
   Windows policy. UTF-8 text, decimal-point formatting, and case conversion need clear
   tests; full Unix locale emulation is not required.
4. **Library entry:** upstream is a binary whose `main` calls `process::exit`. Expose a
   `fn(Vec<OsString>) -> i32`-shaped entry and run it through Cash's process-backed
   bundled-command shim.
5. **CRLF:** default record splitting must treat CRLF as one text record terminator in
   the same places Cash interprets lines, while explicit regex separators remain
   byte/character-defined.

## Robustness findings

The interpreter cannot be exposed unchanged because valid or diagnosable programs can
panic:

| Probe | Current result | Required result |
| --- | --- | --- |
| `a[1]=1; a[2]=2; delete a[2]` | Out-of-bounds panic in `array.rs` | Delete the final stored pair safely. |
| `delete ARGV[2]` with two operands | Same out-of-bounds panic | Continue input selection with the element absent. |
| Function uses a scalar actual argument as an array | `unreachable!` panic in `stack.rs` | Report the AWK type error and exit nonzero. |

The array bug is a small `swap_remove` bookkeeping error: after removing the last pair,
the code indexes the old final position. It also explains the `ARGV` crash. The function
argument case needs an explicit runtime type error. A 26-case boundary probe found these
two panic classes and no parser panics, but the VM contains substantial unsafe pointer
and stack code. Cash's hard gate is broader: malformed input, type errors, I/O errors,
and valid scripts must never unwind out of the command. Add a panic-focused regression
corpus and convert reachable `unwrap`, `expect`, `unreachable!`, and raw-index failures
to ordinary AWK diagnostics.

Process-backed dispatch limits a missed panic to the utility child, but that is a final
containment layer, not a substitute for fixing known panics.

## Other useful code in the repository

The workspace is a useful POSIX behavior reference, not a package Cash should absorb
wholesale.

- **`sed`:** useful as a second reference, but not the chosen implementation. It is
  Unix-only, has no `-i`, and a basic BRE backreference probe produced the wrong result.
  The separate [uutils sed evaluation](uutils-sed-evaluation.md) explains the choice.
- **`grep`:** Microsoft Coreutils already packages the uutils Rust implementation for
  Windows, whose `uucore` shape is a better fit if Cash later chooses to bundle grep.
- **`stat`:** already exists in uutils coreutils; no new source import is needed.
- **`diff` and `patch`:** plausible future candidates because Cash currently bundles
  neither, but they need their own Windows and compatibility evaluations. Their presence
  does not justify importing the shared Unix workspace.
- **`bc`, `m4`, `make`, `vi`, and the remaining utilities:** outside the current native
  text-tools goal and often tied to Unix facilities. Keep them out of the roadmap unless
  a concrete Cash use case appears.

## Implementation gate

After Bash 5.2 is complete:

1. create `crates/cash-awk` from the reviewed AWK source and preserve the MIT notice and
   exact imported revision;
2. split binary setup from the parser/compiler/interpreter and add a non-exiting library
   entry;
3. implement and differentially test the Windows POSIX-ERE adapter;
4. replace all `system`/`popen`/`pclose` use with the Cash command host;
5. remove the irrelevant `plib` dependency and implement the narrow locale policy;
6. fix the known panics and complete a targeted unsafe/stack/array robustness audit;
7. add Linux differential tests against an actual POSIX AWK and native Windows tests for
   CRLF, paths, command pipes, environment, Ctrl-C, and exit status;
8. register `awk` through the process-backed bundled-command shim, then update
   `cash doctor`, README, and the specification.

The command may ship as `awk` when the POSIX core and robustness corpus pass on both
platforms. GNU-only language extensions remain outside the initial gate.
