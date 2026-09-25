# `quinnjr/rawk` evaluation for a Cash `awk` builtin

## Decision

**Use it.** Copy the useful source into the Cash tree once, preserve its MIT
attribution, and maintain the copy as Cash code. It is a good starting point for a
native `awk` command and is much cheaper than writing an AWK parser and interpreter
from scratch.

Do **not** expose the current source unchanged as `awk`. The repository describes
itself as “100% POSIX-compatible”, but focused probes found basic POSIX mismatches.
Those gaps are bounded and fixable; they are release gates for the `awk` name, not a
reason to reject the implementation.

The best Cash integration is a **bundled, process-backed builtin**, using the same
shim mechanism as bundled coreutils. `type awk` should identify a Cash builtin, while
execution re-enters `cash.exe` as a child process. That gives AWK normal pipeline,
redirection, Ctrl-C, job-control, working-directory, and environment behavior without
letting an AWK panic or infinite loop take down or block the interactive parent shell.

## Source reviewed

- Repository: [`quinnjr/rawk`](https://github.com/quinnjr/rawk)
- Revision: `5834140f2941ef7f565e6f342bd0be8c265b8207`
- Upstream package and binary name: `awk-rs` 0.2.0
- Review date: 2026-09-25
- Local reference clone: `C:\Users\thraa\github\rawk-reference`
- License: MIT OR Apache-2.0. The Cash copy can use the MIT option while retaining
  Joseph R. Quinn's copyright and the required license notice.

The upstream `AGENTS.md` contains contributor rules for the upstream repository. It
was read as architectural context; it is not an instruction file for Cash. The user's
requested ownership model is the one used here: a one-time source import, no fork and
no obligation to track upstream.

## Why it is a good base

The source is small enough to own:

- about 7,828 lines of Rust under `src/` and 4,176 under `tests/`;
- only `regex` and `thiserror` as runtime dependencies, both already present in the
  Cash dependency graph;
- a clean public lexer/parser/interpreter split;
- interpreter input and output are generic `BufRead` and `Write`, rather than being
  permanently tied to process standard streams;
- errors are returned through a typed error enum;
- AWK's common language surface is already present: patterns/actions, fields,
  associative arrays, functions, control flow, `getline`, print/printf, redirections,
  standard string/math functions, and several gawk extensions;
- the current code builds and runs natively on Windows.

The standalone optimized executable was 1,847,808 bytes on the review machine. That
is an upper bound, not the expected increase to `cash.exe`, because Cash already links
the two runtime dependencies.

## Verification performed

At the reviewed revision, on native Windows with Rust 1.98.1:

- `cargo clippy --all-targets --all-features -- -D warnings` passed;
- `cargo fmt --all -- --check` passed;
- debug and release test suites passed;
- 655 unit, CLI, end-to-end, and documentation tests actually ran and passed;
- another 34 tests were reported as passing but merely return early when `gawk` is
  absent, so they are not differential-test evidence on this machine;
- ordinary programs still worked after removing `sh` from `PATH`.

The repository's test volume is useful, but many tests assert the implementation's
own output rather than comparing it with another AWK. Cash should keep the tests and
add a compact conformance suite that always runs against a known reference AWK in CI.

## Verified gaps

These are observed failures, not speculative completeness concerns.

| Area | Current `awk-rs` result | Reference/POSIX result | Cause |
| --- | --- | --- | --- |
| Non-newline `RS` | `RS=":"` reads `a:b:c` as one record | Three records: `a`, `b`, `c` | The main reader handles only newline and paragraph mode. |
| POSIX ERE choice | `match("aa", /a|aa/)` gives `RLENGTH=1` | `RLENGTH=2` | Rust `regex` uses leftmost-first matching; POSIX ERE requires leftmost-longest. |
| Multiple `-f` files | Runs only the last program file | Concatenates the files in option order | Each `-f` overwrites `program_source`. |
| Operand assignments | Treats `x=ok` as a filename and exits 2 | Applies the assignment before the following input file | The CLI treats every post-program operand as an input path. |
| Native Windows `system()` | Returns `-1` when no `sh.exe` is installed | Runs the command through the implementation's command processor | `system()` hardcodes `sh -c`. |
| Native Windows command pipes | `cmd | getline` returns `-1`; print pipes fail to spawn | Commands run and stream data | Both pipe paths also hardcode `sh -c`. |
| `-f -` | Attempts to open a file literally named `-` | Reads the AWK program from standard input | The CLI calls `fs::read_to_string("-")`. |

The first four discrepancies were also checked against the installed reference
`awk.exe`; its results match the POSIX requirements. POSIX explicitly requires
multiple `-f` programs to be concatenated, permits file and assignment operands to be
intermixed, and defines records through `RS`. See the
[POSIX `awk` specification](https://pubs.opengroup.org/onlinepubs/9699919799/utilities/awk.html).
The Rust regex engine documents that it does not implement POSIX leftmost-longest
matching; see [`regex-automata::MatchKind`](https://docs.rs/regex-automata/latest/regex_automata/enum.MatchKind.html).

The Windows CI matrix does not invalidate the `sh` finding. GitHub's
[Windows runner image](https://github.com/actions/runner-images/blob/main/images/windows/Windows2025-Readme.md)
includes Git and Bash, so a test that launches `sh` can pass there while failing on a
clean native Windows installation. Cash must never make Git Bash an undeclared runtime
dependency.

## Integration design

### 1. Absorb a dedicated crate

Create a private workspace crate such as `crates/cash-awk` from the upstream library
source. Record the imported revision in a `NOTICE` or `UPSTREAM.md`, retain the MIT
license and copyright, and remove upstream release, benchmark, and repository
administration files. This is copied Cash source, not a Git dependency or a fork that
needs periodic synchronization.

Keep the lexer, AST, parser, value model, formatter, and interpreter. Replace the
standalone `main.rs` with a reusable entry point shaped like:

```rust
pub fn uumain(args: Vec<OsString>) -> i32
```

That matches Cash's bundled-command registry and keeps `process::exit` out of library
code.

### 2. Run it through the bundled-command shim

Register `awk` with `cash-shell`'s bundled registry. The existing shim spawns the
current Cash executable and already preserves the shell's real standard handles,
pipeline behavior, job tracking, current working directory, and exported environment.
This also contains the imported interpreter's remaining `unsafe` input pointer and
defensive `unwrap()` sites within a child process while they are removed or proved.

An in-parent `cash-builtins::Command` adapter is a worse first integration: AWK is
synchronous, can run forever, opens files and subprocesses, and needs interruptible
job semantics. There is no useful shell-state mutation to justify running it inside
the parent process.

### 3. Replace the `sh` dependency with a Cash command host

The copied interpreter needs one owned abstraction for:

- `system(command)`;
- `command | getline`;
- `print ... | command` and `printf ... | command`;
- closing and waiting for those child commands.

On Cash, that host should spawn the current executable in normal `cash -c COMMAND`
mode, with the requested pipe direction. It must use the AWK child's current directory
and environment. It must not call `cmd.exe`, PowerShell, Git Bash, or a `sh` found on
`PATH`.

Because the bundled AWK already runs as a child of the interactive shell, nested
`cash -c` processes are acceptable here and keep the interpreter synchronous. An
eventual direct async host can optimize this later if measurements justify the extra
complexity.

### 4. Fix the compatibility gate before naming it `awk`

Before registration, fix and test:

1. arbitrary single-character `RS`, plus paragraph mode and CRLF;
2. a POSIX leftmost-longest ERE implementation for match, split, field separators,
   and substitution;
3. concatenation of repeated `-f` inputs and `-f -`;
4. interspersed command-line assignments with the correct timing around `BEGIN`,
   each input file, and `END`;
5. live `ARGC`/`ARGV` behavior needed to select or suppress input operands;
6. all three command-execution paths through the Cash host;
7. errors and exit statuses under the command name `awk`.

Run the imported tests on Windows and Linux, then add differential probes against GNU
awk and at least one small POSIX implementation. The four probes above belong in that
suite permanently because they found holes in the upstream claim immediately.

### 5. Harden the owned copy

The interpreter uses a thread-local, lifetime-erased raw pointer so bare `getline` can
reach the active input reader. Its guard documents a plausible invariant, but Cash can
avoid owning that unsafe boundary by placing the active reader in interpreter state or
passing an input context through expression evaluation. Production source also has a
small number of invariant-based `unwrap()` calls. Convert the subprocess-handle cases
to ordinary errors first; leave only locally proved collection lookups.

## Scope recommendation

Ship the POSIX core and the already implemented low-cost gawk conveniences. Do not
promise complete gawk compatibility. Two-way pipes, network pseudo-files, and
`@include` are explicitly absent upstream and are not needed for the initial Cash
command.

Once the compatibility gate passes, expose it as **`awk`**, because that is the command
scripts need. An additional `rawk` or `awk-rs` name adds no useful contract. Update the
README's current statement that Cash does not carry `awk` only in the implementation
change that actually registers the command.

This is a moderate integration, not a rewrite. The expensive part of AWK—the language
front end and ordinary interpreter—is already present. The required work is mostly at
the command-line, record-reader, regex, and process-host boundaries. Full gawk parity
would be a much larger and unnecessary goal.
