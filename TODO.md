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
  1.3.7 the shared part on the first Tab and the grid on the second (phase 7).
- An item says what was seen and what Bash does; a cause only where it was looked for.
  Longer notes on a thing that stays open belong in `open-issues.md`.

Collected on 2026-09-30 from every cash session of that day and the day before. All of
their work is in `main`; the sessions themselves are not needed any more. The order and
the grouping were chosen that day: the test suite first, so that every later phase can
trust its runs, then by what a user notices most.

Phases 1 to 7 are done. Phase 6 changed no code and had no release of its own.

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

## Phase 8. Crashes and security (R2, R4, R9)

### 8.1 Shallow recursion overflows the stack and kills the shell

`f(){ (( $1 > 0 )) && f $(( $1 - 1 )); }; f 200` ends with "has overflowed its stack";
inside `$(…)` at depth 45, in a pipeline stage at depth 20. Bash runs depth 500. Tokio's
workers keep their 2 MiB stack (`cash-shell/src/entry.rs:189`); the 500-deep guard
(`callstack.rs:126`) never fires first. Big worker stacks, a guard below the measured
depth, tests at depth 200 in all three places. BIN-01.

### 8.2 Inputs that panic or hang

- `echo {1..3..99999999999999999999}`: crash (`cash-parser/src/word.rs:961`); then drop
  the crate-wide `#![allow(clippy::unwrap_used)]` in `cash-parser/src/lib.rs`. PI-06.
- `declare -c c; c=éa`: crash (`variables.rs:524`). LANG-08.
- `PS1='\D{%Q} '`: crash (`prompt.rs:236`); `\!` and `\#` are "not yet implemented".
  LANG-07.
- A lone `/` in Terminal's settings.json: the JSONC tokenizer loops for ever
  (`cash-win32/src/terminal.rs:655-680`), reached by `ls --icons`. From reading. BIN-02.
- Then property tests (proptest, on the stable toolchain, so CI runs them) feeding
  random input to the tokenizer, the parser, `word::parse` and the JSONC tokenizer.
  PI-14.

### 8.3 `start` passes a URL to cmd unquoted

`start 'https://x/?a=1&b=2'` hands `&b=2` to `cmd.exe` as a second command; `%VAR%`
expands; a relative path resolves against the process folder
(`cash-builtins/src/win.rs:120-131`). Call `ShellExecuteExW` with the absolute path. From
reading. BI-06.

### 8.4 Two forms get past the subscript hardening of §4 row 37

`unset "a[$k]"` with `k='$((touch x) )'` or `k='${ touch x; }'` creates `x`; plain
`$(…)` is refused as promised. `runs_a_command` (`expansion.rs:781`) checks text; decide
on the parsed word instead. LANG-04.

### 8.5 History ignores HISTCONTROL and the rest

` export TOKEN=…`, typed with a leading space, is in `~/.cash_history` at once;
`HISTCONTROL`, `HISTIGNORE`, `HISTSIZE`, `HISTFILESIZE` are read nowhere, `set +o history`
does not stop recording, and the starter `.bashrc` sets `HISTCONTROL=ignoreboth`. Also:
each flush walks all of history; one entry is written in two or three writes
(`history.rs:209`). PI-03, LANG-05, PI-11.

### 8.6 CI and release hardening

`permissions: contents: read` with `write` only on the release job; SHA-pinned actions;
`cargo deny check advisories` (or rustsec audit); a `dependabot.yml` (rust-toolchain.toml
already cites one); `--locked` on the dist build; no rust-cache in the release job;
`checkout` of the tag on `workflow_dispatch`; `inputs.tag` through `env:`; pre-release
from a `-` in the tag; a nextest `ci` profile that shows flaky passes; the vendored
reedline and crossterm tests in `xtask ci full`. ARCH-03, ARCH-12, BIN-21, PI-09.

---

## Phase 9. The shell owns its state (R1)

### 9.1 One test helper module for `it`

First, so the items after it write their tests on it: an `it/common.rs` with the run
helper, the output struct and a unique scratch folder, isolating config.toml and
`BASH_ENV`. Today 60 of 62 modules declare their own `CASH`, 43 their own run helper, 36
their own output struct. Existing modules move to it as they are touched. BIN-18.

### 9.2 Programs run from the folder cash started in, not from the shell's

`cd sub && ./tool.exe` runs `tool.exe` from the folder cash was started in, or reports
"command not found" when there is none; Bash runs `sub/tool.exe`. `build_windows_command`
finds `candidate` against the shell's folder, then gives `Command::new` the relative name
(`crates/cash-core/src/commands.rs:420-425`, also `:1000`). Absolutize the command path
once in `SimpleCommand::execute`. EXE-01.

### 9.3 Builtins that start programs skip the shell's environment and folder

`xargs`, `find -exec`, `nohup`, `detach` and `start` spawn through
`std::process::Command` or `CreateProcessW` with the process's folder, environment and
PATH. `export FOO=bar; echo x | xargs cmd /c echo %FOO%` prints `%FOO%`; `xargs npm` and
`find -exec npm` do not find `npm.cmd`; `find -exec … > out` writes past the redirection;
`nohup sh -c …` is refused; `cd proj; detach code .` opens the wrong folder. One helper in
cash-core on `compose_std_command` and `exported_environment`, with the context's fds and
`jobreg::contain`, used by all of them. Same pattern: `install` operands and `find -newer`
are relative to the process folder. XC-1, BI-01, BI-02, BI-07, BI-08, BI-12.

### 9.4 PATHEXT, PATH and `umask` live in the process, not the shell

- `export PATHEXT=.EXE` still finds `hello.cmd`; `PATHEXT=…;.FOO` does not find `z.foo`.
  Three parsers disagree (`sys/windows/fs.rs:19`, `cash-win32/src/resolve.rs:58`,
  `commands.rs:290`); `pathindex.rs` and `msys.rs` also read the process. One
  `Shell::pathext()`. ARCH-04, XC-10.
- `PATH=a:$PATH; foo; PATH=b:$PATH; foo` runs `a/foo` twice; Bash clears the hash on a
  PATH assignment. EXE-05.
- `( umask 077 ); umask` prints `0077`; the mask is a process static
  (`cash-builtins/src/umask.rs:170`). XC-7.
- Then an `xtask check` that refuses `std::process::Command::new` and `std::env::var` in
  cash-core and cash-builtins outside an allow-list.

---

## Phase 10. awk, sed and bc (R3)

### 10.1 awk prints wrong numbers

`awk 'BEGIN{print 100000.4, 150000.2}'` prints `1 15`; gawk `100000 150000`.
`printf "%g", 100000` prints `1`; `print 0.00001234` prints `0.000012`; `print 2^64`
prints `9223372036854775807`; `printf "%x", -1` is fatal. `format.rs:169-235, 737-833`
and `value.rs:85`. Add the cases to `tests/awk-differential.sh`. TXT-01, TXT-02.

### 10.2 awk and sed bugs that break ordinary scripts

- awk: a `for (k in a)` left by `break` or `return` locks `a`, so the usual dedup function
  dies with "active iterator"; `for (k in a) delete a` panics. TXT-03.
- awk: `split(s, a, "\.")` is a parse error. TXT-05.
- awk: plain `getline` reads only the current file. TXT-06.
- awk: `awk '{print}' | head -1` panics; one `WriteFile` per record; no flush before
  `system()`. TXT-08, TXT-14.
- sed: `/x/,+1p` and `/x/,3p` miss the second range; `2,3c T` never prints `T`. TXT-04,
  TXT-07.
- The rest of TXT-09 to TXT-18 (sed `-z`, `\$` in the literal fast path, NUL and RS in
  awk, stale element references, `-i` atomicity, awk speed, sed locale, arrays passed to
  functions) in the report, §4.6.

### 10.3 awk, bc and sed are outside every lint

The three crates set `warnings = "allow"` and `clippy::all = "allow"`. Move them to
`[lints] workspace = true` with a local allow-list for style lints, keeping the unsafe
lints and rustc warnings on; add SAFETY comments to awk's VM; fix `>=` to `>` at
`cash-awk/src/interpreter/stack.rs:230`. ARCH-01.

---

## Phase 11. Pipelines, jobs and process substitution (R5, R6)

The riskiest phase: judge its runs on an idle machine (Decided, below).

### 11.1 An error in a pipeline stage ends the pipeline or the shell

`read -u 99 x | cat; echo after` prints only the error; Bash `after 0 1 0`.
`true | echo ${u:?boom}; echo after` exits cash. Contain each stage's error as the
parent-shell case already does (`interp.rs:1833`). EXE-02.

### 11.2 Background jobs of builtins

- They run on tokio workers and block them; with as many as cores, a foreground `$(…)`
  waits (with `tail -f`, for ever). EXE-03.
- `{ while ((1)); do x=1; done; } &` hangs at the `&`; `while :; do :; done & kill %1`
  cannot be killed. EXE-08.
- `cat <( { sleep 1; echo late; } & echo early )` loses `late`. EXE-06.
- The 128-slot `CASH_MAX_SUBSHELLS` also fails pipeline stages, undocumented. EXE-09.

### 11.3 Coprocesses deadlock

`coproc cat; …; exec {COPROC[1]}>&-; wait` hangs: the coproc holds its own stdin's write
end. `COPROC_PID` is the job number, `kill %1` fails, the fds are 3 and 4 instead of 63
and 60. EXE-04.

### 11.4 Process substitution pipes

- `<(…)` sometimes gives an empty read to a consumer that opens it twice (`cmd /c type
  <(echo x)` failed 11 in 80). W32-02.
- An unopened `<(…)` leaks two threads; the replay buffer keeps every byte. W32-03, W32-04.
- The pipes have the default DACL and accept remote clients (from reading). W32-07.

### 11.5 The resume after a suspended spawn is unchecked

`let _ = resume_process(…)` (`sys/tokio_process.rs:43`), through undocumented
`NtResumeProcess`: on failure the child stays suspended and the shell waits for ever.
Check it, terminate on failure, record the API choice beside D19. From reading. EXE-14.

---

## Phase 12. Language (R7)

### 12.1 Arrays

- `c=([2+1]=y z [i]=w)` puts everything on 0 and 1; Bash `[2] [3] [4]`. LANG-02.
- After `unset 'a[1]'`, `${a[-1]}` is the wrong element (counted from the element count).
  LANG-03.
- `declare -n r='a[1]'; r=Z` creates a variable named `a[1]`. LANG-12.

### 12.2 Here-documents

- `eval "$(declare -f g)"` and `export -f g; bash -c g` fail for a function with a
  here-doc (`ast.rs:1762`). PI-01.
- A `)` in a here-doc inside `$(…)` ends it early (`word.rs:1453`; the tokenizer's own
  scanner is right). PI-02.
- Backslash-newline is kept in an unquoted here-doc. PI-04.
- `echo $(( $((1)) << 2 ))` is taken for a here-doc. PI-05.

### 12.3 Smaller expansion differences

`${#x}` and `${x: -1}` count bytes (LANG-09); `$(( $empty ))` aborts (LANG-10);
`declare -i` through `read`/`printf -v`/arrays/`for` (LANG-11); `\$` in backquotes
(LANG-13); `${x/b/"$r"}` with `&` in `r` (LANG-14); history expansion inside `${!a[@]}`,
`$!`, `[!a]` (LANG-06); the D31 fallback scan is quadratic (LANG-15); nested `case` parses
in exponential time (PI-07). Low ones in the report §5.1.

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

### 13.3 `time` and `times` report CPU time

They show 0 user and sys. The shell's own from `GetProcessTimes`, children's from the
job objects' accounting (decided, below). EXE-11, XC-5.

### 13.4 Error output: colour, prefixes, `--help`

Errors are ANSI-coloured into pipes and files and ignore `NO_COLOR`
(`cash-shell/src/entry.rs:675`); three prefixes (`error:`, `cash:`, `name:`); `--help`
goes to stdout or stderr, exit 0 or 2, by builtin. One "colour this stream" helper. XC-8,
BIN-04, BI-16.

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

### 14.4 Dead code, allows and dependencies

- `spawn::spawn`, `build_cmd_command_line`, the winnow stub, the `sys` stubs behind
  `#![allow(unused)]`, `#![allow(dead_code)]` in cash-shell, the harness's oracle mode,
  stale `TODO(bundled)` comments; `#[expect]` over `#[allow]`. ARCH-11, EXE-12, W32-11,
  PI-13, BIN-16, BIN-17.
- cash-sed's unused `predicates`, `textwrap`, `phf`; brush's dev-deps; external versions
  into `[workspace.dependencies]`; `check_elevation` and `whoami.exe` scraping replaced
  by token calls in cash-win32. ARCH-07, ARCH-08, ARCH-17.
- LICENSE symlinks to a missing `crates/LICENSE`; NOTICE without posixutils-rs and uutils
  sed. ARCH-15.
- The Low findings not listed in phases 8 to 14 are in the report, §5.

---

## Decided, written down so it is not decided twice

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
