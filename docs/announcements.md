# Announcing cash: drafts

Three drafts for the user to post, written 2026-10-30, each in the first person; nothing
here has been posted. Change what reads wrong, then post from your own accounts. The
numbers are the README's of that day (about 900 scripts from about 50 projects giving Git
Bash 5.3's output, about 250 tools, 9,000+ tests); check them against the README on the
day of posting.

## Before posting

- **Show HN's rules** (<https://news.ycombinator.com/showhn.html>, read 2026-10-30): for
  "things people can run on their computers", made by the poster, who is around to
  answer; the title must begin with `Show HN:`; say what is personally interesting and
  how and why it was built; make it easy to try without signing up; "Please don't ask
  friends to upvote or comment." A new Show HN sits on the `shownew` page until it has
  a few points. No rule says when to post; the usual advice is a weekday morning, US
  time, and to be at the keyboard for the first hours.
- **r/rust and r/commandline**: their rules pages cannot be read from here (Reddit is
  blocked for the tools), so read each sidebar first. What is known second-hand of
  r/rust: the Rust project's code of conduct applies, commercial self-promotion gets
  scrutiny, a project post is fine, and AI-generated content falls under the low-effort
  rule, with a project of disclosed AI origin allowed when it is good. The commits of
  this repository name Claude as co-author, so say so in a sentence rather than be asked;
  a suggested one is in each draft, to keep or drop.
- **Answer the same questions everywhere**: "why not WSL", "why not MSYS2", "what breaks",
  "is `uname` lying", "how is this not brush". The answers are in the README's "Why
  bother" and "What is different", `cash help differences` and docs/compatibility.md.
- The GIF clips are in the README; a link to the repository shows them, so the posts
  need no images of their own.

## Show HN

Title (76 characters):

```text
Show HN: Cash - Bash 5.3 for Windows on Win32, the Unix tools in one binary
```

URL: `https://github.com/tomcoolpxl/cash`

Text:

```text
cash is a Bash-language shell for Windows whose execution model is Win32: no POSIX
emulation layer, no msys-2.0.dll, no VM. Processes, paths and handles are native, and
the scripts are ordinary bash. The shell and the Unix userland are one file, cash.exe:
Bash 5.3, the coreutils, awk, sed, grep, diff, find, xargs, tar, zip, 7z, rar, gzip,
xz, zstd, top, lsof, ss, about 250 tools in all. Put that file on a bare Windows
machine and it has a complete Bash environment.

Why: Git Bash, MSYS2, Cygwin and WSL all work by making Windows pretend to be Unix. I
wanted the bash language on a native Windows execution model. So C:\, C:/ and /c/
paths all work and arguments are never rewritten for the program you start; jobs are
Windows job objects, Ctrl-Z stops a program and kill %1 ends one; lsof -i :8080 and
ss -ltn read Windows' own socket tables; top shows a meter per core.

Where it stands: about 900 scripts from about 50 open-source projects (nvm, bats,
bash-completion, shunit2, bashunit, direnv's stdlib, acme.sh's helpers, oh-my-bash and
GNU Bash's own test suite among them) give the same output as Git Bash 5.3, and 9,000+
tests run on every release. Written in Rust since September 2026, with Claude Code as
a pair and Git Bash 5.3 as the oracle for every behaviour. The interpreter started
from brush (MIT) and was heavily modified; the tools are ports of uutils coreutils,
sed and diffutils taken in as cash's own, awk and bc from posixutils-rs, and cash's
own tar, zip, 7z and rar, top, lsof, ss, a man-page browser and a small game.

Install: scoop bucket add tomcoolpxl https://github.com/tomcoolpxl/scoop-bucket, then
scoop install cash. Or the per-user setup.exe on the releases page, no admin needed.
MIT.

Known limits: Windows only, by design. uname -s says Windows_NT, not MINGW, so the
usual "am I on Windows" idiom in scripts needs one more pattern. Some GNU options are
refused by name rather than half-done. cash help differences lists where it knowingly
differs from Bash and why.

I would like to hear which of your scripts break, and what a Git Bash user misses.
```

## r/rust

Flair: whatever the sidebar names for projects (often "🛠️ project"). Text post with the
link in the body.

Title:

```text
cash: a Bash 5.3 shell for Windows in Rust, native Win32, with coreutils, sed, awk, grep, tar and 7z built into one binary
```

Body:

```text
Repository: https://github.com/tomcoolpxl/cash (MIT)

cash is a Bash-language shell whose execution model is Win32: no POSIX emulation
layer, no msys-2.0.dll, no VM. The shell and the Unix userland are one binary,
cash.exe: Bash 5.3, the coreutils, awk, sed, grep, diff, find, xargs, tar, zip, 7z,
rar, gzip, xz, zstd, top, lsof, ss, about 250 tools in all. A bare Windows machine
with that file has a complete Bash environment.

The Rust side, for those who like that part:

- The interpreter started from brush and was heavily modified; parser, shell and
  builtins are workspace crates, with a cash-win32 crate for the Win32 work: job
  objects for job control, the console's own key events for read -n and Ctrl-C, the
  IP Helper tables for lsof and ss, the Restart Manager for fuser.
- The tools are internalized rather than vendored: uutils coreutils 0.12, uutils sed
  and diffutils, awk and bc from posixutils-rs, reedline and crossterm with patches,
  sevenz-rust2 for 7z. An xtask compares each against upstream's newest release or
  commits every week and opens an issue when something wants porting.
- diff uses the similar crate's Myers in linear space over GNU's equivalence classes,
  so two 100,000-line files that differ throughout take a quarter of a second where
  the upstream crate took minutes and 17 GB.
- Process substitution without /dev/fd: <(...) is a pipe opened once, >(...) a temp
  file written in place, and the builtins know both.
- A size budget and a performance budget (perf-budget.toml) are checked in CI; cash.exe
  is about 50 MB with every tool and help page inside.

Where it stands: about 900 scripts from about 50 open-source projects give the same
output as Git Bash 5.3, GNU Bash's own test suite was triaged against it, and 9,000+
tests run on every release. Built since September 2026 with Claude Code as a pair,
every behaviour checked against Git Bash 5.3 before it was kept.

Install: scoop bucket add tomcoolpxl https://github.com/tomcoolpxl/scoop-bucket, then
scoop install cash; or the per-user setup.exe on the releases page.

Happy to answer questions about the Win32 side, the internalized crates, or what it
took to get job control and Ctrl-C right on Windows.
```

## r/commandline

Title:

```text
cash: Bash 5.3 on Windows without MSYS or WSL, one exe with the Unix tools, native paths, jobs, lsof, ss and top
```

Body:

```text
Repository: https://github.com/tomcoolpxl/cash (MIT), with GIF clips of each point.

cash is a Bash-language shell for Windows that runs as a native Win32 program: no
POSIX emulation layer, no msys-2.0.dll, no VM. One file, cash.exe, holds Bash 5.3 and
about 250 Unix tools: the coreutils, awk, sed, grep, diff, find, xargs, tar, zip, 7z,
rar, gzip, xz, zstd, top, lsof, ss and more. Nothing else to install.

What you get at the prompt: C:\, C:/ and /c/ paths all work and arguments are never
rewritten for the program you start; Ctrl-Z stops a program, bg and fg resume it,
kill %1 ends a background job; lsof -i :8080 names the process on a port and ss -ltn
lists the listeners; top has a meter per core, sorting and a process tree; man is a
browser; a command that does not exist turns red as you type, with suggestions from
history and Ctrl-R; Alt-Left and Alt-Right step through where you have been.

Compatibility: about 900 scripts from about 50 open-source projects give the same
output as Git Bash 5.3. uname -s says Windows_NT, not MINGW, and cash help
differences lists where it knowingly differs from Bash and why.

Install: scoop bucket add tomcoolpxl https://github.com/tomcoolpxl/scoop-bucket, then
scoop install cash; or the per-user setup.exe on the releases page, no admin. It adds
a Windows Terminal profile.

Built in Rust since September 2026, with Claude Code as a pair and Git Bash 5.3 as
the oracle. Tell me what breaks for you.
```
