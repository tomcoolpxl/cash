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

## 8. `jobs` and `wait`: what still differs from Bash 5.3

**Seen (2026-09-30)** while fixing `jobs` losing a finished job's status (`jobs`, then
`wait PID`, answered 127). Each script below was run as a file by Git Bash 5.3.15 and by
cash; `$X` is an external program that takes `-c`. None of these is fixed.

| Script | Bash 5.3.15 | cash |
| --- | --- | --- |
| `"$X" -c 'exit 3' & sleep 0.5; while [ -n "$(jobs -pr)" ]; do sleep 0.1; done` | ends at once | never ends |
| `"$X" -c 'exit 3' & sleep 0.5; echo "$(jobs)"` | `[1]+  Exit 3 …` | `[1]+  Running … &` |
| `"$X" -c 'exit 3' & p=$!; wait $p; wait; wait $p; echo $?` | 127 | 3 |
| two jobs, 3 then 4, both ended: `wait; wait $a; echo $?; wait $b; echo $?` | 127, 4 | 3, 4 |
| `"$X" -c 'exit 3' & sleep 0.5; jobs > /dev/null; wait %1; echo $?` | `wait: %1: no such job`, 127 | 3 |
| `"$X" -c 'exit 3' & sleep 0.5; wait -n; wait %1; echo $?` | `wait: %1: no such job`, 127 | 3 |
| `"$X" -c 'exit 3' & sleep 0.5; "$X" -c 'sleep 0.5; exit 4' & wait %1; echo $?; wait %2; echo $?` | 3, 4 | 4, then `wait: no such job: %2`, 1 |
| `wait %5; echo $?` | `wait: %5: no such job`, 127 | `wait: no such job: %5`, 1 |
| `v=old; wait -n -p v; echo "${v-unset}"` | `unset` | `old` |
| `set -o posix; "$X" -c 'exit 3' & sleep 0.5; jobs` | `[1]+  Done(3) …` | `[1]+  Exit 3 …` |

What is behind them, as far as it is known:

- **A subshell sees the parent's jobs as they were when last polled.** `$(jobs)` and
  `jobs | …` get a copy of the job table (`Shell::clone`, `JobManager::snapshot`) that is
  taken without polling the jobs, so a job that has ended since still reads `Running`.
  `jobs -pr > file`, in the shell itself, is right. This is the one a script is likely to
  meet: a loop that waits for `$(jobs -pr)` to be empty does not end.
- **A plain `wait` forgets more in Bash.** It forgets every saved status, and keeps only
  that of `$!` when that job had ended and not been reported. cash forgets the ones
  `wait -n` returned or `jobs` showed.
- **A finished job's number.** In Bash a finished job keeps its number until it is
  reported, and `%N` names nothing after that. A cash script reaps a finished job when the
  next one starts, which takes its number, and `wait %N` still finds a status that was
  saved under that number. Keeping `%N` after `jobs` was asked for with the fix; Bash
  answers `no such job` there.
- The last three are a message, a status and a variable: `wait` for a job that does not
  exist says it Bash's way round and returns 127, `wait -n -p VAR` unsets `VAR` when it
  has nothing to return, and POSIX mode prints a failed job as `Done(3)`.

---

## 9. The `/dev` names outside a redirection, and three things seen beside them

**Seen (2026-09-30)** while fixing `< /dev/stdin` ("failed to redirect to C:/dev/stdin").
A redirection to `/dev/null`, `/dev/stdin`, `/dev/stdout`, `/dev/stderr` or `/dev/fd/N`
and `source` of one now work (spec D7). Each script below was run with `x` piped to it,
by Git Bash 5.3.15 and by cash. None of these is fixed.

| Script | Bash 5.3.15 | cash |
| --- | --- | --- |
| `cat /dev/stdin`, `cat /dev/fd/0` | `x` | `/dev/stdin: The system cannot find the path specified.`, 1 |
| `cat /dev/null > f`, `cp /dev/null f` | empties `f`, 0 | the same message, 1 (the first still empties `f`) |
| `echo x \| tee /dev/stderr` | `x` on both | the same message, 1 |
| `[ -e /dev/stdin ]`, `[ -e /dev/null ]`, `[ -c /dev/null ]`, `[ -p /dev/stdin ]`, `[ -w /dev/stdout ]`, `[ -e /dev/fd/0 ]`, `[[ -e /dev/stdin ]]` | 0 | 1 |
| `cat <&1`, `cat < /dev/stdout` (output a pipe) | `cat: -: Bad file descriptor`, 1 | prints `x`, 0 |
| `echo x \| tee >(cat >&2)` | `x` on both | `\\.\pipe\cash-procsub-…: The parameter is incorrect.`, 1 |
| `cat 3< f <&3` | the contents of `f` | `operation not supported on this platform: fd redirections`, 1 |

What is behind them, as far as it is known:

- **An argument reaches a command as written (D4).** cash knows the names where it opens
  the file itself. A bundled tool opens its own files, as a program on `PATH` does, and
  Windows has no `/dev`. The `/c/…` spelling has the same cliff and gets a hint
  (`warn_about_unix_drive_spellings`); these names get none. Making them work for the
  bundled tools would mean cash translating arguments for tools whose grammar it does
  not know, which is what D28 was revoked for.
- **The file tests ask the filesystem.** §9 of the spec has listed `[ -e /dev/null ]` as a
  D7 gap since the first baseline. `cash_win32::path::accept` already tells the names
  from paths; the tests do not ask it.
- **The last three have nothing to do with the names** and were only met on the way; all
  three were there before the fix, and their causes have not been looked for.

---

## 10. Tests that failed once on a busy machine, and not since

**Seen (2026-09-30)**, a day on which up to ten sessions built and ran the suite at the
same time. Each test below failed in one full run (`--retries 0`) and passed alone; none
has failed on a machine that was doing nothing else. The ones whose fault could be named
were fixed that day (a fixed 300 ms before typing at a `read`, two jobs 150 ms apart, a
loop sized for a quiet machine), and the ConPTY tests now run four at a time
(`.config/nextest.toml`). These are what is left, to look at if one fails again:

- `git_prompt::git_ps1_shows_the_branch_its_state_and_the_upstream` (timed out)
- four `cash-sed` tests, in one run
- `completion_scripts::docker_completion_works`
- `job_groups::at_the_prompt_a_background_job_survives_ctrl_c`, which already runs alone
- `pty-oracle` `cash_leaves_the_screen_bash_leaves`, a different case each time; it
  already runs alone, and its own documentation says why it is bound to timing
