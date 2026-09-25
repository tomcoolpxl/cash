# `uutils/sed` evaluation for a Cash `sed` command

## Recommendation

Use `uutils/sed` as the source base for Cash's native `sed`, after the Bash 5.2
work. It is the best candidate found: it already builds natively on Windows, exposes
a `uucore`-style library entry point, uses the same `uucore` 0.12 line as Cash, and
has a substantial test suite.

Do not register the current source unchanged as `sed`. Focused probes found several
ordinary POSIX/GNU behaviors that need fixes first. Import the source once into a
private Cash crate, preserve its MIT notice and imported revision, and maintain the
small Cash-specific delta locally. A permanent fork or Git dependency would make the
required command-host and compatibility changes harder to control.

## Source reviewed

- Repository: [`uutils/sed`](https://github.com/uutils/sed)
- Revision: `c46dd6dab61a6a571bf08cb6b333e4fe1f3636c3`
- Revision date: 2026-09-21
- Crate/version: `sed` 0.2.0
- License: MIT
- Rust: edition 2024, MSRV 1.88
- Local reference clone: `C:\Users\thraa\github\sed-reference-lf`

The standalone implementation is about 11,400 lines of Rust. Its release-mode
Windows executable was 2,455,040 bytes, although the incremental size in Cash should
be lower because Cash already links `uucore` 0.12 and related dependencies.

## Why Microsoft Coreutils does not include it

GNU `sed` is a separate GNU package; it is not one of GNU Coreutils. The same project
boundary exists in Rust: `uutils/coreutils` and `uutils/sed` are separate repositories.
Microsoft's official package describes itself as a bundle of
[`uutils/coreutils`, `uutils/findutils`, and `uutils/grep`](https://github.com/microsoft/coreutils),
and its [Cargo manifest](https://github.com/microsoft/coreutils/blob/main/Cargo.toml)
contains those components but no `sed` dependency. Therefore the omission is expected,
not evidence that Microsoft evaluated and rejected this implementation.

## Build and test evidence

On native Windows at the reviewed revision:

- `cargo fmt --check` passed;
- `cargo clippy --all-targets --all-features -- -D warnings` passed;
- all 359 unit tests passed;
- 265 integration tests passed and 2 were intentionally ignored;
- `cargo build --release` passed.

The first checkout produced 47 integration failures because this repository has no
`.gitattributes` and the machine-wide `core.autocrlf=true` converted LF-sensitive
fixtures to CRLF. A clone made with `core.autocrlf=false` passed completely. Imported
source and fixtures must live under Cash's existing LF policy.

## Integration fit

The library exports:

```rust
pub fn uumain(args: impl uucore::Args) -> UResult<()>
```

That is almost exactly the interface adapted by `cash-coreutils-builtins` into Cash's
process-backed bundled-command registry. It gives `sed` the normal Cash pipeline,
redirection, working-directory, environment, Ctrl-C, and job-control behavior without
running the utility in the interactive shell process.

Two integration changes are required:

1. The GNU `e` command and `s///e` flag currently execute `/bin/sh -c` on Unix and
   `cmd.exe /C` on Windows. Route them through `cash -c` so nested commands use Cash's
   quoting, builtins, path rules, and process ownership.
2. The no-argument path calls `std::process::exit(1)` inside `uumain`. Child-process
   isolation prevents it from killing an interactive Cash, but the imported library
   entry should return a usage error instead.

## Compatibility probes

The reference was GNU sed 4.9 from Git for Windows. The table records exact focused
results from the Windows build.

| Probe | `uutils/sed` | Reference result | Assessment |
| --- | --- | --- | --- |
| `s/a/X/` | Correct | Correct | Pass |
| `sed -E 's/a|aa/X/'` on `aa` | `Xa` | `X` | Uses leftmost-first matching; POSIX requires the longest match at the earliest position. |
| BRE backreference with no locale or `LC_ALL=C` | Compilation error: backreferences are unavailable in byte mode | Matches | Release blocker for common scripts. It works under `C.UTF-8`, but Windows commonly has no POSIX locale variables. |
| `sed -n 'h;n;G;p'` on two lines | No output | Prints the second line followed by the first | The `n` command incorrectly ends the cycle instead of continuing with the next command. |
| `sed -i 's/a/X/' file` | Misparses the following script as the optional suffix/input | Edits the file | Known upstream issue; ordinary GNU `-i` syntax is a release blocker. |
| `$p` with a non-empty file followed by an empty file | No output | Prints the last line of the non-empty file | Known upstream incompatibility. |
| Duplicate label | Fatal error | Accepted by GNU sed | GNU compatibility gap; POSIX does not require duplicate-label behavior. |
| `s/a$/X/` on CRLF input | Leaves `a\r\n` unchanged | The native Git build prints `X\n` | Cash must deliberately define native CRLF text behavior and test `-i` preservation. |
| `e echo probe-ok` | Runs through `cmd.exe`, returning CRLF | Runs through the reference shell | Must use `cash -c` in the imported version. |

The upstream README already discloses the byte-mode backreference, duplicate-label,
trailing-empty-file, locale, and label parsing limitations. The `n`, leftmost-longest,
and current `-i` problems were confirmed independently and belong in Cash's gate.

## Why not use the `posixutils-rs` sed

The same [`posixutils-rs`](https://github.com/rustcoreutils/posixutils-rs) repository
reviewed for AWK includes a compact 2,620-line sed and 81 focused tests. It correctly
handled the leftmost-longest and `n` probes. It is a weaker Cash base because:

- it supports Linux and macOS rather than Windows;
- its regex layer is a wrapper around libc `regcomp`/`regexec`, which MSVC Windows
  does not provide;
- it has no `-i` support or broader GNU command-line surface;
- `s/\(ab\)\1/X/` on `abab` produced `Xab` rather than `X`, so basic BRE
  backreference matching is not yet correct.

It remains useful as a second behavioral reference, especially for POSIX cycle and
address semantics. It should not replace the uutils implementation.

## Implementation gate (COMPLETED & VERIFIED)

`crates/cash-sed` was created from the reviewed uutils source and integrated into Cash.
All gate requirements have passed:

1. **Leftmost-longest matching:** Implemented AST-level alternation sorting in `regex-syntax` to guarantee POSIX leftmost-longest match behavior. Tested: `sed -E 's/a|aa/X/'` produces `X`.
2. **BRE backreferences in default/C mode:** Removed the artificial byte-mode block in `fast_regex.rs`. Verified `s/\(.\)\1/X/` works under default Windows mode and `LC_ALL=C`.
3. **`n` cycle behavior, GNU `-i`, and trailing-empty-file `$:`**
   - Fixed `n` command cycle in `processor.rs` to output pattern (unless quiet), read next line, and continue to next command in script.
   - Added `require_equals(true)` and argument normalization for `-i` and `-iSUFFIX` in Clap.
   - Handled trailing empty files in `processor.rs` so `$` matches the last line of the last non-empty file.
   - Supported combined `g` and numeric flags (`s/pattern/replacement/2g`).
4. **Execution & isolation:** Routed `e` / `s///e` through Cash executable and replaced `std::process::exit(1)` in `uumain` with `Err(ExitCode(1).into())`.
5. **CRLF preservation:** Preserved CRLF in line reading and in-place file modifications in `fast_io.rs`. `$` correctly matches before `\r\n`.
6. **Differential tests:** 30 classic and esoteric sed tests run and pass against GNU sed in WSL (`tests/sed-differential.sh`). All 360 unit tests and 271 integration tests pass (`cargo test -p cash-sed`).
7. **Shell integration:** Registered `sed` in `crates/cash-shell/src/bundled.rs`, updated `crates/cash/src/doctor.rs` to mark `sed` as carried, and verified all 30 `resolution-honesty` tests pass.
