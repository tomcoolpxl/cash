# To do

Everything found while working on cash that is not done yet, in one list, and the plan
for working through it.

## How this is worked

- **One session, one item at a time, in the order below.** No further worktrees and no
  spawned sessions: what is found on the way is added to the phase it belongs to, or to
  the end, not started beside it.
- **Per item:** reproduce it (the code has moved since the item was written down),
  compare with Git Bash 5.3, fix it with a test that fails without the fix, and commit.
  A done item is deleted here in the same commit.
- **Per phase:** `cargo fmt --all`, `cargo clippy --workspace --all-targets`, the full
  nextest run with `--retries 0`; then `main` is brought up to the branch and pushed,
  the phase is released once CI is green, and a short report is written. The next phase
  starts without waiting. The work stops only for a decision marked **yours** below, or
  for a release whose CI fails.
- **Releases:** each phase ends with a release of its own. A tag is made only after CI
  has passed on the commit (RELEASING.md). So far: 1.3.0 is what was on `main` on
  2026-09-30, 1.3.1 the test suite (phase 1), 1.3.2 Ctrl-C everywhere (phase 2), 1.3.3
  process substitution (phase 3), 1.3.4 the `/dev` names and the bundled tools (phase 4),
  1.3.5 jobs, `wait` and the tools beside them (phase 5), 1.3.6 the Tab menu opening on
  the history hint's candidate (asked by the user on 2026-10-02, outside the phases),
  1.3.7 the shared part on the first Tab and the grid on the second (phase 7), 1.3.8
  crashes, security and CI hardening (phase 8), 1.3.9 the shell's own working directory,
  environment, PATHEXT and umask (phase 9), 1.3.10 awk, sed and bc (phase 10), 1.3.11 pipelines, jobs and process
  substitution (phase 11), with the two parser bugs phase 11 found (`( ( … ) )` and
  `--file=<(…)`), fixed while CI was put right.
- An item says what was seen and what Bash does; a cause only where it was looked for.
  Longer notes on a thing that stays open belong in `open-issues.md`.

Collected on 2026-09-30 from every cash session of that day and the day before. All of
their work is in `main`; the sessions themselves are not needed any more. The order and
the grouping were chosen that day: the test suite first, so that every later phase can
trust its runs, then by what a user notices most.

Phases 1 to 11 are done. Phase 6 changed no code and had no release of its own.

Phases 8 to 14 are the code review of 2026-10-02. `REVIEW_REPORT.md` has the evidence
and the reasons; the IDs (EXE-01, TXT-01, …) are its findings, and R1 to R11 its
recommendations. Each item was reproduced against Git Bash 5.3 during the review unless
it says "from reading". Items that share a fix are one item. The order was chosen by the
user on 2026-10-02: what can hurt a user first, then what removes a whole class of bugs,
then the riskiest changes once the crashes and state bugs are out of the way.

| Phase | Release | What |
| --- | --- | --- |
| 8 | 1.3.8 | Crashes and security, and CI hardening |
| 9 | 1.3.9 | The shell owns its state |
| 10 | 1.3.10 | awk, sed and bc |
| 11 | 1.3.11 | Pipelines, jobs and process substitution |
| 12 | 1.3.12 | Language |
| 13 | 1.3.13 | Builtins and error output |
| 14 | 1.3.14 | Tests, records and leftovers |

---

## Phase 12. Language (R7)

### 12.3 Smaller expansion differences

Tab turns `ls ~/Doc` into `ls C:/Users/me/Documents`; Bash keeps `~/Documents`, as
`compgen` now does. Keeping it means the auto-quoting (D40) has to leave a leading `~/`
outside the quotes (`~/'my dir/'`), or the tilde is no longer expanded. Found fixing
`compgen`'s word.

Also seen while fixing 8.1: past the function depth limit (500, or `FUNCNEST`) cash
reports "maximum function call depth exceeded" and the script goes on, `$?` 0; Bash 5.3
reports "maximum function nesting level exceeded (500)" and abandons the command.

### 12.4 extglob `!(…)` with alternatives

Last in the phase, the largest single change: `echo !(*.tar|*.tar.gz)` lists
`x.tar.gz`; `[[ ab == !(a|ab) ]]` is true. The atomic group in
`cash-parser/src/pattern.rs:152` cannot be patched right; a backtracking matcher for
extglob patterns. LANG-01.

---

## Phase 13. Builtins and error output (R4, R8)

### 13.1 Batch files get stray carets

`./show.bat "Q&A notes.txt"` receives `Q^&A notes.txt`, `"a | b"` receives `a ^| b`:
`escape_for_cmd` escapes inside the quotes it added (`cash-win32/src/cmd.rs:81-92`).
Track cmd's quote state; a round-trip test through a real `.bat`. W32-01.

### 13.2 Builtins

- `kill $a $b` refuses the second operand. BI-03.
- `find d -delete` cannot remove directories (pre-order). BI-04.
- `chmod go-w f` makes `f` read-only for its owner; `u+rw,go-w` is refused. BI-05.
- `chmod +x` prints "execute: not represented"; make it silent, as D23 says (decided,
  below). BI-17.
- `[ a -ef b ]` is "not supported"; `stat` on a directory gives inode 0: one
  `cash_win32::fs::file_info` with backup semantics. XC-3, ARCH-06.
- `mapfile -t` keeps the `\r` (D20 names mapfile). XC-4.
- `printf '%d' abc` exits 0; `\c` in `%b` does not stop reuse. BI-09, BI-10.
- `find -exec echo "<{}>"`, `-exec … +` batching, silent spawn errors; loops under `-L`
  and `chmod -R` through junctions. BI-11, BI-13.
- Two `kill -STOP` need two `-CONT` (W32-05); `kill -9 PID` kills the tree, against D22
  (W32-06); `#!/usr/bin/env -S` (W32-08).
- `elevate` asks UAC through `powershell.exe` started with the process's folder and
  environment, so the elevated command gets neither the shell's working directory nor its
  exported variables (the `process state:` mark at `cash-builtins/src/win.rs`); with
  BI-19's quoting, through `ShellExecuteExW("runas")` with the shell's folder.
- A syntax error in `eval` ends a script with status 2; Bash reports it, sets `$?` to 2
  and goes on (`eval 'if then'; echo "after $?"` prints `after 2`). Seen in 1.3.10 while
  fixing PI-07.

### 13.3 `time` and `times` report CPU time

They show 0 user and sys. The shell's own from `GetProcessTimes`, children's from the
job objects' accounting (decided, below). EXE-11, XC-5.

### 13.4 Error output: colour, prefixes, `--help`

Errors are ANSI-coloured into pipes and files and ignore `NO_COLOR`
(`cash-shell/src/entry.rs:675`); three prefixes (`error:`, `cash:`, `name:`); `--help`
goes to stdout or stderr, exit 0 or 2, by builtin. One "colour this stream" helper. XC-8,
BIN-04, BI-16.

A warning is printed as an error: `error: warning: c1: circular name reference`, where
Bash says `script: line 6: warning: c1: circular name reference`. Found in phase 12.

An array index before the start (`a=(1 2)`): `${a[-3]}` expands to nothing in both
shells, but Bash also says `a: bad array subscript`, and cash says nothing; `a[-3]=x`
ends both with 1, cash saying `array index out of range: -3`. Found in phase 12.

Arithmetic errors name neither the line nor the expression: cash says `error: arithmetic
evaluation error: division by zero` and `failed to parse expression: 1 +` (and `08`, and
`i = ` in `for ((`); Bash says `script: line 14: 1/0: division by 0 (error token is
"0")`, `((: 1 + : arithmetic syntax error: operand expected (error token is "+ ")`,
`08: value too great for base (error token is "08")`, and `read: 1/0: …` from `read`.
Found in phase 12.

### 13.5 D36: measure the pooled prompt job, then build it or drop it

Every prompt spawn creates a job object and sweeps the registry; D36's pool was never
built. Time a Starship prompt with and without the job creation. Build the pool if it
saves more than about 5 ms a prompt; otherwise amend D36 and D6's exception table, with
the measurement (decided, below). EXE-13, W32-10.

---

## Phase 14. Tests, records and leftovers (R10, R11)

### 14.1 Tests

Freeze `tests/corpus` and the awk, sed and git-prompt differential outputs as goldens in
`it` (D43 rests on `cases/brush`, which has no language cases); move the remaining `it`
modules to `it/common.rs`; unique temp folders for the 19 fixed `%TEMP%` names; drop the
test that needs the author's `kali-linux` WSL or make it skip loudly. BIN-05, BIN-19,
BIN-20.

### 14.2 Records

spec.md: D1 (5.3.15, not 5.2.37), §1 and §4 row 19 (`stat` is carried), §5, D6 and §6
(the spawn race is closed), the D9 table; README Layout; RELEASING's crate list; the
MSRV (1.88 vs 1.95, a missing `msrv-policy.md`). ARCH-05, ARCH-13, ARCH-14, EXE-17.

### 14.3 brush names users see

`brush:` messages, `help cat`, the `brush$ ` prompt, `$BRUSH_VERSION`, `BRUSH_PS_ALT`,
the `thraa/cash` URL, the `experimental-bundled-coreutils` feature name. ARCH-02,
ARCH-09, ARCH-10.

### 14.4 **Yours:** default `HISTSIZE` and `HISTFILESIZE`

Seen while fixing 8.5. Bash sets both to 500 when they are unset; cash leaves them unset,
so a user who sets neither keeps every line, in memory and in `~/.cash_history`. Following
Bash would cut an existing long history file to 500 entries at the next start. Follow
Bash, or keep "no limit" and add a row to spec §4.

### 14.5 **Yours:** `\v` and `\V` in a prompt

They give cash's version (`1.3`, `1.3.11`) while `$BASH_VERSION` says `5.3.15(1)-release`;
in Bash they are Bash's version. `\s` gives `cash`. Keep cash's, and add a row to spec
§4, or follow `$BASH_VERSION`. Seen in 12.3.

### 14.6 Dead code, allows and dependencies

- `spawn::spawn`, `build_cmd_command_line`, the winnow stub, the `sys` stubs behind
  `#![allow(unused)]`, `#![allow(dead_code)]` in cash-shell, the harness's oracle mode,
  stale `TODO(bundled)` comments; `#[expect]` over `#[allow]`. ARCH-11, EXE-12, W32-11,
  PI-13, BIN-16, BIN-17.
- cash-sed's unused `predicates`, `textwrap`, `phf`; brush's dev-deps; external versions
  into `[workspace.dependencies]`; `check_elevation` and `whoami.exe` scraping replaced
  by token calls in cash-win32. ARCH-07, ARCH-08, ARCH-17.
- LICENSE symlinks to a missing `crates/LICENSE`; NOTICE without posixutils-rs and uutils
  sed. ARCH-15.
- awk, bc and sed allow the lints for code that can panic (`unwrap_used`, `expect_used`,
  `panic`, `panic_in_result_fn`, `unwrap_in_result`, `string_slice`,
  `missing_panics_doc`): 58 `unwrap` and 34 `expect` in awk, 37 `expect` and 8 `panic!`
  in bc, 41 `panic!` and 24 `unwrap` in sed. Each is an input that can crash the tool
  or an invariant to write down. Left from 10.3, ARCH-01.
- Left from phase 11: `cmd /c type <(echo x)` sometimes prints `x` and then "The pipe has been ended" (the
  review saw 11 in 80 on a busy machine; on 2026-10-03, 1 in 450 on an idle one, with
  1.3.10 and with the replay fix of phase 11 alike). The race the review suspected, between a pump's
  look at the replay and its look at the end, was real and is closed, but it is not
  this. Unexplained: `type` may take the pipe's end (ERROR_BROKEN_PIPE) for an error
  depending on when the server closes. W32-02.
- The Low findings not listed in phases 8 to 14 are in the report, §5.

---

## Decided, written down so it is not decided twice

- **Tab globs a typed `*` and `?`, and completes everything else as typed** (the user,
  2026-10-03): no Windows name can hold `*` or `?`, so `**/nee` keeps completing, and
  `[draft]` now does. Spec §4 row 40, LANG-16. Completing all of it literally, as Bash
  does, and leaving every glob character a glob, were turned down.
- **The D31 case-insensitive fallback keeps reaching every variable** (the user,
  2026-10-03): an unset `$seconds` still reads `SECONDS`; only its speed and the order
  among `Foo`/`FOO` changed (LANG-15). Limiting it to exported or imported names was
  turned down.

- **A writer whose reader went away ends with 141 in silence, if it is cash's own** (the
  user, 2026-10-03): builtins and bundled tools, as SIGPIPE ends Bash's; a program on
  `PATH` ends as it chooses. D71.

- **`$!` for a background job that starts no program is a number of cash's own** (the
  user, 2026-10-03): 4n + 1, which no Windows process has; `kill`, `wait` and `jobs -p`
  take it to mean the job. And **at most 256 subshells at once**, as Git Bash's
  `ulimit -u` says, documented with `CASH_MAX_SUBSHELLS`. Both are D70.

- **The code review's findings are worked as phases 8 to 14** (the user, 2026-10-02):
  crashes and security first, with CI hardening pulled into phase 8 and the `it` test
  helper into the start of phase 9.
- **`chmod +x` is silent and returns 0, as D23 says** (the user, 2026-10-02). The
  execute bits of a numeric mode are silent too; `chmod -x` keeps its D34 warning.
  Keeping the warning and adding the execute ACE were turned down.
- **D36 is measured before it is built or dropped** (the user, 2026-10-02): the pool is
  built only if it saves a noticeable amount (about 5 ms a prompt); otherwise D36 and
  D6's table are amended, with the measurement.
- **`time` and `times` report real CPU time** (the user, 2026-10-02). Documenting zeros
  as a divergence was turned down.
- **The first Tab inserts the shared part, the second opens the grid on the hinted
  candidate** (the user, 2026-10-02; spec D40, reedline patches 6 and 7). One Tab used
  to do both, against what D40 said. Bash, which beeps on the second Tab and lists on
  the third, and keeping one Tab were turned down.
- **`scoop update cash` from within cash: the user's Scoop ignores running processes**
  (the user, 2026-10-02, on their machine; no code change). Tried before deciding:
  with `ignore_running_processes` set, the update goes through while cash runs. The
  running ones keep their version, and `current`, `cash.shim`, the tool links (a
  running one too) and the Terminal fragment move to the new one. Only
  `shims\cash.exe` stays as it was while a cash started through the shim runs. Scoop
  prints two errors about it, but the file is Scoop's generic shim and keeps working.
  Scoop has no switch per manifest or per command for this check. Other users get the
  manifest's note. A `cash --update`, and an update run after the last cash exits,
  were turned down.
- **Timing-sensitive tests run on an idle machine, and their failures under load are
  never fixed** (the user, 2026-10-02). They are run with the CPU free and nothing else
  of this project beside them: no second build or test run, no other session's. One
  that fails under load is run again on an idle machine and judged from that; no
  timeout is lengthened and no test rewritten for it. `open-issues.md` entry 10 lists
  the ones seen.
- **`> /dev/stdout` shares the descriptor, it does not open the file again** (spec D7,
  §4 row 39). Git Bash reopens, so `{ echo a; echo b > /dev/stdout; } > out` leaves `b`
  there and `a`, `b` in cash. Both `/dev/stdin` sessions chose sharing, on their own.
- **Two sessions fixed `/dev/stdin` at the same time.** `zealous-goldstine`'s
  implementation is the one in `main` (it was built on the `/dev/tty` work). Of
  `kind-nash`'s, the tests, `open-issues.md` entry 9 and the spec row were kept and its
  implementation was not; its commits are in the history behind the merge.
