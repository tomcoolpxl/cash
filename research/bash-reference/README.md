# Bash source comparison — 2026-09-23

This is a focused compatibility review, not an exhaustive Bash conformance claim.
The separate [Bash 5.2 gap audit](bash-5.2-gaps.md) records known gaps outside
this probe set. The [remaining compatibility plan](bash-5.2-remaining-plan.md)
turns its unverified NEWS items into ordered probes and implementation phases.
It found **12 mismatches in 35 selected probes**; 23 matched stdout and exit status.
Two original mismatches concerned new Bash 5.3 features, outside cash's advertised 5.2 interface.
Diagnostics are retained but are not compared byte-for-byte.

**Resolution:** All seven numbered Bash 5.2 gaps below have code changes and
Windows integration regressions in `crates/cash/tests/bash_gaps.rs`. The same
35-probe run now matches on **all 35 cases**, including the two Bash 5.3 cases.
`results.json` contains the latest run. The mismatch descriptions
below describe the original findings and remain as the rationale for the fixes.

## Reference and reproduction

GNU Bash was **cloned**, without vendoring it into cash:

- Remote: https://git.savannah.gnu.org/git/bash.git
- Checkout: `C:/Users/thraa/github/bash-reference`
- Commit: `9c465866b7d849378369ef700cbe2965aa9691e3` (Bash 5.3 patch 20).
- Clone is shallow (`--depth 1`); the working source and upstream tests are present.
- Runtime oracle: the installed `C:/Program Files/Git/bin/bash.exe`, reporting
  **5.3.15(1)-release (x86_64-pc-cygwin)**. The cloned C source was inspected, not built.
- Cash baseline: `50f596fd3ec486c23239283e8a4233a42da5a322`, plus the local xargs
  resolution and line-ending documentation changes already in progress.

Run from the cash repository with PowerShell 7:

```powershell
cargo build -p cash
./research/bash-reference/probe.ps1
```

`probe.ps1` accepts `-Bash` and `-Cash` executable paths. It runs independent shell
processes, closes stdin, captures stdout/stderr/status, and bounds each probe to
10 seconds. Startup-file probes write under `target/bash-reference-probes`.
`results.json` contains exact scripts and observed results; `provenance.json`
records the binaries and source revisions used. This is an observational tool:
it reports known mismatches rather than making the current CI red.

These probes use shell-language behavior rather than POSIX path, signal, or
permission assumptions. The MSYS/Cygwin oracle is useful for those checks but is
not evidence that cash should adopt POSIX process semantics.

## Confirmed gaps in the existing Bash interface

### 1. `wait %job` discards failure status — fix first

```bash
bash -c "exit 7" & wait %1; echo status:$?
```

Bash prints `status:7`; cash prints `status:0`. The corresponding `wait $!` probe
returns 7 in both shells. Scripts that check background jobs by job specification
can therefore report success after a failure.

- Cash: `crates/cash-builtins/src/wait.rs:50` calls `job.wait().await?` and discards
  the returned result in the job-spec branch, unlike the PID branch.
- Bash: `builtins/wait.def:316` assigns the result of `wait_for_job` to `status`.
- Follow-up: preserve that result and test job specs, PIDs, multiple operands,
  non-existent jobs, and the separate no-operand behavior.

### 2. Pattern replacement interprets dollar data as regex replacement syntax

```bash
x=abc; r='$1'; echo "${x/b/$r}"
```

Bash prints `a$1c`; cash prints `ac`. This silently changes replacement data.

- Cash: `crates/cash-core/src/expansion.rs:2236` passes the replacement string
  directly to `fancy_regex::Regex::replace` / `replace_all`.
- Bash: `subst.c`, particularly replacement preparation around lines 9430–9443,
  implements shell replacement rules rather than a regex engine's `$1` syntax.
- Follow-up: treat literal replacement bytes literally, then implement Bash's
  replacement-specific expansion rules. Do not merely escape every special
  character: the ampersand behavior below needs separate handling.

### 3. `patsub_replacement` is advertised and enabled but not applied

```bash
x=abc; echo "${x/b/[&]}"
```

Bash prints `a[b]c`; cash prints `a[&]c`. With `shopt -u patsub_replacement`, both
print `a[&]c`. Bash's NEWS lists this option among the Bash 5.2 changes, so this
is relevant to cash's stated compatibility version.

- Cash: `options.rs:253` defaults it to true; `namedoptions.rs:815` exposes it.
  The replacement path in `expansion.rs:1746` / `2236` does not use it.
- Bash: `subst.c:9443` gates replacement expansion on this option;
  `builtins/shopt.def:253` exposes it.
- Follow-up: cover unquoted, quoted, escaped, and variable-supplied ampersands,
  plus first/all/prefix/suffix replacement. Preserve quoting information.

### 4. `local -` succeeds but leaks changed options

```bash
set +u
f() { local -; set -u; }
f
case $- in *u*) echo leaked;; *) echo restored;; esac
```

Bash prints `restored`; cash prints `leaked` and emits a warning. A function can
unexpectedly enable nounset or other options for its caller.

- Cash: `crates/cash-builtins/src/declare.rs:367` explicitly returns success
  without saving options.
- Bash: `builtins/declare.def:115` specifies save/restore; upstream
  `tests/func.tests:205` exercises it.
- Follow-up: attach an option snapshot to the function scope and restore it on
  every return/unwind path, including nested functions and errors.

### 5. POSIX startup still sources `BASH_ENV`

```bash
printf 'echo startup\n' > startup.sh
BASH_ENV=./startup.sh bash --posix -c 'echo body'
```

Bash prints only `body`; cash prints `startup` then `body`. Ordinary non-POSIX
`BASH_ENV` loading matched, so the existing implementation is useful but its
conditions are too broad.

- Cash: `crates/cash-core/src/shell/initscripts.rs:192` selects and loads the
  noninteractive environment file without the equivalent POSIX-mode gate.
- Bash: `shell.c:1216` checks `posixly_correct == 0`, `act_like_sh == 0`, and
  `privileged_mode == 0` before reading `BASH_ENV`.
- Follow-up: model the startup matrix explicitly: interactive/noninteractive,
  login/non-login, bash/sh, POSIX mode, and privileged mode. Only the cases in
  `results.json` were executed here; this is not a claim that every matrix cell
  is broken.

### 6. `wait -n`, `-p`, and `-f` are missing

The three probes return 99 in cash rather than the child's status; `-p` does not
populate its destination variable. These matter for worker pools and scripts
that launch several background commands.

- Cash: `crates/cash-builtins/src/wait.rs:33–40` explicitly rejects the options.
  The combined `-n -p` probe reaches the `-n` rejection first; the independent
  `-p` rejection is established by source inspection.
- Bash: `builtins/wait.def:238` uses `wait_for_any_job`; `tests/jobs.tests:28`
  starts its wait-next regression coverage. NEWS records `-p` before Bash 5.2.
- Follow-up: preserve completed-job statuses and identifiers; define how cash's
  task-only jobs participate instead of assuming every job has a process ID.

### 7. `mapfile` callbacks and `printf %n` are missing

- `mapfile -t -C cb -c 1 a <<< hello` fails with status 99 in cash; Bash invokes
  the callback and populates the array. Cash's explicit rejection is at
  `crates/cash-builtins/src/mapfile.rs:55`. Bash implements callback dispatch in
  `builtins/mapfile.def:108`; `tests/mapfile.tests:26` provides upstream cases.
- `printf 'abc%n\n' n; echo "$n"` gives `abc` then `3` in Bash. Cash rejects the
  format. Cash delegates formatting to uucore in `printf.rs:69`; Bash's
  `builtins/printf.def:654` handles `%n` by assigning a shell variable.
  `tests/printf.tests:279` covers the counter behavior.

Neither feature follows automatically from bundling a utility implementation:
they need access to shell state. Prioritize these behind silent data/status
corruption, unless an actual user script needs them now.

## Bash 5.3: selected features, not a version claim

- `${ command; }` performs command substitution in the current environment.
  The original probe returned `new:value` in Bash and `old:old` in cash; both now
  return `new:value`. The `${| command; }` form also uses a temporary `REPLY`.
- `compgen -V array` now stores candidates in an indexed array and clears it
  when there are no matches.

Both appear in the cloned `NEWS` under the 5.3 changes. These selected cases
do not establish full Bash 5.3 compatibility.

## Assumptions this review supports or corrects

- Selected tests of `set -e` suppression inside condition-tested functions,
  command-substitution errexit inheritance, `pipefail`, `PIPESTATUS`, `lastpipe`,
  assignment status, dynamic function scope, namerefs, integer expressions,
  IFS empty fields, empty and negative-index arrays, `read`, and ERR/RETURN traps
  matched. This is targeted evidence, not exhaustive coverage of those features.
- **`xargs` is not a Bash builtin.** Bash source cannot supply its semantics;
  GNU findutils is the relevant upstream. Cash's new builtin-first xargs dispatch
  is a deliberate extension of its bundled userland, not proof of Bash parity.
  Keep its explicit-path and `enable -n` behavior tested and documented.
- The compatibility runner's comment says conformance runs on Linux CI and a
  reference Bash is unavailable on Windows. The checked-in CI workflow currently
  has only a Windows job, and `compat_tests.rs:159` skips the differential suite
  on Windows. The Bash executable used by these probes demonstrates that useful
  non-PTY differential tests can run on this Windows machine.
- The earlier 1,165 passing workspace tests therefore did not establish Bash
  conformance. Keep Win32 process tests, but add a separate language-only oracle
  job with an explicit Bash path/version. Do not remove Windows-specific tests
  or blindly enable PTY-dependent upstream suites.
- README's claim of twelve documented divergences is stale: spec §4 already has
  25 numbered entries, plus the newly documented xargs behavior in D48.

## Useful next work from this source checkout

1. Fix lost `wait %job` status and literal replacement corruption, with native
   regression tests that also run in GitHub's existing Windows job.
2. Implement `local -` restoration and correct the startup-mode gates.
3. Add quoting-aware `patsub_replacement` handling and edge-case tests.
4. Build a curated, non-PTY differential suite; use the cloned Bash tests to
   identify cases, and write small independently authored regressions for cash.
5. Treat `wait` options, callbacks, and newer Bash syntax as explicit capability
   work rather than discovering each through a broken user script.

The cloned source stays outside the MIT cash tree. This review adds references
and independently written probes, not copied Bash implementation code.
