# Bash 5.3 Audit and Classification

This document reviews the complete upstream Bash 5.3 release (`NEWS` and git reference commit `9c465866`)
and classifies every change relative to Cash's Win32 execution model and roadmap.

In accordance with [ROADMAP.md](../../ROADMAP.md), Cash does not claim `BASH_VERSION=5.3` until
each applicable item is audited, implemented, or documented as a deliberate divergence.

## Evidence

Every portable and POSIX-mode item below that can be observed from a script has a probe in
[`probe.ps1`](probe.ps1) (`-Suite 53`), run against the Git for Windows Bash 5.3.15 oracle
recorded in [`provenance.json`](provenance.json). Results are in `results-53.json`.

```powershell
cargo build -p cash
./research/bash-reference/probe.ps1 -Suite 53
```

**Status (2026-09-27): complete, and cash reports `BASH_VERSION=5.3.15`.** 39 of 39 probes
match, and the ConPTY harness covers section 2. The first run of this suite matched 17: several
items earlier recorded here as "Complete" or "Verify" did not hold up against the oracle, and
the probes also exposed older Bash behavior Cash had wrong (listed under
[Found while auditing](#found-while-auditing)). Each fix has a regression in
`crates/cash/tests/bash_gaps.rs`.

A `kill -INT $$` probe is deliberately absent: on Windows the signal reaches the whole
console process group, including the harness running the probes. Trap delivery for real
signals needs its own isolated test (see below).

## Summary of Classifications

| Category | Description | Count |
| --- | --- | :---: |
| **1. Portable language & builtins** | Script-visible language syntax, variable, and builtin semantics | 21 |
| **2. Interactive, Readline & jobs** | Terminal line editing, interactive completion, and job tracking | 14 |
| **3. POSIX-mode changes** | Conformance items active under `--posix` | 7 |
| **4. Build & internal architecture** | Internal memory, loadable C-ABI modules, and build scripts | 5 |
| **5. Deliberate Win32 divergences** | Features incompatible with Windows process/filesystem models | 3 |

---

## 1. Portable Language and Builtin Behavior

| Item | Feature | Upstream Description | Cash Status |
| :--- | :--- | :--- | :--- |
| **1.a** | Binary script check | Check the first two lines for NUL when the first starts with `#!` | **Fixed.** `cash script` refuses an ELF file, or a NUL in the first line (two with `#!`) of the first 80 bytes, with status 126; `source` does not check. NUL bytes elsewhere are discarded like Bash's input reader. A missing script now exits 127. |
| **1.c** | Syntax error lines | Report the starting line of an unterminated compound command | **Fixed.** `unexpected end of file from `if' command on line 3`, for `if`, `case`, `while`, `until`, `for`, `select` and `{`. Bash also runs the lines before the error; Cash parses the whole file first (existing behavior, not a 5.3 item). |
| **1.g** | Regex compile errors | Print an error if a `[[ =~ ]]` regex fails to compile | **Fixed.** Was a log `WARN` and status 1; now a diagnostic on stderr and status 2, and the script continues. |
| **1.i** | `type -a -P` | Report both the hashed path and the `$PATH` result | **Verified** by probe. |
| **1.j** | `trap -P` | Print the trap action for each signal argument | **Verified** by probe, including both error cases. |
| **1.k** | `command declare` | `command` before a declaration builtin keeps assignment parsing | **Fixed, POSIX mode only.** The oracle still field-splits outside POSIX mode, so Cash does too. Fixing this exposed that `declare` and `export` rejected any assignment arriving as an ordinary word (`declare "a=x y"`, `export "$spec"`); both now accept it. |
| **1.l** | `printf %#q` | Alternate form of `%q`/`%Q` forces single quoting | **Fixed.** `%#q` had used "quote if needed". Strings with control characters still use `$'...'`, as in Bash. |
| **1.p** | Empty `$PATH` | Treat a null `$PATH` as `.` | **Fixed.** Empty and relative entries were resolved against the process directory rather than the shell's `cd` directory; they now use the latter, and such hits are not hashed. |
| **1.q** | `GLOBSORT` | Sort pathname expansion by name, size, blocks, mtime, atime, ctime, numeric, nosort | **Implemented** from `pathexp.c`: `+`/`-` direction, name tiebreak, invalid values keep name order. Windows has no inode change time or block count, so `ctime` uses creation time and `blocks` counts 512-byte units of the size. (NEWS says `none`; the source's keyword is `nosort`.) |
| **1.r** | `compgen -V` | Store completions in an array | **Complete** (earlier regression). |
| **1.s** | `${ cmd; }`, `${| cmd; }` | Current-shell command substitution | **Complete** (earlier regressions). |
| **1.t** | `array_expand_once` | Replaces `assoc_expand_once` | **Fixed.** The two names are now one option. With it set, `unset`, `read` and `printf -v` do not expand a subscript again, associative or indexed (`unset 'a[$i]'` becomes an arithmetic error); `declare` and `[[ -v ]]` still do, as in Bash. Without it, cash had not expanded indexed subscripts at all (`unset 'a[$i]'` failed); it now does. A command substitution in that second expansion is refused rather than run (spec divergence 37). |
| **1.v** | `$TIMEFORMAT` | Up to six digits of precision | **Fixed.** Cash ignored `TIMEFORMAT` completely. It now follows `print_formatted_time`: `%[p][l]R/U/S`, `%P`, `%%`, precision capped at 6, empty value suppresses the report, invalid characters are diagnosed; `time -p` uses the fixed POSIX format. |
| **1.w** | `BASH_MONOSECONDS` | Monotonic clock | **Verified.** Whole seconds (as in Bash, not microseconds) from `GetTickCount64`. |
| **1.x** | `BASH_TRAPSIG` | Number of the running trap | **Fixed.** Was a dynamic variable that was empty for `ERR`/`DEBUG` and never unset. It is now bound while a handler runs and restored afterwards, with Bash's numbering: signals by number, `EXIT` 0, `DEBUG` 65, `ERR` 66, `RETURN` 67. For real signals it is checked by the ConPTY harness (`kill -INT $$` and so on), which also found that cash ended itself on such a signal, trap or no trap; it now runs the trap (spec D21). |
| **1.ee** | `test` with >4 args | Parenthesized subexpression heuristic | **Verified** by probe. |
| **1.ff** | `MULTIPLE_COPROCS` | Several coprocesses at once | **Verified** by probe. |
| **1.kk** | `source -p PATH` | Search path for `.`/`source` | **Complete** (earlier regression). |
| — | `BASH_SOURCE` from `-c` and stdin | A function defined in a `-c` string or read from standard input has `$0` as its `BASH_SOURCE` (5.2: `environment` and `main`); at the prompt it stays `main` | **Fixed.** Missed by the first pass of this audit; a compat test capped at 5.2 pointed to it, and it was probed against Git Bash 5.3.15 and Bash 5.2 and 5.3 in Docker. |
| **1.pp** | `bash_source_fullpath` | Full paths in `BASH_SOURCE` | **Implemented.** The option existed but did nothing; the real path is now recorded when a file is entered, as `push_source` does. |
| **1.ss** | EOF parse detail | More detail when EOF ends a command | Covered for compound commands by 1.c. |
| **1.uu** | `exit` in a trap | Uses the pre-trap `$?` at the trap's top level | **Verified** by five probes, including subshell and function forms. |

### Found while auditing

These are not 5.3 items. They were wrong in Cash, found by the probes above, and are fixed
with regressions:

- **`exit` in an `ERR` or `DEBUG` trap did nothing.** The handler's control flow was
  discarded, so `trap exit ERR; false; echo x` printed `x`.
- **The `RETURN` trap never ran.** It now fires for sourced files, for functions with
  `set -T`, and for functions with the trace attribute. `declare -ft` had been accepted and
  ignored.
- **Function trap scoping.** Cash hid the caller's `ERR` trap inside functions by checking
  "am I in a function", which also hid traps the function set itself. It now follows Bash:
  traps a function does not inherit are set aside on entry and restored on return unless the
  function replaced them.
- **`declare +ft name` was a parse error.** `+f`/`+F` are now accepted and, as in Bash, ignored.
- **`command -v` rendered a `PATH` hit with a backslash** (`C:/WINDOWS/system32\netstat.exe`),
  disagreeing with `type` (D3).

Found with the last three items (ROADMAP item 14):

- **The `RETURN` trap saw the returned status.** It now sees `$?` as `return` found it,
  for functions and sourced files; the caller still gets the returned status.
- **`wait` lost finished jobs' statuses.** A job reaped when the next one started, or
  waited for twice, reported 0 or 127; and a background `{ exit 3; } &` made `wait` and
  `fg` exit the waiting shell.
- **Arithmetic errors.** In a builtin (`unset 'a[1+]'`, `printf -v`, `read`, `test -v`)
  one now abandons the rest of the line, or of a `-c` string, as Bash's does; in `(( ))`
  it fails the command with status 1 and the line goes on, where cash had abandoned it.

Found later, in a script that called `jobs` before `wait` (2026-09-30):

- **`jobs` lost a finished job's status.** `jobs` took a finished job it had shown out of
  the table without saving its status, so `wait PID` answered 127; for one it had seen
  finish but not shown (`jobs -r`), `wait` answered 0. Both keep the status now. As in
  Bash, a job `jobs` has shown is no longer `wait -n`'s to return, except by pid or in
  POSIX mode; a plain `wait` forgets it; `jobs -p` reports nothing; and in POSIX mode
  `wait PID` forgets a status once it has returned it. What is left is in
  [open-issues.md](../../open-issues.md), item 8.

### Known remaining differences

- A command substitution in a subscript that a builtin expands a second time
  (`unset "a[$key]"` with `key='$(cmd)'`) is refused, not run: spec divergence 37.
- `/dev/tcp/host/port` and `/dev/udp/...` redirections are not implemented. They are a Bash
  redirection feature rather than an OS one, so a Winsock implementation is possible.

---

## 2. Interactive Completion, Readline, and Job Control

These need a terminal rather than `-c` probes. `crates/cash/tests/pty_oracle.rs` runs
each case's keystrokes in a ConPTY under cash and under Git Bash 5.3.15, and compares
the screens the two leave (ROADMAP item 15). Rows marked "by the harness" are cases
there.

| Item | Feature | Upstream Description | Cash Assessment |
| :--- | :--- | :--- | :--- |
| **1.b** | Completion quoting | Preserves user-supplied quotes around word completion | **Verified** by the harness: `ls "al<TAB>` gives `ls "alpha beta.txt"`, as Bash does. With no quote typed cash quotes in its own style (D40). |
| **1.f** | Signal handling | Bash signal handlers active during completion | **Not applicable.** Readline runs Bash's completion inside its signal handling; cash's completer runs as a task beside the line editor, and a Ctrl-C during it cancels the task (`completion.rs`). There is no handler state to keep active. |
| **1.o** | Window size check | Check winsize during traps, `bind -x`, completion | **Fixed**, by the harness: cash never set `COLUMNS` and `LINES` at all, though `checkwinsize` was on. It now sets them from the console before each prompt, so a trap, a `bind -x` command or a completion function sees the size at that prompt. Reedline itself redraws on a resize. |
| **1.u** | `compopt -o fullquote` | Force full quoting for completions | **Implemented**, by the harness: `complete` and `compopt` take `-o fullquote`, and a function's completions are then quoted as file names are, in D40's style (`'x y'` where Bash writes `x\ y`); `noquote` still wins. |
| **1.y** | `checkwinsize` | Enabled in subshells from interactive shells | **Fixed** with 1.o, by the harness: a subshell sees the `COLUMNS` and `LINES` its interactive shell set. |
| **1.aa** | `bind -x` syntax | Key sequence and command separated by whitespace | **Fixed**, by the harness: `"\C-t" "echo hi"` is read, the command dequoted, as are `"\C-t": "cmd"` and anything after the closing quote. The harness also found that no `bind -x` binding showed its output: the prompt was redrawn over it. That was Reedline's; cash carries a patch (`vendor/reedline/CASH-PATCHES.md`). |
| **1.bb** | `bind -x` output | Print bindings using new syntax | **Verified** by the harness; a command's `\` and `"` are now escaped, as in Bash. |
| **1.cc** | `read -E` | Use line editor with shell completion | **Verified** by the harness: `read -E` edits with completion; a completed name is quoted in cash's style (D40), where Bash backslash-escapes. |
| **1.dd** | `bash-vi-complete` | Vi mode completion keybinding | **Verified** by the harness (`bind -l`). |
| **1.gg** | `bind -p NAME` | Restrict output to bindings for named commands | **Fixed**, by the harness: `bind -p NAME` and `bind -P NAME` list one command's keys; a name that is not bound, or not a command, is reported as not bound, status 0, as in Bash. Keys are now spelled as terminals send them (`"\e[H"` and `"\eOH"` for Home, `"\e[1;5H"` for Ctrl-Home) rather than `"Home"`, so `bind -p` output reads back, and on Windows a binding spelled that way binds the key, as an `.inputrc` written for Bash expects. |
| **1.ii** | Trap job notify | Print job notifications when trap completes | **Fixed**, by the harness: at the prompt a `CHLD` trap runs, then the job notices are printed, as in Bash. cash had refused `trap … CHLD`; it now emulates it (spec D64). |
| **1.jj** | Compfunc 124 | Reload compspec and retry completion | **Verified** by the harness. |
| **1.rr** | Sourcing notify | Suppress job notifications while sourcing | **Fixed**, by the harness: a job that finishes while a file is sourced is reported when the file is done. The harness also found that cash reported jobs only at the prompt, where Bash reports them when a foreground command finishes; laid them out differently (`[1]+    PID`, `[1]+Done<tab>cmd`, no `Exit N`); and lost the `Done` of a job that finished before the next one started. All now as in Bash. |
| **2.a-o** | Readline 8.3 | `search-ignore-case`, `force-meta-prefix`, etc. | **Not applicable.** These are GNU Readline's own variables and commands; cash's line editor is Reedline, which has none of Readline's variable set to extend. `bind` reads Readline's syntax and maps the commands Reedline has (D40, the harness's `bind` cases). |

---

## 3. POSIX Mode Changes

| Item | Feature | Upstream Description | Cash Status |
| :--- | :--- | :--- | :--- |
| **1.d** | `jobs` removes jobs | `jobs` clears reported terminated jobs | **Verified** by probe. |
| **1.h** | `umask` options | Full POSIX symbolic modes | **Verified** by probe (`-S`, `-p`, `a+w`, `g-w,o-rwx`). |
| **1.k** | `command declare` | See section 1 | **Fixed** (POSIX mode only). |
| **1.z** | `test <` / `>` locale | Locale collation in POSIX mode | **Verified** by probe in the oracle's default locale; other locales are untested. |
| **1.nn** | `wait -n` table drain | Removes jobs from the table in POSIX mode | **Fixed.** Cash now keeps Bash's list of finished jobs' statuses: `wait PID` reads one after the job has gone, and `wait -n` returns each finished job once, oldest first, and forgets it only in POSIX mode. Probed with an external job, since `$!` is empty for a builtin-only one (divergence 14). |
| **1.qq** | POSIX notify timing | Notifications when POSIX specifies | **Fixed**, by the harness: in POSIX mode a finished job is reported only at the prompt, not after a foreground command, a sourced file or when the next job starts (which then takes a new id), as in Bash. |
| **1.tt** | Function names | Non-identifier function names in POSIX mode | **Verified** by probe. |

---

## 4. Build and Internal Architecture (Non-Contractual)

| Item | Feature | Description | Cash Status |
| :--- | :--- | :--- | :--- |
| **1.n** | `patsub_replacement` default | Build-time default option | Cash defaults to `true`. |
| **1.p** | Loadables (`kv`, `strptime`) | C-ABI dynamic plugins | Not applicable: Windows dynamic loading disabled (status 2). |
| **1.vv** | `fltexpr` | Loadable floating-point let | Not applicable: Cash does not load C-ABI modules. |
| **1.ll** | Documentation | Man page updates | Reference only. |
| **1.ww** | Cross-compile Makefile | Makefile targets | Cash builds via Cargo. |

---

## 5. Deliberate Win32 Divergences

| Item | Feature | Bash Behavior | Cash Win32 Architecture |
| :--- | :--- | :--- | :--- |
| **1.m** | `printf %ls` / `%lc` | Wide character formatting via libc `wchar_t` | Windows consoles and streams are native UTF-8/UTF-16; Cash uses standard Rust UTF-8 formatting. |
| **1.hh** | `$BASH` for `su` | Replaces `$BASH` with login shell under `su` | Windows has no `su` (Cash provides `elevate` for UAC). |
| **1.mm/oo**| Procsub `wait` | Waiting on FIFO process substitution handles | Process substitutions on Windows use temp files and jobs (D20, D26). |

---

## Next steps

None: every item above is probed, fixed, or recorded as not applicable or a deliberate
divergence. `/dev/tcp` (above) is not a 5.3 item.
