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

## 3. `type -a` renders a mixed path separator

**Seen:**

```text
❯ type -a ls
ls is a shell builtin
ls is C:/Program Files/Git/usr/bin\ls.exe
```

Forward slashes for the directory, a backslash before the file name. D3 says one
canonical spelling, rendered on the way out; this path is being assembled after that
point, by joining a rendered directory to a raw file name.

**Where to look:** the `type` builtin's external-candidate path, and whichever join
produces the candidate — the rendering needs to happen on the whole path, not the
directory.

**Note:** the repeated lines in that listing are not a bug. `PATH` genuinely holds
`Git/usr/bin` five times on this machine, and bash prints one line per hit too.

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

## 5. Startup files: two gaps left after the `BASH_ENV` fix

A review of cash's rc/profile handling against bash's. The order and the flags match —
`--login`, `--noprofile`, `--norc`, `--rcfile`, and the `~/.bash_profile` →
`~/.bash_login` → `~/.profile` fallback chain are all as bash does them, with `~/.cashrc`
sourced after `~/.bashrc` as cash's own addition. Git for Windows' `/etc/bash.bashrc`
sources cleanly.

**Fixed while reviewing:** `$BASH_ENV` in a non-interactive shell refused with "not yet
implemented" — so `BASH_ENV=setup.sh cash script.sh` failed before running the script at
all. It now expands the value and sources the file if it is there, as bash does. Same for
`$ENV` in an interactive `sh`, which was silently ignored.

**Still missing:**

- **`~/.bash_logout`.** bash sources it when a *login* shell exits. cash has no reference
  to it anywhere, so a logout hook a user has carried from Linux never runs.
- **No system-wide rc or profile on Windows.** `get_system_rc_path()` and
  `get_system_profile_path()` both return `None`, by design — there is no `/etc`. That is
  defensible, but it is also *why* `ls` has no colour by default (the alias lives in
  `/etc/bash.bashrc` on a distro) and why per-machine configuration has nowhere to go.
  Worth deciding whether something like `%ProgramData%\cash\cashrc` should exist.

---

## 6. `uname -n` still reports the DNS spelling

`hostname`, `$HOSTNAME` and `$COMPUTERNAME` now all answer `DESKTOP-TOMC`. `uname -n`
answers `desktop-tomc`, because it is uutils' and takes the nodename from the DNS API.

Carrying a whole `uname` to change one field is out of proportion — uutils' values for
`-s`, `-r`, `-v` and `-o` are good — so this is left until something trips on it.

**Related, and more likely to bite:** `uname -s` answers `Windows_NT` where Git Bash
answers `MINGW64_NT-10.0-26200`. The near-universal Windows check in a shell script is

```bash
case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) ... ;; esac
```

which does **not** match under cash. `$OSTYPE` is `windows` (Git Bash says `cygwin`), so
that idiom misses too. Claiming to be MINGW would be a lie, and cash is not MSYS — but
the consequence is that a script's Windows branch is skipped, which is worth a divergence
row and a `doctor` note rather than silence.

---

## 7. Which version should the prompt claim?

With no rc file the default prompt is bash's `\s-\v\$`, and cash renders it
`cash.exe-0.5$`. The `.exe` is fixed — `\s` now strips it, so it reads `cash-0.5$` — but
the number is still a question, because there are three of them:

| source | value |
|---|---|
| `cash-core`'s crate version, which `\v` uses today | 0.5 |
| the `cash` binary's own version (`PRODUCT_VERSION`) | 0.1.0 |
| `$BASH_VERSION`, which cash reports for compatibility | 5.2.37 |

bash's `\v` means "the version of bash", so today the prompt and `$BASH_VERSION` disagree
about what shell this is. `cash-core` cannot see the product version — that constant
lives in `cash-shell`, which depends on it, not the other way round — so fixing it means
either passing the product version down or deciding the prompt should say `5.2`.

---

## 8. `cash doctor` says nothing about `sudo`

**Seen:** Windows 11 24H2 ships `C:\Windows\System32\sudo.exe` — Microsoft's Sudo for
Windows — but it is **off by default**, gated behind Settings → System → For developers.
On this machine it is present and `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Sudo`
has `Enabled = 1`; on a machine where it is not enabled, running it fails in a way that
does not explain itself.

**Why cash should care:** it is the one command in the elevation domain cash also answers
for, with the `elevate` builtin (D45). Either route produces a child cash cannot put in
its job object (D42, §4 #12), so the shell's behaviour is the same — but a user who types
`sudo` and gets an unhelpful failure has no way to know the feature is switched off.

**Not carried, deliberately:** Windows supplies it, and by the rule that decides what cash
carries — does the tool have to agree with cash about something cash owns? — `sudo` does
not. `doctor` reporting on it is the whole fix.

