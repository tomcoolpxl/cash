# Building and contributing

## Build

```bash
cargo build                                    # target\debug\cash.exe
cargo xtask ci quick                           # format check, clippy, docs, the quick tests
cargo xtask test slow                          # the slow tests: cash.exe's, rar's, sed's
cargo xtask ci full                            # all of it, as before a release
powershell -File scripts\tidy.ps1 -Report      # delete unused build output, show sizes
```

Windows 11 (or Windows 10 1809+, for ConPTY). Rust 1.99, which `rust-toolchain.toml` pins
and rustup installs on the first build; it is also the oldest Rust that builds cash. The
tests run through cargo-nextest (`cargo binstall cargo-nextest`), which gives each test a
process of its own.

Build output is disposable, and each checkout has its own `target\`:

- **Profiles.** `dev` and `test` keep only line numbers as debug info (enough for panic
  backtraces), `--profile debugging` keeps all of it. `release` is thin LTO and builds
  quickly; `dist`, fat LTO with one codegen unit, is what the release workflow ships.
- **The shell you use** is a released one, installed by Scoop. Nothing runs from
  `target\`, so any of it can be deleted at any time.
- **Cleanup.** `scripts\tidy.ps1` deletes build directories not built for a week, what
  removed worktrees leave behind, and old installed copies, skipping anything in use;
  `-All` takes every build directory. Claude Code runs it when a session starts
  (`.claude\settings.json`). Do not point worktrees at one shared target directory: cargo
  locks it, so builds queue behind each other, and it can mix up the worktrees' crates
  ([cargo#12516](https://github.com/rust-lang/cargo/issues/12516)).
- **Features.** Local builds use the default features, so that clippy and the tests share
  one build; CI also lints with `--all-features`.

## Tests

Two lanes, set in `.config/nextest.toml`: a plain `cargo nextest run` is the quick lane,
every crate's own tests; `--profile slow` and `--profile full` add the ones that drive
`cash.exe`, rar's and sed's ported suites. CI runs both on every push, and the release
workflow runs both again on the commit it releases.

Tests that drive `cash.exe` are modules of one test executable,
`crates/cash/tests/it`: add a file there and a `mod` line to its `main.rs`, rather than
a new file directly under `tests/`, which would be one more executable linking the
whole shell. `tests/` at the root holds the differential corpus, ~900 scripts whose
output is frozen from Git Bash 5.3, and the git-prompt check.

The checks that need a person, a UAC prompt or a second account, are listed in
[docs/try-sudo.md](docs/try-sudo.md).

## Line endings

Source files use LF even on Windows. `.gitattributes` defines Git's line-ending
policy, and `.editorconfig` asks editors to save matching endings. Windows scripts
(`.bat`, `.cmd`, `.ps1`) use CRLF; byte-sensitive CRLF/BOM fixtures are exempt from
normalization. No global `core.autocrlf` change is needed.

## Layout

```
crates/cash              the binary
crates/cash-win32        the Win32 semantics layer: job objects, signals, paths, encoding
crates/cash-core         shell runtime: expansion, control flow, traps
crates/cash-parser       bash grammar: tokenizer and parser
crates/cash-builtins     the standard builtins
crates/cash-interactive  line editing, history, completion
crates/cash-shell        the shell library that crates/cash drives
crates/cash-man          the help pages and topics, markdown embedded at build time
crates/cash-coreutils-builtins  uutils coreutils, as builtins
crates/cash-sed          sed, from uutils
crates/cash-awk          awk, from posixutils-rs
crates/cash-bc           bc, from posixutils-rs
crates/cash-archive      tar, zip, 7z, rar and the compressors
crates/cash-figlet       figlet's fonts and rendering
crates/cash-sextant      the game
crates/cash-test-harness runs the YAML compatibility cases
vendor/                  patched copies of crossterm, reedline and uutils crates
tests/                   the differential corpus and the git-prompt check, with frozen output
packaging/               the Scoop manifest, the Inno Setup script, the winget templates
assets/                  the logo, and the demo tapes and GIFs the README shows
research/                the designs and evaluations behind the larger pieces
docs/                    the records: spec, decisions, roadmap, releases, open issues
xtask/                   test orchestration
```

The `cash-core`, `cash-parser`, `cash-builtins`, `cash-interactive`, `cash-shell`,
`cash-coreutils-builtins` and `cash-test-harness` crates are **absorbed from brush** at
upstream commit `737dd57` and modified. Both projects are MIT, so this is a
straightforward absorption rather than a dependency; see [NOTICE](NOTICE) for attribution
and the list of changes. `cash-sed`, `cash-awk` and `cash-bc` were absorbed later, from
uutils and posixutils-rs. `cash-win32` stays a separate crate so the Windows semantics
and the shell language remain separable.

Every builtin has a help page in `crates/cash-man`, with one entry for each in its
catalogue; a test fails when a builtin has none. Help, `--help`, warnings and `cash
doctor` never cite the spec or the roadmap: those are for this repository, not for the
person at the prompt.

## The records

- [docs/spec.md](docs/spec.md): the decisions, each with the reasoning behind it, and
  the deliberate divergences from Bash.
- [docs/decisions.md](docs/decisions.md): the later decisions, by topic, under the same
  numbers.
- [docs/ROADMAP.md](docs/ROADMAP.md): the feature order, and what each item turned into.
- [TODO.md](TODO.md) and [docs/DONE.md](docs/DONE.md): what is open, and what was done in
  which release.
- [docs/open-issues.md](docs/open-issues.md): what was seen and is not fixed, so that it
  is not rediscovered.
- [docs/compatibility.md](docs/compatibility.md): the Bash 5.2 and 5.3 audits.
- [docs/RELEASING.md](docs/RELEASING.md): how a release is made, and the commit message
  convention the release notes are grouped by.

## Commits

Conventional Commits: `type(scope): description`, as in `fix(doctor): Scoop's install
line passes PowerShell's execution policy`. The release notes are generated from the
types; [docs/RELEASING.md](docs/RELEASING.md) has the list.
