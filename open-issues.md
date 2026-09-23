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

## 4. A bundled tool has no path a script can exec

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

