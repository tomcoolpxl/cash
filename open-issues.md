# Open issues

Things noticed while using cash that are not yet fixed, kept here so they stop being
rediscovered by accident. Each entry says what was seen, not what the cause is, unless the
cause is known.

---

## 1. Fast backspace drops characters from the display — not cash's

**Seen:** typing is smooth, but holding backspace to delete a long command line gets out of
step: the cursor travels faster than the letters disappear, so characters linger on screen
that are no longer in the buffer. Releasing and pressing again resynchronises it.

**Narrowed:** it reproduces in **PowerShell with Starship**, with cash nowhere in the
picture. So it is not cash's line editor, not reedline, and not D26's fd handling — it is
the prompt or the terminal repainting slower than a held key repeats.

**Left here anyway** because cash runs under the same Starship prompt (D37), so anyone
using cash will see it and reach for this file. The remaining question worth answering is
only whether cash makes it *worse* than PowerShell does — same terminal, same Starship
config, same held key, compare. If it is identical, this belongs upstream.

**Answered (2026-09-27): the lag is Windows Terminal's; cash made it more work.** Both
shells were driven through a ConPTY, the layer Windows Terminal reads from, with the same
Starship config in this repository: Backspace held at the machine's repeat rate (33 ms), and
pastes written in one piece, as the terminal delivers them. The stream was replayed onto a
screen after each chunk to see when each key's effect appeared.

| | cash before | cash after | PowerShell |
|---|---|---|---|
| Time until a Backspace shows, median | ~1 ms | ~1 ms | ~13 ms |
| Stale characters on screen | none | none | one, briefly |
| Bytes per Backspace, prompt at the bottom | 182 | 61 | 68 |
| Bytes per Backspace, prompt on an empty screen | 319 | 60 | 68 |
| 2000-character paste, until drawn | 650–680 ms | 30–40 ms | 220–260 ms |

cash was never the one falling behind: every update left the ConPTY within a few
milliseconds and none showed a character already deleted. So the characters that linger in
the window are the terminal drawing late — which fits it reproducing in PowerShell. What
cash did was give the terminal more to draw: Reedline repainted the whole Starship prompt on
every key, and every paste crawled in through one console round trip per key record, then
waited 100 ms more. Four patches to the vendored Reedline and Crossterm (their
`CASH-PATCHES.md`) fixed those. What remains of the lag, if anything, is Windows Terminal's
to fix.

---

## 2. A session looked like cash but was PowerShell — cash had exited unnoticed

**Seen:**

```text
cash\target\debug [ master][!?]
❯ pwd

Path
----
C:\Users\thraa\github\cash\target\debug
```

**Resolved, and it is not `pwd`.** That is PowerShell's `Get-Location` rendering, byte for
byte — blank line, `Path`, `----`, backslashes. cash's `pwd` is a builtin (`type pwd` says
so) and prints a single line, `C:/Users/thraa/github/cash/target/debug`, per D3. Both were
run side by side to confirm. The multi-column `ls .\build\` above it is consistent with a
real `ls.exe` on `PATH`, which PowerShell runs happily.

**Closed: the window was PowerShell from the start** — it was opened by accident, and cash
never ran in it. No cash session ended unexpectedly.

**What is worth keeping** is the reason it was not obvious: Starship renders an identical
prompt under both shells, so nothing on screen says which shell is answering. `$SHELL`
names cash (D5, §4 #21), so a prompt element reading it would tell the two apart — worth
doing before chasing any bug reported from a session whose shell is assumed rather than
checked.

**Adjacent:** Starship in that session reported `Scanning current directory timed out`
inside `target/debug`, which is Starship's own scan_timeout rather than cash — but a
directory that large making the prompt slow is worth knowing.

---

## 3. `type -a` renders a mixed path separator — closed

`type`, `type -a`, `type -p` and `which` now all render paths through `cash_win32::path::render`,
guaranteeing canonical forward slashes without mixed separators (`C:/Program Files/.../ls.exe`).

---

## 4. A bundled tool has no path a script can exec — closed

**Closed** by spec D58: `which ls` prints `C:/…/cash.exe/ls`, a virtual path cash runs as
`ls`, so `LS=$(which ls); "$LS" -la` works; no launcher files were shipped. What follows
is the issue as it was recorded.

**Seen:** `ls` is a builtin (D48 carries it), so `which ls` answers `ls: shell builtin`
and `$(which ls)` yields nothing to run. A script that captures a tool's path —
`LS=$(which ls); "$LS" -la` — gets an empty command.

**There is an answer, and it is undiscoverable.** cash re-enters its own binary to run a
bundled tool, so the real executable is cash itself:

```bash
"$(which cash)" --invoke-bundled ls -la
```

That is the same code path the builtin uses, which is how redirections and pipes work for
the bundled tools already.

**Worth deciding:** whether `which`/`type` should say so — something like
`ls: shell builtin (cash --invoke-bundled ls)` — against the cost that both commands'
output is parsed by scripts. Shipping shim executables is the other option, and the one
Scoop uses, but it puts files on `PATH` that cash does not control.

**Not a concern, checked:** alias recursion. bash does not re-expand an alias for its own
name, and `command ls` bypasses aliases — so `alias ls='ls --color=auto'` is safe. The
missing path is the real issue, and it is an argument for defaulting colour *inside* the
builtin rather than aliasing.

---

## 5. Startup files: two gaps left after the `BASH_ENV` fix — closed

A review of cash's rc/profile handling against bash's:
- **`$BASH_ENV` and `$ENV`**: expanded and sourced if present.
- **`~/.bash_logout` and `logout` builtin**: `logout` is implemented (requiring a login shell) and `~/.bash_logout` is sourced on login shell exit.
- **System-wide rc and profile on Windows**: `get_system_rc_path()` points to `%ProgramData%\cash\cashrc` and `get_system_profile_path()` points to `%ProgramData%\cash\profile`. If either file exists on the machine it is sourced, providing a clean place for machine-wide configuration without requiring `/etc`.

---

## 6. `uname -n` hostname spelling — closed

`hostname`, `$HOSTNAME`, `$COMPUTERNAME`, and now `uname -n` all answer `DESKTOP-TOMC`.
`uname` nodename output is intercepted and unified to the Windows NetBIOS computer name,
so `[ "$(uname -n)" = "$COMPUTERNAME" ]` and `[ "$(hostname)" = "$COMPUTERNAME" ]` evaluate
to true while preserving uutils' native implementations for `-s`, `-r`, `-v`, and `-o`.

**Related, and more likely to bite:** `uname -s` answers `Windows_NT` where Git Bash
answers `MINGW64_NT-10.0-26200`. The near-universal Windows check in a shell script is

```bash
case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) ... ;; esac
```

which does **not** match under cash. `$OSTYPE` is `windows` (Git Bash says `cygwin`), so
that idiom misses too. Claiming to be MINGW would be a lie, and cash is not MSYS — this is
now recorded as divergence #25 in `spec.md` (§4) and diagnosed in `cash doctor`.

---

## 7. Which version should the prompt claim? — closed

With no rc file the default prompt is bash's `\s-\v\$`, and cash rendered it
`cash.exe-0.5$`: the `.exe` Windows adds to the name, and `cash-core`'s crate version,
which is not the product's and not what `cash --version` prints.

**Both are fixed.** `\s` strips the extension on Windows, and the version escapes now
report the version the shell is handed at startup — the one it already publishes as
`$BRUSH_VERSION` — falling back to the crate's only if there is none. `coolfetch` reads
the same one, so the banner and the prompt cannot disagree. `$BASH_VERSION` stays
`5.2.37`: that is the *interface* version a script tests, and it is a different question
from which shell is running.

---

## 8. `jobs` and `wait`: what differed from Bash 5.3 — closed

**Seen (2026-09-30)** while fixing `jobs` losing a finished job's status; **closed
(2026-10-02)** in TODO 5.1: each of the ten scripts that were listed here now prints what
Git Bash 5.3.15 prints, and spec D11 says how. `crates/cash/tests/it/bash_gaps.rs`,
`jobs_and_wait_answer_as_bash_53_does`, runs them all.

---

## 9. The `/dev` names outside a redirection, and three things seen beside them — closed

**Seen (2026-09-30)** while fixing `< /dev/stdin` ("failed to redirect to C:/dev/stdin"),
**closed (2026-10-02)** in 1.3.4: the names as an argument of a bundled tool
(`cat /dev/stdin`, `tee /dev/stderr`, `cp /dev/null f`) and in the file tests
(`[ -e /dev/null ]`) work (spec D7); `cat <&1` with output a pipe, which read standard
input, fails as in Bash, in Windows' words ("Access is denied"); `cat 3< f <&3` runs
(spec D26); `echo x | tee >(cat >&2)` was fixed in 1.3.3 (spec D17). What is left of the
names is entry 12.

---

## 10. Tests that fail on a busy machine: run them on an idle one

**Seen (2026-09-30)**, a day on which up to ten sessions built and ran the suite at the
same time. Each test below failed in one full run (`--retries 0`) and passed alone; none
has failed on a machine that was doing nothing else. The ones whose fault could be named
were fixed that day (a fixed 300 ms before typing at a `read`, two jobs 150 ms apart, a
loop sized for a quiet machine), and the ConPTY tests now run four at a time
(`.config/nextest.toml`).

**Decided (2026-10-02), by the user: these are run on a machine whose CPU is free, with
nothing else of this project beside them, and their failing under load is never fixed.**
One that fails is run again on an idle machine and judged from that. Seen so far:

- `git_prompt::git_ps1_shows_the_branch_its_state_and_the_upstream` (timed out)
- four `cash-sed` tests, in one run
- `completion_scripts::docker_completion_works`
- `job_groups::at_the_prompt_a_background_job_survives_ctrl_c`, which already runs alone
- `pty-oracle` `cash_leaves_the_screen_bash_leaves`, a different case each time; it
  already runs alone, and its own documentation says why it is bound to timing
- On 2026-10-02, with Playwright tests, Docker and this project's own full runs on the
  machine: `bc_cash::bc_matches_gnu_bc`, `corpus::every_corpus_script_runs_with_crlf_endings`
  and `corpus::the_ci_glue_script_matches_real_bash` (timed out at 15 s),
  `fuser_lsof::fuser_finds_tcp_and_udp_owners`, `cash-sed` `test_sed::pi`, two ConPTY
  tests, and `cash_leaves_the_screen_bash_leaves` again (100 and 135 s; 65 s and passing
  alone). Each passed alone.

---

## 11. `read`, `select` and `mapfile` at a console: what a terminal does differently

**Seen (2026-09-30)**, once these took the keys of a console themselves
(`cash_win32::conin`) so that `-t`, `-d`, `-n`, `-s` and Ctrl-C work there. None of these
is known to bother anyone, and each would cost more than it is worth until one does.

- **The console's own editing of a line is gone** from a plain `read`: the arrow keys,
  Escape and the function keys do nothing there. A line is edited as a terminal driver
  lets it be: Backspace, Ctrl-W, Ctrl-U. `read -e` has an editor. This is the price of
  Ctrl-C working, since a console collecting a line makes an event of it.
- **Keys typed past the count of `-n` are shown when something reads them,** not when
  they are typed: a console shows keys only while something reads them.
- **A line not ended when `-t` runs out is dropped.** A terminal keeps it for the next
  read. Keeping it means putting the keys back into the console's queue, and not showing
  them a second time when they are read again.
- **An answer to a terminal query that arrives before `read` has begun** can be taken by
  an older console host for itself; Windows Terminal passes it through. The keys are
  taken before the prompt is shown, so a script that asks after its prompt is safe.

---

## 12. Small differences left in the `/dev` names

**Seen (2026-10-02)** in TODO 4.6, measured against Git Bash 5.3.15; each was judged not
worth its cost, or a decision.

- **`/DEV/STDIN`** works with Git Bash's MSYS programs (`cat /DEV/STDIN`), which take
  `/dev` paths without regard to case. Bash's own redirections (`< /DEV/NULL`: "Permission
  denied") and file tests (`[ -e /DEV/NULL ]`: false) do not, and cash follows Bash's own.
- **`read x < 'CONIN$'`** fails: a reserved Windows name is an ordinary file name (D28),
  and the console's name is `/dev/tty` (D7).
- **`exec 3<>/dev/tty`** can be read from and not written to: a console's keys and its
  screen are two handles, and a descriptor is one (spec D7).
- **A read of more than a mebibyte from `/dev/urandom` or `/dev/random`** can come back
  short: they are a pipe of that size, where Linux answers up to 32 MiB at once.
  `dd if=/dev/urandom bs=4M count=1` gets less than 4 MiB; `iflag=fullblock` takes it all.
