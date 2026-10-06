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
  `--file=<(…)`), fixed while CI was put right, 1.3.12 the language (phase 12), 1.3.13
  builtins and error output (phase 13).
- An item says what was seen and what Bash does; a cause only where it was looked for.
  Longer notes on a thing that stays open belong in `open-issues.md`.

Collected on 2026-09-30 from every cash session of that day and the day before. All of
their work is in `main`; the sessions themselves are not needed any more. The order and
the grouping were chosen that day: the test suite first, so that every later phase can
trust its runs, then by what a user notices most.

Phases 1 to 13 are done. Phase 6 changed no code and had no release of its own.

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
| 15 | 1.3.15 | GNU parity of the bundled sed and awk |
| 16 | 1.3.17 | sudo, su and `cash help` |
| 17 | 1.3.18 | `lsof` lists everything |
| 18 | 1.3.18 | `ss` as iproute2 7.2 has it |
| 19 | 1.3.19 | `ss -i` and `dev`, and the vi fix |
| 20 | 1.4.0 | `croot`: a built-in file and folder picker on Alt-E |

---

## Phase 16. sudo, su and `cash help`

Found writing the help catalogue (2026-10-04); all fixed, released as 1.3.17.

---

## Phase 17. `lsof` lists everything

Chosen by the user on 2026-10-04, after 1.3.17:

Bare `lsof` and the open files of `-p`/`-c`/`-u` come from the handle walk
(`cash_win32::handles`, spec D50) since 2026-10-05, and so do `+D`, `+d` and a folder
named: `lsof +D /tmp` over `TEMP`'s 87,411 folders took over two minutes asking the
Restart Manager file by file, and takes under two seconds. Done.

---

## Phase 18. `ss` as iproute2 7.2 has it

Compared with iproute2 7.2.0's `ss` in WSL and its `misc/ss.c` on 2026-10-05; done the
same day. The default view, `state`/`exclude`, the state names, the filter grammar's
errors, peerless sockets, `-s` with a selection, IPv6 scope names, getopt_long parsing,
`-A !table`, the Linux-only long options, row order, `-V` and `-F` follow iproute2; `-r`,
`-K` and `-B` were added, as the user chose. As in iproute2, whose `SS_CONN` keeps the
bound-inactive bit, plain `ss -t` lists bound sockets as `UNCONN`.

Left as differences (spec D51, row 29): a dual-mode socket is two lines, not one `*`;
`-K` closes IPv4 connections only and only elevated; `-K`'s elevated path has not run
on the author's machine (the test runs it when elevated).

Not chosen then: `dev NAME` and `-i` (both taken up in phase 19), `-m` (`GetPerTcpConnectionEStats`
needs an administrator to switch collection on per connection), `-E` (polling only).

---

## Phase 19. `ss -i` and `dev`, and the vi fix

Asked for by the user on 2026-10-05; released as 1.3.19 with the vi-mode fix (reedline
patch 8):

- `ss -i`: per-connection TCP statistics from `GetPerTcpConnectionEStats`.
- `ss ... dev NAME`: matches the IPv6 scope's interface; a socket without one has no
  device, as a Linux socket without `SO_BINDTODEVICE`.

---

## Phase 20. `croot`: a built-in file and folder picker on Alt-E

The user used broot on Alt-E for two weeks (set up 2026-09-27 in `~/.bashrc` and broot's
`cash-cd.hjson`) and wants it built in, as a picker only: no search language, no
editor integration, no verbs. Broot takes up to two seconds to open, being a separate
program; built in, the first frame comes from one folder read. Chosen on 2026-10-05:

- **Inline, below the prompt** (as fzf's `--height`): 40% of the window, at least 8
  rows, `CASH_PICKER_HEIGHT=50%` or `=15`; the scrollback above stays visible, the
  screen scrolls up when the prompt is near the bottom, a tiny window gets full screen.
- **Broot's tree**: several levels open, trimmed to fit with `… N more`; Right makes the
  selected folder the root, Left goes to the parent; at a drive's root, Left shows the
  drives (C:, D:, mapped drives, `~`).
- **Typing is a fuzzy filter that also searches deeper**, in the background, best
  matches first, capped in time and entries; Esc clears it, then closes.
- **Context from the command line**:
  - folders only for an empty line, `cd`, `pushd`, `rmdir`, `mkdir -p`; folders and
    files for any other command; Alt-F switches;
  - the word at the cursor, when it is part of a path, sets the start (`cd ~/src/fo`
    starts in `~/src` filtered by `fo`) and is replaced by the pick;
  - an empty line, `cd` or `pushd`: the pick goes in and runs at once; any other
    command: the pick is inserted with a space after it and nothing runs;
  - a command that takes several paths (`cp`, `mv`, `diff`, …) keeps the picker open
    for the next argument; one known to take one closes it; unknown commands stay open.
    Esc, or picking with Ctrl-Enter, closes.
- **Path form**: relative below the current folder, `~/…` under home, else `C:/…`;
  quoted only when needed; folders end in `/`.
- **Extras in v1**: drives above `C:/`; a folder-history view (Alt-H) from cash's own
  folder history (`cdh`); hidden entries (dotfiles, the hidden attribute) and
  .gitignored ones left out by default, Alt-. and Alt-I show them.
- **Colours**: entries as `ls` colours them (`LS_COLORS`); the picker's parts from
  `CASH_PICKER_COLORS` (`sel=…:match=…:frame=…`).
- **Keys**: Alt-E, as a Readline function (`cash-picker`) that `bind` can move; it
  replaces the broot binding in `~/.bashrc`. No mouse in v1.
- **Also a builtin, `croot [-f|-d] [DIR]`**: the same picker, printing the pick(s) on
  standard output for scripts (`cd "$(croot)"`, `vim $(croot -f)`).

Spec D73, approved by the user on 2026-10-05, is the full description. Fuzzy
matching and .gitignore from established crates (e.g. `nucleo-matcher`, `ignore`),
not hand-written.

Released in 1.4.0 (2026-10-05): the `cash-picker` crate (line context, picks, tree, filter,
deep search, colours, the picker's keys and frames, the terminal runner), Alt-E in the
line editor (`cash-picker`), the `croot` builtin, `help croot`; ConPTY tests for a `cd`
pick and an inserted file. The broot block is out of the user's `~/.bashrc`, at their
request (backup `~/.bashrc.before-croot`).

After the user's first use: 1.4.1 lets Down and PgDn scroll through a long folder,
1.4.2 shows each pick on the command line while picking, 1.4.3 stops the flicker while a
search runs. Then (2026-10-06): the first frame measured at a median of 17-19 ms
(`tests/croot_latency.rs`, spec D73), a resize works the height out again, and Alt-S
sorts newest first with each entry's age. Nothing left open.

---

## Found along the way

- A test in the full suite leaves `x.lnk` in the repository root: a 0-byte named pipe as
  MSYS2 makes them (its `mkfifo` writes a FIFO as a special `.lnk` file), timestamped
  during the 1.4.3 gate on 2026-10-06. No test names `x` with `mkfifo`; one of them runs
  with the repository as its folder. Find it and give it a temporary folder.

---

## Decided, written down so it is not decided twice

- **What cash prints never cites its own documents** (the user, 2026-10-05): no
  decision numbers, §4 rows, `spec.md`, ROADMAP items or research files, in help pages,
  `--help`, version lines, warnings or `cash doctor`. `ss --help` ended with "cash's ss
  is the Windows subset described in ROADMAP item 9"; help pages had 122 such
  references. Point to a `help` page instead.

- **An elevated `ss -i` switches statistics collection on** (the user, 2026-10-05):
  Windows keeps per-connection TCP statistics (rtt, rto, cwnd, bytes, retransmits,
  queues) only for connections an administrator switched collection on for; unelevated,
  only the MSS is readable and the rest reads as zeros or garbage. So unelevated `-i`
  prints the MSS with a note pointing to `sudo ss -i`, and elevated `-i` switches
  collection on for the listed connections (on until each closes) and prints the rest,
  counted from that moment.
- **Files an elevated `sudo` makes are yours, and an approver who is not you is named**
  (the user, 2026-10-04): when you approve with your own account, the elevated side sets
  your account as the default owner of what it creates (Unix leaves root owning them, and
  Windows `Administrators`); when another account approves (a standard user), cash says
  `running as ADMIN, not you`. Leaving Windows' default, and a system-wide policy, were
  turned down.

- **`elevate` stays, beside `sudo` and `su`** (the user, 2026-10-04): it starts a
  program elevated in a new window without waiting and needs no tool; `sudo` and `su`
  fall back to its UAC request when neither gsudo nor Windows' sudo is there. Removing
  it, or keeping `sudo` refusing without a tool, was turned down.

- **Bundled tools stay processes of their own** (the user, 2026-10-04): a call to
  `cut`, `sort`, `sed` or `awk` costs a process start, about 25 to 35 ms in a debug
  build against 0.5 ms for pure shell, and a tool that crashes or exits cannot take the
  shell with it. Running them inside the shell's process was turned down. A bundled
  `grep` likewise: Git for Windows' GNU grep is always there (D35), costs the same per
  call, and handles CRLF.

- **Uptime is Windows' figure** (the user, 2026-10-04): `coolfetch`, `uptime` and `top`
  count from the last full kernel boot (`GetTickCount64`), as Task Manager and fastfetch
  do, so with Fast Startup a power-on resumes the count rather than restarting it.
  Counting from the last power-on (the Kernel-Boot event), or showing both, was turned
  down.

- **sed's errors read as GNU sed's, prefix and all** (the user, 2026-10-04):
  `sed: -e expression #1, char 5: unknown command: `x'`. Keeping cash's
  `<script argument 1>:1:5: error:` before GNU's words was turned down.
- **1.3.14 goes out with phase 14's list done; the GNU gaps found on the way are phase
  15** (the user, 2026-10-04). Another round before the release was turned down: each
  round of comparing had found more.

- **awk's `system()` and pipes and sed's `e` run plain cash** (the user, 2026-10-04):
  `cash -c`, `$0` `cash`, Bash's behaviour. Running them as `sh`, in POSIX mode, as
  gawk and GNU sed run `/bin/sh`, was turned down.

- **`exec -a NAME` is refused for a program other than cash** (the user, 2026-10-04): std's `Command` cannot
  set a program's `argv[0]`, and running it under its own name in silence was the
  alternative to an error. A spawn of cash's own to set it, and leaving it ignored, were
  turned down. `exec -l`, which only puts `-` before `argv[0]`, goes with it.
- **Times honour `TZ`, as in Bash** (the user, 2026-10-04): `printf '%(…)T'`, prompts and
  `HISTTIMEFORMAT` format in the zone `TZ` names (`UTC`, a POSIX `EST5EDT`, an IANA name),
  else Windows' own. Noting that cash ignores `TZ` was turned down.
- **W32-02 gets one timed investigation** (the user, 2026-10-04): a reproduction rate,
  then the server waiting for its reader before it closes, kept only if the rate drops;
  else it is recorded as a `cmd` quirk and closed.

- **A cash started as `sh` runs in POSIX mode, as Bash does** (the user, 2026-10-04):
  `sh -c`, `exec sh`, `#!/bin/sh` and `#!/usr/bin/env sh` turn on `set -o posix`, every
  builtin kept. Keeping D7's "no modes", with `sh` plain cash, was turned down. Spec D7.
- **A program in a folder too long for Windows starts in its 8.3 short name** (the user,
  2026-10-04), and fails saying why where there is none that fits. A clear error alone,
  as Git Bash gives for native programs, was turned down. Spec D29, §4 row 45.

- **The MSRV is the pinned toolchain's minor, 1.98** (the user, 2026-10-04): it is the
  only Rust CI builds with, so it is the only one promised; it rises with
  `rust-toolchain.toml`. A 1.95 floor with a CI job of its own, and no `rust-version`
  at all, were turned down.

- **The git-prompt goldens are frozen against the installed Git for Windows** (the user,
  2026-10-04): `tests/git-prompt/SOURCES` holds the hashes of the scripts the output came
  from, and the test fails and says to re-freeze when they change, a runner's Git update
  included; without Git for Windows it fails. Vendoring the GPL-2.0 `git-prompt.sh`, and
  keeping the differential a by-hand script, were turned down.

- **History is not cut while `HISTSIZE` and `HISTFILESIZE` are unset** (the user,
  2026-10-03): every line is kept, where Bash defaults both to 500. Spec §4 row 43.
  Following Bash, which would cut an existing long `~/.cash_history`, was turned down.
- **`\v` and `\V` in a prompt give cash's version** (the user, 2026-10-03), as `\s`
  gives `cash`. Spec §4 row 44. Following `$BASH_VERSION` was turned down.
- **Errors are worded as Bash words them** (the user, 2026-10-03): `script.sh: line 3:
  foo: command not found` in a script, `cash: line 1: …` for `-c`, `cash: foo: command
  not found` interactively, with Bash's message text (`r: readonly variable`,
  `x: unbound variable`), builtins' messages included. Keeping cash's words with Bash's
  location, and `error:` with a location added, were turned down.
- **`fg` and `bg` need job control, as in Bash** (the user, 2026-10-03): in a script
  they say `fg: no job control` and return 1, until `set -m`. Keeping cash's leniency,
  where `fg` in a script waited for the job, was turned down.
- **Errors are coloured only on a terminal** (the user, 2026-10-03): the prefix is red
  when stderr is a terminal and `NO_COLOR` is unset; a pipe or a file gets plain text,
  through one helper for every stream. No colour at all was turned down.
- **A builtin's bad option is Bash's two lines** (the user, 2026-10-03): `name: -q:
  invalid option` and `name: usage: <synopsis>`, status 2. Keeping clap's block was
  turned down. `name --help` goes to stdout with status 2, as Bash's does (the user,
  2026-10-03, correcting the status 0 first written here); `help name` gives 0.

- **`chmod` is silent about group and other bits** (the user, 2026-10-03): Windows has
  none per file, so `chmod go-w ~/.ssh` and `chmod o+r f` change nothing and say nothing,
  as a numeric mode's other bits do. Spec §4 row 18. Warning on grants only, or on all of
  them, were turned down.

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
  D6's table are amended, with the measurement. Measured 2026-10-03: the job setup is
  0.18 ms a spawn against 128 ms for a Starship prompt (`prompt_job_cost` example), so
  the pool was dropped and D36 and D6's table amended.
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
