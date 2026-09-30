# To do

Everything found while working on cash that is not done yet, in one list.

- Work from the top, in one session, one item at a time: commit, bring `main` up to the
  commit, take the next. **No further worktrees and no spawned sessions**: what is found
  on the way is added here, not started beside it.
- An item says what was seen and what Bash does. A cause is given only where it was
  looked for. "Checked" means the behaviour was reproduced by the session that wrote the
  item down; re-run the script before trusting it, the code has moved since.
- A done item is deleted, and its commit says what it fixed. Longer notes on a thing that
  stays open belong in `open-issues.md`.

Collected on 2026-09-30 from every cash session of that day and the day before: `read`
at the console, `/dev/tty`, `/dev/stdin` and friends, piped standard input, the
load-sensitive tests, `kill` and reused pids, `jobs` and `wait`, `top`, `coolfetch`. All
of their work is in `main`; this file and `open-issues.md` hold what they left open, so
the sessions themselves are not needed any more.

---

## Waiting for your decision

- **Release.** `main` is pushed (2026-09-30), not released: the version in `Cargo.toml`
  is still 1.2.1, and the installed `cash` (Scoop's) shows none of this until a release.
  When, and as which version (1.3.0 was proposed), is open.

---

## 1. Ctrl-C does not end a script outside `read`

Checked on a ConPTY, `cash script.sh`, Ctrl-C handling enabled in the parent.

```bash
trap 'echo bye' EXIT
ping.exe -n 4 127.0.0.1 > /dev/null
echo "after-ping rc=$?"
i=0; while [ $i -lt 300000 ]; do i=$((i+1)); done
```

- During `ping.exe`: ping dies, and the script goes on with `after-ping rc=137`.
  `ChildProcess::wait_or_stop` (`crates/cash-core/src/processes.rs`) and
  `Job::wait_in_foreground` (`jobs.rs`) wait on `await_ctrl_c()` and do nothing with it.
- During the loop of builtins: nothing listens, Windows ends cash with `0xC000013A`, and
  neither the `EXIT` trap nor a trap on `INT` runs.
- Git Bash 5.3: the script ends at once in both cases, `bye` is printed, status is
  SIGINT's; with a trap on `INT` the trap runs and the script goes on.

To build on: `ErrorKind::Interrupted` ends a script with 130 and abandons the line at the
prompt (`Error::to_control_flow`, `Program::execute`), and `run_interrupt_trap` in
`read.rs` runs a trap on `INT`. D13 must stay true: a foreground program that handles
Ctrl-C itself and lives on (a REPL) must not take the script down.

## 2. Ctrl-C in `select` and `mapfile` at the console

Checked. Both still read the console through the standard library, as `read` did.

```bash
trap 'echo bye' EXIT
select x in one two; do echo "chose $x"; break; done; echo "after-select rc=$?"
```

- Ctrl-C enabled: Windows ends cash (`0xC000013A`), no `EXIT` trap.
- Ctrl-C ignored (a test runner's children): nothing happens; after Enter the script goes
  on. `mapfile -t lines` behaves the same way.
- Git Bash: the script ends at once and `bye` is printed.

`select` reads its answer in `crates/cash-core/src/interp.rs`; `mapfile` looks for 0x03
in bytes a console collecting lines never hands over. `ConsoleInput` in `read.rs` does the
job for `read` but lives in cash-builtins, and `select` is in cash-core.

## 3. A running `>(...)` is lost when cash exits, and can leave a process behind

Reported by the session that stabilised `write_process_substitution_works`.

- `cash -c 'echo x > >(cat)'` with stdout a file printed nothing in 9 of 12 runs;
  `cash -c 'echo x > >(sleep 1; cat)'` never prints `x`. The substitution is a thread of
  the shell and ends with it; `wait` does not cover it (`$!` is empty). In Bash it is a
  process that outlives the shell, so the output still arrives.
- About 1 run in 15 left a `cash.exe --invoke-bundled cat` whose only thread was
  suspended. It never ends and locks `target\debug\cash.exe` ("Access is denied" on the
  next build). Likely the shell exits between creating the child suspended and resuming
  it; not proven.
- **Decided (2026-09-30): cash waits for its running substitutions before it exits**, so
  their output always arrives, as it does in Bash. One that never ends keeps cash from
  exiting. `wait $!` for a substitution is not asked for. Fix the leftover suspended
  process with it, and bring `spec.md` D17 up to date: it still describes the old
  temp-file model.

## 4. `tee >(cmd)` fails

Reported; present before the `/dev` fixes. `open-issues.md` entry 9.

```bash
echo x | tee >(cat >&2)
```

cash: `\\.\pipe\cash-procsub-…: The parameter is incorrect.`, status 1. Bash: `x` on both
streams.

## 5. The `/dev` names as a command's argument

Reported by both `/dev/stdin` sessions. `open-issues.md` entry 9, spec D7 and §4 row 39.

- `cat /dev/stdin`, `cat /dev/fd/0`, `tee /dev/stderr`, `cat /dev/null > f`,
  `cp /dev/null f`: "The system cannot find the path specified.", status 1.
- The names are known only where cash opens the file itself (a redirection, `source`).
  An argument reaches a command as written (D4); translating arguments for tools whose
  grammar cash does not know is what D28 was revoked for.
- **Decided (2026-09-30): the tools cash bundles accept the names themselves.** `cat`,
  `tee`, `cp` and the others open `/dev/stdin`, `/dev/stdout`, `/dev/stderr`, `/dev/null`
  and `/dev/fd/N` where they open a file, so cash translates no argument. A program on
  `PATH` still gets the name as written and fails. This is a new decision for the spec.

## 6. File tests on the `/dev` names are false

Reported by three sessions. `open-issues.md` entry 9.

`[ -e /dev/null ]`, `[ -c /dev/null ]`, `[ -e /dev/tty ]`, `[ -c /dev/tty ]`,
`[ -e /dev/stdin ]`, `[ -p /dev/stdin ]`, `[ -w /dev/stdout ]`, `[ -e /dev/fd/0 ]` and
`[[ -e /dev/stdin ]]` are all false where Git Bash says true. The tests ask the
filesystem; each predicate needs its own answer for each name.

## 7. `cat <&1` reads standard input instead of failing

Reported; present before the `/dev` fixes. With `x` piped in and output a pipe,
`cat <&1` and `cat < /dev/stdout` print `x`, status 0. Bash:
`cat: -: Bad file descriptor`, status 1.

## 8. `cat 3< f <&3` is refused

Reported; present before the `/dev` fixes. cash:
`operation not supported on this platform: fd redirections`, status 1. Bash prints the
contents of `f`.

## 9. `$(jobs)` and `jobs | …` show a finished job as still running

Reported by the session that fixed `jobs` losing a job's status; `open-issues.md`
entry 8 has the scripts and both outputs.

```bash
"$X" -c 'exit 3' & sleep 0.5; while [ -n "$(jobs -pr)" ]; do sleep 0.1; done
```

Bash ends at once; cash never ends. A subshell gets a copy of the job table
(`Shell::clone`, `JobManager::snapshot`) taken without polling the jobs. `jobs -pr > file`
in the shell itself is right. Entry 8 lists eight smaller `jobs`/`wait` differences
beside it (what a plain `wait` forgets, a finished job's number, `wait %5`'s message and
status, `wait -n -p VAR`, `Done(3)` in POSIX mode).

**Decided (2026-09-30): `wait %N` after `jobs` has shown the job matches Bash**,
`wait: %1: no such job` and 127, where cash returns the job's status now. A one-line
change in `collect_saved_job` and its test; `wait $pid` keeps returning the status.

## 10. `fuser` and `lsof` are slow on a system DLL

Reported, with measurements. `fuser -v kernel32.dll` took 2.5 to 6.5 s alone and 41.8 s
beside another build. The Restart Manager refuses a system DLL at once, and
`file_holders` then walks every process: about 15,500 modules in 290 processes to list
(0.9 s idle, up to 14 s under load), and `path_key` opens each module's file to
canonicalize it (1.3 to 4.9 s). `crates/cash-builtins/src/fileuse.rs`.

## 11. Tests that fail when the machine is busy

Each failed once in some session's full run (`--retries 0`) while other sessions were
building, and passed when run alone. Several sessions at once was most of the cause, and
that stops with this file; what is left is worth a look only if one fails on a quiet
machine.

- `pty-oracle` `cash_leaves_the_screen_bash_leaves` (different cases each time)
- `conpty_ctrl_z_stops_a_foreground_program_and_fg_resumes_it`,
  `conpty_ctrl_z_stays_end_of_input_for_a_program_reading_the_keyboard`,
  `conpty_interactive_variables_and_arithmetic`
- `read_console::a_script_with_both_streams_redirected_asks_the_terminal_by_name`: the
  answer is typed 300 ms after the query is shown, and is swallowed by the console if the
  inner cash has not begun its read by then
- `pipeline_concurrency::a_while_loop_does_not_lose_output` (timed out)
- `bash_gaps::wait_preserves_status_and_waits_for_next_completion`,
  `bash_gaps::a_chld_trap_runs_once_for_each_child_reaped`
- `job_groups::at_the_prompt_a_background_job_survives_ctrl_c`
- `git_prompt::git_ps1_shows_the_branch_its_state_and_the_upstream` (timed out)
- four `cash-sed` tests, `completion_scripts::docker_completion_works`
- `fuser_lsof::fuser_marks_an_executable_and_a_loaded_module`: given three periods of
  30 s in `.config/nextest.toml`; item 10 is its cause

## 12. `cargo doc -p cash-win32` fails

Reported: four broken doc links in `crates/cash-win32/src/children.rs` and `ctrl_z.rs`.

## 13. `stdout_capture` sometimes fails while nextest lists tests, and no test runs

Seen twice on 2026-09-30, and it passed on the rerun both times. nextest asks each test
binary for its tests with `--list`; `stdout_capture` of cash-win32 has a harness of its
own, runs its cases instead, and `the_handle_is_restored_even_if_the_body_panics` fails
at `crates/cash-win32/tests/stdout_capture.rs:233` with "the panic was swallowed". The
whole run then ends with exit code 104 before any test has started. Not looked into.

## 14. Small differences left in `read` at a console

None of these is known to bother anyone yet.

- Tab in a line is shown as `^I`, where a terminal shows blanks up to the tab stop
  (erasing it would need the column it began in).
- Keys typed past the count of `-n` are shown when something reads them, not when typed.
- A line not ended when `-t` runs out is dropped; a terminal keeps it for the next read.
- An answer to a terminal query that arrives before `read` has begun can be swallowed by
  an older console host (Windows Terminal passes it through).
- The console's own line editing (arrow keys, Escape, function keys) is gone from a plain
  `read`, the price of Ctrl-C working there; `read -e` has an editor.

## 15. Small differences left in the `/dev` names

- `/DEV/STDIN` works in Git Bash and is a path in cash; `/dev/null/` is refused by Bash
  and is still the device in cash.
- `read x < 'CONIN$'` fails; only the `/dev/tty` name is mapped to the console.
- `exec 3<>/dev/tty` can be read from but not written to: a console has no one handle
  that is both (spec D7).

## 16. Small things left in `kill` and held processes

From the session that guarded `kill` against reused pids (spec D22).

- Holding a job stopped with Ctrl-Z open has no test of its own: one line calling the
  tested `hold`, and a test needs the ConPTY harness.
- Still asked by number, and so open to a reused pid: a pid cash never started as a job's
  (a foreground command, one read from `ps`), and a job's process pushed out of the held
  set after 1024 later ones ended.

---

## Decided, written down so it is not decided twice

- **`> /dev/stdout` shares the descriptor, it does not open the file again** (spec D7,
  §4 row 39). Git Bash reopens, so `{ echo a; echo b > /dev/stdout; } > out` leaves `b`
  there and `a`, `b` in cash. Both `/dev/stdin` sessions chose sharing, on their own.
- **Two sessions fixed `/dev/stdin` at the same time.** `zealous-goldstine`'s
  implementation is the one in `main` (it was built on the `/dev/tty` work). Of
  `kind-nash`'s, the tests, `open-issues.md` entry 9 and the spec row were kept and its
  implementation was not; its commits are in the history behind the merge.
