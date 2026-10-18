# Decisions

What was decided about cash, why, and what was turned down, by topic, so nothing is
decided twice. The numbered decisions keep the numbers they had in spec.md, where code
and documents cite them ("D7"); numbers missing here were dropped as done or
superseded. Each topic then lists the decisions made in working sessions, most of them
the user's answers, with their dates. A new decision goes here, under its topic.

| D | Decision | Topic |
| --- | --- | --- |
| D1 | Build on brush's code; the interface is Bash 5.3.15 | Scope and foundation |
| D2 | Unmodified POSIX-shaped `.sh` scripts run as they are | Scope and foundation |
| D3 | Windows paths first-class, Unix spellings accepted | Paths, files and text |
| D4 | Arguments are never rewritten | Paths, files and text |
| D5 | `PATH` is the one variable translated, at the process boundary | Running programs and the environment |
| D6 | Every job in a job object, nested in the session's | Processes, jobs and signals |
| D7 | No modes: compatibility behaviours are always on | Scope and foundation |
| D8 | Command resolution is cash's own | Running programs and the environment |
| D9 | brush is absorbed, not vendored | Scope and foundation |
| D10 | The shell's folder is the shell's; paths render at two chokepoints | Paths, files and text |
| D11 | cash owns job control | Processes, jobs and signals |
| D13 | Ctrl-C is graceful first; the job object is the backstop | Processes, jobs and signals |
| D14 | `trap EXIT` runs to completion; a second Ctrl-C forces teardown | Processes, jobs and signals |
| D15 | Exit codes truncate; crashes map to Bash's 128+n | Processes, jobs and signals |
| D16 | Globbing is case-insensitive; `nocaseglob` is honoured | Paths, files and text |
| D17 | Process substitution streams through pipes | Processes, jobs and signals |
| D19 | Suspend by enumerating threads, documented APIs only | Processes, jobs and signals |
| D20 | `\r\n` ends a line wherever cash decides where lines end | Paths, files and text |
| D21 | `kill -TERM` escalates in the background; `kill -9` is immediate | Processes, jobs and signals |
| D22 | Job specs kill trees; bare pids kill one process | Processes, jobs and signals |
| D23 | `test -x` needs an execute ACL and a discriminator | Paths, files and text |
| D24 | `~/.bashrc` is read, then `~/.cashrc` | Startup files, help and messages |
| D25 | Scoop shims are run, not resolved through | Running programs and the environment |
| D26 | Descriptors above 2 work where cash holds both ends | Processes, jobs and signals |
| D27 | `ln -s`: a symlink, a junction for folders, else an error | Paths, files and text |
| D28 | Reserved names stay devices | Paths, files and text |
| D29 | Paths of any length | Paths, files and text |
| D31 | Variable names are case-insensitive; POSIX names upper-cased | Running programs and the environment |
| D32 | `.bat` and `.cmd` arguments are escaped for cmd | Running programs and the environment |
| D33 | Files are opened fully shared, deleted with POSIX semantics | Paths, files and text |
| D34 | `chmod -x` warns and returns 0 | Paths, files and text |
| D35 | No userland required; `cash doctor` names the fix | Running programs and the environment |
| D36 | Prompt commands get a job each, like every command | The prompt, editing and completion |
| D37 | Starship works, and a test says so | The prompt, editing and completion |
| D38 | Shipped through Scoop, an installer, winget and a zip, with a Terminal profile | Packaging, installing and updating |
| D39 | Terminal shell integration is on by default | The prompt, editing and completion |
| D40 | Completion ignores case and quotes as it goes | The prompt, editing and completion |
| D41 | The BOM is stripped; the console is UTF-8 | Paths, files and text |
| D42 | Elevated programs are outside D6 | sudo and elevation |
| D43 | Conformance rests on cash's own cases | Scope and foundation |
| D44 | History is appended at once, safely across tabs | The prompt, editing and completion |
| D45 | cash's own builtins only where a decision or a Windows gap asks | Builtins and bundled tools |
| D46 | App Execution Aliases run as any `.exe` | Running programs and the environment |
| D47 | Windows 10 1809 is the floor; Windows 11 the target | Scope and foundation |
| D48 | coreutils are bundled as builtins | Builtins and bundled tools |
| D49 | Bundled `sed` and `awk` keep CRLF files CRLF; `CASH_EOL=lf` is Linux | Builtins and bundled tools |
| D50 | `fuser` and `lsof` from documented sources, plus the handle walk | Builtins and bundled tools |
| D51 | `ss` is iproute2's over the Windows socket tables | Builtins and bundled tools |
| D52 | MSYS2 and Cygwin programs get Cygwin's argument encoding | Running programs and the environment |
| D53 | `winpaths`: an unquoted `C:\…` typed at the prompt means the path | Paths, files and text |
| D54 | `pkill`, `pidof`, `killall`: one set of name rules, `kill`'s signals | Builtins and bundled tools |
| D55 | `getopt`, `rev`, `clear` and `reset` are util-linux's and ncurses' | Builtins and bundled tools |
| D56 | `bc` is POSIX bc and names GNU's extensions | Builtins and bundled tools |
| D57 | `ping` takes Linux's flags and shadows `ping.exe` | Builtins and bundled tools |
| D58 | `which` prints a path that runs, for the commands cash carries | Builtins and bundled tools |
| D59 | Syntax highlighting is on, and never waits on the disk | The prompt, editing and completion |
| D60 | `abbr`: fish's abbreviations | The prompt, editing and completion |
| D61 | `CASH_TRANSIENT_PS1` collapses the prompt after Enter | The prompt, editing and completion |
| D62 | Folder history: `prevd`, `nextd`, `cdh`, Alt-← and Alt-→ | The prompt, editing and completion |
| D63 | Completions from carapace, when it is installed | The prompt, editing and completion |
| D64 | `CHLD` is cash's own event | Processes, jobs and signals |
| D65 | `cash --link-tools`: hard links programs outside cash can run | Packaging, installing and updating |
| D66 | `where` is `where.exe` with dashes and cash's paths | Builtins and bundled tools |
| D67 | `ls` icons only with a Nerd Font | Builtins and bundled tools |
| D68 | The console is put back before each prompt | The prompt, editing and completion |
| D69 | `cash --init-rc` writes a starter `~/.bashrc`, once | Startup files, help and messages |
| D70 | Background jobs and subshells run inside the shell, on threads | Processes, jobs and signals |
| D71 | A writer whose reader went away ends as SIGPIPE ends it, if it is cash's | Processes, jobs and signals |
| D72 | `help` from one catalogue | Startup files, help and messages |
| D73 | `croot`: a file and folder picker on Alt-E | The prompt, editing and completion |
| D74 | Small tools: only what nothing else answers | Builtins and bundled tools |
| D75 | A per-user installer, `cash --update`, and winget | Packaging, installing and updating |
| D76 | `grep`, `diff` and `cmp` are carried after all | Builtins and bundled tools |
| D77 | The compatibility corners follow Bash and fish | Archives and compression |
| D78 | Archive and compression tools, on shared parts | Archives and compression |
| D79 | Ctrl-R: a history picker | The prompt, editing and completion |
| D80 | `z`: the folder jump | The prompt, editing and completion |
| D81 | `winglob`: at the prompt, a Windows program globs for itself | Paths, files and text |

## 1. Scope and foundation

### D1 — Build on brush's code; the interface is Bash 5.3.15

cash's Win32 layer sits on brush's language layer instead of a rewrite: at M0 every
failure lay in the Win32 layer, and the language passed its 2,631 differential tests.
`$BASH_VERSION` is `5.3.15(1)-release`; Git Bash 5.3.15 is the reference every probe is
checked against. Turned down: a rewrite from scratch; a Windows shell with its own syntax,
which drops D2.

### D2 — Unmodified POSIX-shaped `.sh` scripts run as they are

Windows is the substrate, not the surface syntax. The deliberate exceptions are spec §4's
list, kept short.

### D7 — No modes: compatibility behaviours are always on

Each compatibility behaviour is a superset no Windows script can trip on, so all are on:
`/dev` names, CRLF script source, the BOM stripped (D41), `/usr/bin/env`, Unix path
spellings. `sh`, `bash` and `cash` are cash, ahead of `PATH`; started as `sh` it runs in
POSIX mode with nothing switched off. `/dev/stdin`, `/dev/stdout`, `/dev/stderr` and
`/dev/fd/N` share the shell's descriptor, never reopen a file. Turned down: a `--posix`
flag, shebang sniffing or a `set -o` switch; running the `bash` on `PATH` (often WSL's
launcher); reopening as Git Bash does, which lets `> /dev/stderr` empty a log.

### D9 — brush is absorbed, not vendored

brush's crates are cash's own (`cash-*`), edited directly, nothing upstreamed; `NOTICE`
keeps brush's copyright and the absorbed commit. An upstream fix is a manual port. Turned
down: a library dependency (spec §6 found no clean seam); a soft fork that upstreams; a
hard fork in `vendor/`.

### D43 — Conformance rests on cash's own cases

cash is Windows-only. Conformance rests on the inline-expectation cases, the Windows corpus
of real scripts and recorded Bash 5.2/5.3 probes. Turned down: Git Bash or WSL as a
wholesale reference, where every spec §4 divergence shows as a failure; a Linux build.

### D47 — Windows 10 1809 is the floor; Windows 11 the target

ConPTY sets the floor, which also brings nested job objects, unprivileged symlinks and
POSIX deletes. Windows 10 left support in October 2025; no fallback for older systems is
written.

### Decided in working sessions

- **cash is Windows-only** (the user, 2026-09-28): the Unix code and the Linux CI went;
  the platform's scope stays the user's call.
- **Nothing is invented, nothing ignored**: where Windows lacks the data, cash shows none
  (`ss`'s queues, `top`'s swap, `lsof`'s `-`), and an option it cannot honour is refused by
  name, never accepted and ignored.
- **A Windows tool is shadowed only where it is not what a Bash script means by the
  name** (2026-09-25): `ping`, `reset`, `where` and `tar` are shadowed; `netstat` and
  `strings` are not.
- **A crate cash must change becomes cash's own code**, its licence kept (brush, D9;
  sevenz-rust2, 2026-10-07; the patched uutils tools, reedline and crossterm,
  2026-10-10). Turned down: patched copies in `vendor/`, which is gone.

## 2. Paths, files and text
- **Scripts are read a command at a time, as Bash reads them** (the user, 2026-10-18,
  after 1.26.0): a script parsed whole runs one command at a time, and the rest is
  parsed again when a command changes how text parses (an alias added, extglob
  turned); aliases are expanded on the tokens, before parsing, so an alias's `&&` and
  `;` are operators; extglob starts off. Turned down: fixing both at run time, where
  `gs | less` pipes both commands of an alias and Bash only the last.

### D3 — Windows paths first-class, Unix spellings accepted

cash prints `C:/test/dir` (drive, forward slashes, never a backslash) for `pwd`, `$PWD`,
globs and every path it builds, because a printed path usually becomes an argument to a
native program, where `/c/src` is fatal. `C:/`, `/c/`, `/tmp` and `/dev` names work wherever
cash opens a path itself; anywhere else an argument goes as written (D4). A backslash
follows Bash's quoting (D53 covers the prompt). Turned down: printing `/c/...`; tracking
how a path was typed; bending Bash's quoting so unquoted backslash paths work.

### D4 — Arguments are never rewritten

A child gets exactly the string a variable holds; cash never guesses which arguments are
paths. `winpath` converts on request; quoting for `cmd.exe` (D32) is escaping, not
rewriting. Turned down: MSYS2's argument conversion and its `MSYS2_ARG_CONV_EXCL`.

### D10 — The shell's folder is the shell's; paths render at two chokepoints

cash's `cd` stores the canonical `C:/` form in the shell's own state, without moving the
process's folder, so `$PWD` follows and every path cash resolves is against the shell's
folder; the file-open boundary takes the Unix spellings and converts them (D29). A path
the core computes itself (a glob, a tilde) is fixed where it arises. Turned down:
rewriting the core's path handling.

### D16 — Globbing is case-insensitive; `nocaseglob` is honoured

`nocaseglob` is on by default and `find -name` matches the same way: on a
case-insensitive volume this only removes false negatives. `shopt -u nocaseglob` gives
Bash's matching. Turned down: Bash's case-sensitive default.

### D20 — `\r\n` ends a line wherever cash decides where lines end

Command substitution, `read`, `mapfile`, here-documents and here-strings treat `\r\n` as
`\n`, since Python, .NET and `cmd` emit CRLF and cannot be fixed at the source. Bytes
between external programs are never touched, and builtins write LF. Turned down: stripping
`\r` from data in general.

### D23 — `test -x` needs an execute ACL and a discriminator

True when the ACL grants `FILE_EXECUTE` and the extension is in `PATHEXT` or the file
starts with `#!`. `chmod +x` changes nothing and returns 0, so `chmod +x x.sh && ./x.sh`
works. Turned down: ACLs alone, which call every profile file executable; the extension
alone, which invents a permission model.

### D27 — `ln -s`: a symlink, a junction for folders, else an error

With privilege (admin or Developer Mode) a real symlink; without, a junction for a folder
and an error naming Developer Mode for a file. Turned down: hard links for files, which are
not symlinks and give scripts wrong answers.

### D28 — Reserved names stay devices

`nul`, `con`, `com1` and the rest are devices, as every Windows program sees them;
`/dev/null` discards output. Turned down, after it was built: ordinary files through
`\\?\`, which the bundled tools could neither read nor delete, and translating their
arguments is what D4 forbids.

### D29 — Paths of any length

File operations take paths of any length, whatever the long-path registry setting.
Windows starts no program in a folder over 258 characters, so one is started in the
folder's 8.3 short name, and fails saying why when none fits. Turned down: a clear error
alone, as Git Bash gives.

### D33 — Files are opened fully shared, deleted with POSIX semantics

cash opens files with read, write and delete sharing, and deletes with
`FILE_DISPOSITION_FLAG_POSIX_SEMANTICS`, so `tool > log & rm log` works as on Linux instead
of failing with a sharing violation.

### D34 — `chmod -x` warns and returns 0

Revoking execute means breaking ACL inheritance or a Deny ACE that can lock the owner out,
so `chmod -x` warns and returns 0, as do `-r`, setuid, setgid and sticky. Group and other
bits, which Windows does not have per file, change nothing and say nothing. Turned down:
Deny ACEs; succeeding silently on `-x`; warning about group and other bits.

### D41 — The BOM is stripped; the console is UTF-8

A UTF-8 BOM at the start of a script is stripped, as Windows editors write it; the console
code page is 65001. Pipes between programs keep their runtimes' code pages. Turned down:
injecting `PYTHONUTF8`, `JAVA_TOOL_OPTIONS` and the like into children, against D5.

### D53 — `winpaths`: an unquoted `C:\…` typed at the prompt means the path

With `shopt winpaths`, a word starting with a drive and a backslash, or `\\server`, keeps
its backslashes; elsewhere Bash's escape rules hold (`x=C:\y` keeps Bash's meaning). On in
interactive shells, off in scripts. Turned down: quoting pasted text, impossible without
bracketed paste on Windows; Bash's reading of `\\server\share` as `\servershare`.

### D81 — `winglob`: at the prompt, a Windows program globs for itself

cmd never expands a wildcard, so every program Windows ships reads `*` itself or means
something else by it (`net user NAME *` asks for a password). With `shopt winglob`, a
command under `System32` or `SysWOW64` gets its words unexpanded. On in interactive shells,
off in scripts. Turned down: quoting the star, right in scripts but not for a line typed
from Windows habit; skipping globbing for programs installed elsewhere, which need it.

### Decided in working sessions

- **`chown` and `chgrp` change nothing, warn and return 0** (the user, 2026-10-11): Windows
  sets an owner only with special privileges and has no POSIX groups, so, as `chmod -x`
  (D34), they say so on standard error and succeed, and an install script carries on;
  `enable -n chown` reaches a copy on `PATH`. Turned down: refusing, which stops install
  scripts; leaving them to `PATH` (Git's MSYS copies, none on a bare machine). Kept so
  on 2026-10-12, the warning and help naming Windows' own tools (`sudo icacls FILE
  /setowner USER`, `takeown /f FILE`): a Windows owner may change the access list but
  gets no access from being owner, mode bits are not stored and the group is never
  consulted, and files an elevated `sudo` makes are already yours. Turned down: a real
  owner change under `sudo`, which would set what decides nothing.

## 3. Running programs and the environment

### D5 — `PATH` is the one variable translated, at the process boundary

Scripts see `PATH` colon-separated as `/c/...`; a child's environment gets the Windows
form. Apart from `$SHELL`, nothing else is translated or injected: `GOPATH`, `PYTHONPATH`
and `CLASSPATH` pass verbatim. Turned down: a list of path variables to translate.

### D8 — Command resolution is cash's own

Builtin, function or alias, then a `PATH` search with `PATHEXT`, dispatched by extension
before any file is read: `.exe`/`.com` directly, `.cmd`/`.bat` through `cmd.exe /d /s /c`
(D32), `.ps1` through `pwsh` or else `powershell`, an extensionless file by its shebang.
The extension comes first because an App Execution Alias reads as nothing (D46). Every
D3 spelling works as a command name. Turned down: special-casing
Scoop shims (D25).

### D25 — Scoop shims are run, not resolved through

A shim is an executable like any other, so cash never needs Scoop; the cost is an extra
process per job. `cash doctor` and completion read a `.shim` file. Turned down: resolving
through shims, which ties cash to Scoop.

### D31 — Variable names are case-insensitive; POSIX names upper-cased

Windows supplies `Path`, `Temp` and `UserProfile`, so lookup ignores case for every
variable (an exact match wins), and `PATH`, `HOME`, `TMPDIR` and `USER` are upper-cased on
import; an unset `$seconds` reads `SECONDS`. Turned down: Bash's case-sensitive lookup,
where `$PATH` is empty; limiting the fallback to exported names.

### D32 — `.bat` and `.cmd` arguments are escaped for cmd

`cmd.exe` re-parses the line, so cash caret-escapes `&`, `|`, `^`, `<`, `>` and neutralises
`%`; the quotes around each argument stay. Where cmd's parser is ambiguous, the
documentation says fidelity stops.

### D35 — No userland required; `cash doctor` names the fix

What cash does not carry it finds on `PATH` (D8); `cash doctor` reports what shadows what
(BusyBox applets that break scripts, DOS `find` and `sort`, odd shims, the System32 tools
cash shadows) and names the fix, because shadowing on Windows is silent. Turned down:
depending on one userland install, since none is complete.

### D46 — App Execution Aliases run as any `.exe`

Store aliases (`python.exe` in `WindowsApps`) are 0-byte files tagged
`IO_REPARSE_TAG_APPEXECLINK`, which `CreateProcessW` resolves itself, so they need no
special support, only three rules: dispatch by extension before reading the file (D8),
`test -L` checks the reparse tag, not the attribute (D27), and `[ -s python.exe ]` is
false, documented in spec §4.

### D52 — MSYS2 and Cygwin programs get Cygwin's argument encoding

A program linked to `msys-2.0.dll` or `cygwin1.dll` (all of Git for Windows' `usr/bin`)
splits its command line by Cygwin's rules, so cash reads the import table and encodes for
those rules. Turned down: the C runtime's encoding for every program, which Cygwin mangles.

### Decided in working sessions

- **Times honour `TZ`, as in Bash** (the user, 2026-10-04): `printf '%(…)T'`, prompts and
  `HISTTIMEFORMAT` use the zone `TZ` names (`UTC`, a POSIX `EST5EDT`, an IANA name), else
  Windows' own. Turned down: documenting that `TZ` is ignored.
- **awk's `system()` and pipes and sed's `e` run plain cash** (the user, 2026-10-04):
  `cash -c`, as Bash does. Turned down: running them as `sh` in POSIX mode, as gawk and
  GNU sed run `/bin/sh`.
- **`exec -a NAME` and `exec -l` are refused for a program other than cash** (the user,
  2026-10-04): the standard library cannot set another program's `argv[0]`. Turned down:
  a spawn of cash's own to set it, and ignoring the option in silence.
- **A DLL is no command, though it is a PE image** (2026-10-18, the user's pick after
  1.24.0): an extensionless file runs by its contents, and a PE image counts only when
  its COFF header does not set `IMAGE_FILE_DLL`, so completion, `type` and `which` leave
  System32's `noise.dll` out. An `MZ` file too short for the header still counts.
  Turned down: the old rule, where any `MZ` file counted and running a DLL failed.

## 4. Processes, jobs and signals

### D6 — Every job in a job object, nested in the session's

Each pipeline and background job runs in its own job object inside a session job with
`KILL_ON_JOB_CLOSE`, so killing a job reaps its tree and nothing outlives cash; each
program is created suspended and assigned before it runs. A finished command's job is
released, not reaped, so what it left running goes on, as in Bash; at exit cash ends its
console programs and leaves GUI applications. The exceptions are elevated programs (D42)
and `detach` (D45). Turned down: assigning a program after it starts, which lets a
grandchild escape.

### D11 — cash owns job control

brush's job manager is off; `jobs`, `fg`, `bg`, `kill`, suspend and resume are cash's, in
Win32 terms, with the job object (D6) the one notion of a job. `trap` and `kill` take only
the signals Windows can deliver (D13, D19, D21, D22, D64); `USR1`, `PIPE`, `ALRM` and the
rest are refused, since a trap that can never fire fails silently. Turned down: keeping
brush's job manager beside cash's; accepting signals Windows cannot deliver.

### D13 — Ctrl-C is graceful first; the job object is the backstop

Ctrl-C during `terraform apply` must let Terraform release its state lock, so the first
move is the console event and the grace period is the user's, not a timer's. Foreground
programs share the console's group and get every Ctrl-C; background jobs lead groups of
their own. Under `fg` Ctrl-C reaches the job as Ctrl-Break, and a second one ends its
tree. Turned down: a fixed timeout; a group of its own for each foreground program, which
turns every Ctrl-C into Ctrl-Break and breaks REPLs; reaping first, which leaves remote
locks.

### D14 — `trap EXIT` runs to completion; a second Ctrl-C forces teardown

A cleanup cut short is worse than a slow one, so an `EXIT` trap has no timeout; a second
Ctrl-C while it runs tears down at once. Turned down: a timeout on traps.
Since 2026-10-16 the second Ctrl-C must come two seconds or more after the first: one
sooner is the same key pressed twice, and a quick double press at a long `grep` ended the
shell (`cash_win32::console::STUCK_AFTER`).

### D15 — Exit codes truncate; crashes map to Bash's 128+n

A normal exit code is its low byte, as in Bash; an NTSTATUS crash maps as Bash reports a
fatal signal (an access violation gives 139), and Ctrl-C gives 130. Turned down:
truncation alone, which turns `0xC0000100` into 0, a crash reported as success.

### D17 — Process substitution streams through pipes

`< <(cmd)` and `> >(cmd)` are anonymous pipes; one passed as a path is a named pipe; an
opener that must stat, seek or create gets a temp file instead (`diff`, `cmp`, a program
writing to `>(...)`). The shell waits for each `>(...)` before it exits. Turned down: temp
files for everything, which never stream (`< <(tail -f log)` hangs); a named pipe for
every path, which `tee -a` cannot append to.

### D19 — Suspend by enumerating threads, documented APIs only

`Ctrl-Z` and `kill -STOP` suspend each thread with `SuspendThread`; `fg`, `bg` and
`kill -CONT` resume. Windows has no Ctrl-Z event, so cash takes a Ctrl-Z the foreground
program leaves unread for 200 ms. Turned down: `NtSuspendProcess`, undocumented. Accepted:
a program busy over 200 ms between reads can be suspended where it would have read the
key.

### D21 — `kill -TERM` escalates in the background; `kill -9` is immediate

`TERM`, `HUP`, `QUIT` and `INT` ask the target and return at once (`WM_CLOSE`, or
Ctrl-Break to a group leader), then terminate it after five seconds; `kill -9` terminates
at once. Turned down: a console event aimed at a pid that leads no group, which reaches
every process on the console.

### D22 — Job specs kill trees; bare pids kill one process

`kill %1` signals the whole job through its job object; `kill 1234` signals that process
only, so `kill $pid` keeps POSIX meaning. cash holds a handle on recently ended job
processes so a reused pid is never hit. `kill -1` is refused. Turned down: signalling
whatever process has the number, which hits another program once Windows reuses it.

### D26 — Descriptors above 2 work where cash holds both ends

Builtins and cash-to-cash calls have the full table. A native program has no POSIX
descriptors, so `tool.exe 3>x` is an error, not a silent no-op; descriptors the shell only
holds are not passed and do not stop the command. Turned down: the MSVC CRT's
`STARTUPINFO` table, which Go and Rust programs never read; Bash's passing a group's fd 3
to a program.

### D64 — `CHLD` is cash's own event

Windows has no `SIGCHLD`, but cash reaps every child it starts, so a `CHLD` trap runs once
per child reaped, as Git Bash 5.3 counts. Turned down: refusing `trap … CHLD`, which job
pools and Bash 5.3's job notices need.

### D70 — Background jobs and subshells run inside the shell, on threads

cash forks nothing: a background job, a subshell and a command substitution are copies of
the shell on threads of their own. A job that runs no program gets a number no Windows pid
can be (4n + 1 from 100001), which `$!`, `kill`, `wait` and `jobs -p` take. At most 256
run at once, Git Bash's `ulimit -u` (`CASH_MAX_SUBSHELLS`). Turned down: tasks on the
runtime's workers, where a job that never waits holds a worker for good.
Added (2026-10-18, the user's pick after 1.24.0): a job's threads run below normal
priority, in turns of a millisecond between pipelines. At the foreground's priority,
forty busy jobs held it 11 to 19 s on one processor and past 15 s on CI's Windows
Server, where Linux hands a waking shell the processor at once. The programs a job
starts keep their priority. Turned down: lowering alone, where a job holding a lock the
foreground wanted kept it a second behind the other jobs' time slices.

### D71 — A writer whose reader went away ends as SIGPIPE ends it, if it is cash's

Windows has no SIGPIPE, so a builtin or bundled tool whose pipe reader is gone ends with
141 in silence, as in Bash; a program cash did not build ends as it chooses. Turned down:
letting the write merely fail, which kept `while :; do echo y; done | head -1` running for
ever.

### Decided in working sessions

- **`fg` and `bg` need job control, as in Bash** (the user, 2026-10-03): in a script they
  say `fg: no job control` and return 1 until `set -m`. Turned down: cash's earlier
  leniency, where `fg` in a script waited for the job.

## 5. Startup files, help and messages

### D24 — `~/.bashrc` is read, then `~/.cashrc`

An interactive cash reads `~/.bashrc` (`$HOME` is `%USERPROFILE%`) and then `~/.cashrc`,
so a Bash setup carries over and `.cashrc` has the last word. A failing command in a
startup file prints Bash's error, startup goes on, and a summary line ends it
(`3 errors in ~/.bashrc`), so a Linux-written `.bashrc` cannot fail silently.

### D69 — `cash --init-rc` writes a starter `~/.bashrc`, once

With neither `~/.bashrc` nor `~/.cashrc`, installs write a starter `~/.bashrc` (the
author's own), only the first time per user, so one deleted on purpose never returns; an
existing file is never overwritten. It is `.bashrc` so Git Bash reads it too.

### D72 — `help` from one catalogue

Every builtin has an entry and a summary in one catalogue, checked by a test; each page
ends with the builtin's own `--help` instead of copying its options.

### Decided in working sessions

- **What cash prints never cites its own documents** (the user, 2026-10-05): no decision
  numbers, spec rows, ROADMAP items or research files in help pages, `--help`, version
  lines, warnings or `cash doctor`; they point to a `help` page instead.
- **Errors are worded as Bash words them** (the user, 2026-10-03): `script.sh: line 3:
  foo: command not found` in a script, `cash: line 1: …` for `-c`, `cash: foo: command
  not found` interactively, with Bash's message text, builtins' included. Turned down:
  cash's words with Bash's location, and `error:` with a location added.
- **A builtin's bad option is Bash's two lines** (the user, 2026-10-03): `name: -q:
  invalid option` and `name: usage: <synopsis>`, status 2; `name --help` goes to stdout
  with status 2, as Bash's does, and `help name` gives 0. Turned down: clap's block.
- **sed's errors read as GNU sed's, prefix and all** (the user, 2026-10-04): `sed: -e
  expression #1, char 5: unknown command: `x'`. Turned down: cash's `<script argument
  1>:1:5: error:` before GNU's words.
- **Errors are coloured only on a terminal** (the user, 2026-10-03): the prefix is red when
  stderr is a terminal and `NO_COLOR` is unset; a pipe or a file gets plain text. Turned
  down: no colour at all.

## 6. The prompt, editing and completion

### D36 — Prompt commands get a job each, like every command

`PROMPT_COMMAND` and `PS1` substitutions are contained as every command is (D6). Turned
down: a pooled job for them; the per-job setup measured 0.18 ms against a 128 ms Starship
prompt.

### D37 — Starship works, and a test says so

The author runs Starship in every shell, so a correct Starship prompt is an acceptance
test, not a manual check; it needs `PROMPT_COMMAND`, the DEBUG trap, and Unix-spelled
command paths (D3).

### D39 — Terminal shell integration is on by default

cash emits prompt, command and exit-status marks and its folder (OSC 133, 9;9, 633, 7), so
Windows Terminal and VS Code get command blocks, jumps and new tabs in the same folder.

### D40 — Completion ignores case and quotes as it goes

Completion matches case-insensitively, completes `C:/Prog` and `/c/Prog`, and quotes names
with spaces or metacharacters in the word's own style, else single quotes, as PowerShell
does. Cobra-generated scripts (`docker`, `kubectl`, `gh`) complete through
bash-completion's helpers, defined for interactive shells only. Turned down: Bash's
unquoted completion, which breaks on `C:/Program Files`.

### D44 — History is appended at once, safely across tabs

Each command goes to `~/.cash_history` as it runs, safe when several tabs write one file
and when a kill skips unwinding. Turned down: Bash's write at exit, which loses a killed
session and lets the last tab win.

### D59 — Syntax highlighting is on, and never waits on the disk

It marks a missing command before Enter from a background listing of `PATH`; a network
path keeps the neutral colour. Turned down: probing `PATH` on each keystroke, 127 ms for a
missing name.

### D60 — `abbr`: fish's abbreviations

An abbreviation expands on the line, visibly, and the expansion is what runs and what
history keeps; they persist in `%APPDATA%\cash\abbreviations`. Turned down: turning aliases
into abbreviations; aliases stay as they are.

### D61 — `CASH_TRANSIENT_PS1` collapses the prompt after Enter

A variable, read like `PS1` and set in one `.bashrc` line. Turned down: a function hook.

### D62 — Folder history: `prevd`, `nextd`, `cdh`, Alt-← and Alt-→

fish's model: the last 25 folders left; Alt-← and Alt-→ walk them on an empty line and
move a word otherwise. Readline binds nothing to Alt-arrows, so nothing of Bash is lost.

### D63 — Completions from carapace, when it is installed

For a command with no `complete` spec, Tab asks carapace-bin if it is on `PATH` or beside
`cash.exe`, and falls back to cash's own after three seconds. Turned down: bundling it: Go,
not linkable, and 90 MB.

### D68 — The console is put back before each prompt

A program that leaves the console wrong (k3d left VT input on, and Enter stopped working)
no longer breaks the tab: before every prompt cash restores the console modes, input
settings and UTF-8 code page, and after a program ran resets the terminal, but not
colours or the palette.

### D73 — `croot`: a file and folder picker on Alt-E

Built in and drawn inline below the command line from one folder read, so the first frame
comes within 30 ms; broot's tree, `ls`'s colours, a fuzzy filter, and the command line
deciding folders or files. Keyboard only. Turned down: broot on Alt-E, a separate program
up to two seconds to open; a search language, previews, file operations and mouse input.

### D79 — Ctrl-R: a history picker

Ctrl-R opens the history in D73's picker: each command once, newest first, fuzzy-filtered;
Enter puts the command on the line, Tab runs it, and Ctrl-R again limits it to this
folder's history. Bash's reverse search stays a Readline function `bind` can put back.
Turned down: deleting entries, previews, statistics and sync.

### D80 — `z`: the folder jump

Every change of folder counts a visit; ranking and ageing are zoxide's. A folder that is
gone is forgotten only after 90 days without a visit, so an unplugged drive keeps its
folders. A `z` function, zoxide's included, comes before the builtin.

### Decided in working sessions

- **The first Tab inserts the shared part; the second opens the grid on the hinted
  candidate** (the user, 2026-10-02). Turned down: Bash's beep on the second Tab and list
  on the third; one Tab doing both; a grid opening on its first candidate.
- **Tab globs a typed `*` and `?`, and completes everything else as typed** (the user,
  2026-10-03): no Windows name holds `*` or `?`, so `**/nee` completes and so does
  `[draft]`. Turned down: completing all of it literally, as Bash does, and leaving every
  glob character a glob.
- **History is not cut while `HISTSIZE` and `HISTFILESIZE` are unset** (the user,
  2026-10-03): every line is kept, where Bash cuts at 500 (spec §4 row 43). Turned down:
  Bash's default, which would cut an existing long `~/.cash_history`.
- **The prompt names cash** (the user, 2026-10-03): `\v` and `\V` give cash's version and
  `\s` gives `cash`; `$BASH_VERSION` stays the Bash interface version (spec §4 row 44).
  Turned down: `$BASH_VERSION`'s numbers in the prompt.
- **`complete -A service` and `-o bashdefault` work as Bash's** (the user, 2026-10-11):
  `-A service` gives the network service names of Windows' own `drivers\etc\services`
  (`ssh`, `http`), as Bash's gives `/etc/services`', and `-o bashdefault` falls back to
  Bash's default candidates when a spec gives nothing. Turned down: refusing both;
  Windows' system services for `-A service`, which a Bash completion script does not
  expect.
- **`bind -m vi-insert` and `-m vi-command` say they are not supported** (the user,
  2026-10-11), status 1, where they were ignored in silence; cash's vi mode keeps its own
  keys. Turned down: vi keymaps of their own, a large change for few users.
- **A prompt element naming the shell is the prompt's, not cash's** (2026-10-11): cash sets
  `$SHELL` (D5), which a Starship module can show, so a PowerShell window is never taken
  for cash.

## 7. Builtins and bundled tools

### D45 — cash's own builtins only where a decision or a Windows gap asks

`winpath` (D4), `detach`, `elevate` (D42), `sudo`, `su`, `start`, `abbr`, `prevd`,
`nextd`, `cdh`, `croot` and the like. `detach` needs every job to allow breakaway, a
weakening of D6 accepted as the price of an explicit escape.

### D48 — coreutils are bundled as builtins

uutils coreutils and cash's `sed` and `awk` are builtins, so the shell works with an empty
`PATH`, loops are faster, and BusyBox or DOS shadowing stops mattering. Builtins come
first; `enable -n NAME` reaches the `PATH` copy. `ps`, `pgrep`, `top` and `tree` are cash's
own, on the native pids `kill` takes. Turned down: requiring a userland on `PATH`; MSYS
`ps`, whose pids `kill` cannot use; rewriting the paths tools echo.

### D49 — Bundled `sed` and `awk` keep CRLF files CRLF; `CASH_EOL=lf` is Linux

A CRLF line is matched and split without its CR and written back with it, so `s/foo$/bar/`
works on Windows files and line endings never change as a side effect. A CR the program
names is data, as in every run under `CASH_EOL=lf` and with `sed -b`. Converting is
`dos2unix`'s and `unix2dos`'s job.

### D50 — `fuser` and `lsof` from documented sources, plus the handle walk

Who holds a file comes from the Restart Manager, ports from IP Helper's tables, and what a
process holds from the system handle list, undocumented but stable since Windows XP. What
Windows cannot tell shows as `-`; options with no Windows answer are refused. Turned down:
leaving the handle walk out, which left "what does this process have open" unanswered.

### D51 — `ss` is iproute2's over the Windows socket tables

Linux scripts use `ss -ltn`; `netstat.exe` has other flags and layout. cash's `ss` gives
iproute2's layout, options, filters and statuses over the owner-PID tables, shows nothing
where Windows has no data (Recv-Q and Send-Q are 0), and refuses options with nothing
behind them. `netstat.exe` is not shadowed; `ss -ano` gets a hint.

### D54 — `pkill`, `pidof`, `killall`: one set of name rules, `kill`'s signals

procps-ng's and psmisc's output and statuses; names match without case and with `.exe`
optional, as Windows image names do. They never signal the shell, pids 0 and 4, critical
images (`csrss`, `lsass`, …) or service accounts' processes.

### D55 — `getopt`, `rev`, `clear` and `reset` are util-linux's and ncurses'

`getopt` shadows Git's MSYS `getopt.exe`; `reset` shadows `System32\reset.exe` (still
reached by its path). `clear` and `reset` write `xterm-256color`'s sequences without
terminfo, since ConPTY speaks VT.

### D56 — `bc` is POSIX bc and names GNU's extensions

posixutils-rs' bc; a GNU extension fails with its name, so a GNU script fails with a
reason. After an error `bc` exits 1 where GNU's exits 0, so `set -e` catches it. Turned
down: GNU bc's extensions, but for comparisons as expressions (below, 2026-10-17); `dc`.

### D57 — `ping` takes Linux's flags and shadows `ping.exe`

`ping -c 1 host && …` is how scripts test a host, and `ping.exe` reads `-c` as something
else. cash's `ping` has iputils' flags and statuses over ICMP echo APIs, since Windows gives
no raw socket; `ping -n 3` gets a hint, and `ping.exe` stays reachable by its path.

### D58 — `which` prints a path that runs, for the commands cash carries

`LS=$(which ls); "$LS"` must work, and a builtin has no file, so `which` prints a virtual
path (`C:/…/cash.exe/ls`) that cash runs as the builtin, or a D65 link where there is one.
Turned down: launcher executables kept in step with cash; printing a command line, which a
quoted `"$LS"` cannot run.

### D66 — `where` is `where.exe` with dashes and cash's paths

`where.exe`'s search, matching and statuses; options take dashes, never `/q`, so a path is
never taken for an option; paths print as `pwd` prints them. Turned down: slash options
and backslashed paths.

### D67 — `ls` icons only with a Nerd Font

Icons come with `--icons`, and under the default `--icons-theme=auto` only when the
Terminal profile uses a Nerd Font; colours follow `LS_COLORS`. Hidden-and-system entries
are left out without `-a`, as Explorer does. Turned down: lsd's colour-emoji icons.

### D74 — Small tools: only what nothing else answers

A tool is added when nothing answers its name on a bare Windows with cash and it must
agree with something cash owns, or is cheap and collides with nothing; no System32 name is
taken. Hence `tput`, `stty`, `iconv`, `pbcopy`, `pbpaste`, `flock`, `nice`, `watch`,
`xxd`, `nc` and the like, each tested against its Linux original. `nice` never sets
REALTIME. Turned down: `clip.exe`, which writes the console code page and cannot paste;
`patch`; `strings`, a name Sysinternals' tool holds.

### D76 — `grep`, `diff` and `cmp` are carried after all

They are the commonest failures on a bare machine, so under D75's aim cash carries them:
`grep` with GNU's interface on ripgrep's library crates, `diff` and `cmp` from uutils
diffutils. Turned down: leaving them to Git Bash, a rule made for a cash beside Git Bash.

### Decided in working sessions

- **Bundled tools stay processes of their own** (the user, 2026-10-04): a call costs a
  process start, 25 to 35 ms in a debug build, but a tool that crashes or exits cannot take
  the shell with it. Turned down: running them inside the shell's process.
  Confirmed (the user, 2026-10-16, `research/in-process-tools-design.md`): uutils cannot
  run inside cash (uucore's process-wide state, its exits and console handlers), and even
  a native tool loses Ctrl-C, `&`, kill, nice and crash isolation. Only `basename` and
  `dirname` move inside, rewritten as cash's own, as BusyBox runs them: pure string work
  that cannot hang. Turned down: native rewrites of the filters (`cat`, `cut`, `tr`, `wc`,
  `head`), and a warm helper process that runs bundled tools without starting one.
- **awk and sed are imported once as cash's own** (2026-09-28): awk is posixutils-rs' AWK,
  sed is uutils' sed; gawk's and GNU sed's extensions are outside the gate. Turned down:
  tracking forks, git dependencies, and posixutils' sed.
- **`time` and `times` report real CPU time** (the user, 2026-10-02). Turned down:
  documenting zeros as a divergence.
- **Uptime is Windows' figure** (the user, 2026-10-04): `coolfetch`, `uptime` and `top`
  count from the last full kernel boot (`GetTickCount64`), as Task Manager does, so with
  Fast Startup a power-on resumes the count. Turned down: counting from the last power-on,
  or showing both.
- **`uname -s` stays `Windows_NT`**. Turned down: claiming MINGW, as Git Bash does.
- **An elevated `ss -i` switches TCP statistics collection on** (the user, 2026-10-05):
  Windows keeps per-connection statistics only for connections an administrator switched
  them on for. Unelevated, `-i` prints the MSS with a note pointing to `sudo ss -i`;
  elevated, it switches collection on for the listed connections and prints the rest.
- **`xargs -P N` runs N commands at once** (the user, 2026-10-11), as GNU xargs does.
  Turned down: refusing N above 1.
- **`declare -t` stays, as Bash's** (2026-10-11): a traced function inherits the DEBUG
  and RETURN traps, which cash already does (`declare -p` shows no `t`, as in Bash).
  Refusing it was chosen the same day on a wrong report that it did nothing, built, and
  taken back when a test of the RETURN trap failed.
- **`logger` writes to the Application event log** (the user, 2026-10-11), under the
  source `cash`, `-p` mapped to error, warning or information. Event Viewer shows each
  entry with a "description not found" note, since registering a source needs admin.
  Turned down: not carrying it.
- **`flock`'s help says the lock lasts until the command exits** (the user, 2026-10-11),
  not until its detached children do, which no tool can see. Turned down: leaving it
  unsaid, as util-linux's manual does.
- **A builtin's output is gathered into larger writes, and a builtin that streams says
  so** (the user, 2026-10-16; `cash_core::output`): in blocks of up to 64 KiB, as C's
  standard output into a pipe or a file, but never held more than 50 ms, on a terminal
  or not: a watching thread writes it when the builtin writes nothing more, so a builtin
  that hangs still shows what it wrote. It is written before the builtin's standard
  error, a read of its input, a program or command it runs, and when it returns. `top`, `watch`, `nc`, `free`, the pagers and
  `croot` stream; a test lists them. `stdbuf -oL` and `-o0` make any builtin stream.
  Turned down: each builtin buffering on its own (opting in), which `ls` did first.
- **A folder's link count is 1 in `ls -l`, counted in `stat`** (the user, 2026-10-16):
  1 is Git Bash's figure and the "not counted" of Cygwin, btrfs and CIFS; counting means
  listing every folder shown, which made `ls -la` in TEMP list 164,757 folders for `..`.
  `stat`, where one goes to find the number, counts 2 plus the subfolders, as Linux has
  it, and `ls --count-links` counts in a listing too. Turned down: 2 for every folder.
- **A file's link count and identity are asked by name** (the user, 2026-10-16):
  `GetFileInformationByName` (Windows 11 24H2) gives them without opening the file; an
  open is the fallback, also where Windows leaves the volume's serial number out.
- **Facts about the session live in one place** (the user, 2026-10-16;
  `cash_win32::facts`): the account, the machine's name, account names by SID, and a
  Terminal profile's font, each with its own rule for going stale. Turned down: a cache
  beside each caller.
- **Builtins share their option parsing, input opening and folder walks** (the user,
  2026-10-16; `research/central-components-design.md`):
  - **Options:** cash-getopt, finished with `POSIXLY_CORRECT`, a hook for `-9` and `-10`
    words, and one usage-error line with each upstream's style. A repeated option
    overrides in every clap builtin. xargs, ls, stat and pgrep move off clap for GNU's
    messages and abbreviations.
  - **Inputs:** `ExecutionContext::open_input`, which gives builtins the `/dev` names,
    "Is a directory" and a read that fails at a stop.
  - **Walks:** walkdir with a cash layer: one answer to what a link is (a symlink or
    junction), loop detection, the stop token, and every error left to the tool.
  - **Order:** options, then inputs, then walks. The archivers move onto the walker last.
  - Each tool keeps its upstream's messages and statuses, and moves one at a time against
    its tests and Git Bash.
  - Turned down: a walker of cash's own; patching the archivers' own walks; keeping
    clap's messages.
- **bc takes a comparison as an expression** (the user, 2026-10-17): `echo "1.5 > 1.2" |
  bc` prints 1, as in GNU bc and posixutils-rs 0.9.1, the one GNU extension in D56's
  POSIX bc. Turned down: refusing it with its name, as the other extensions are.
- **awk follows gawk as far as generic awk needs** (the user, 2026-10-17: "no need to go
  too deep in replicating gawk as long as generic awk works"): an upstream fix or a gawk
  difference is taken where ordinary programs meet it. Turned down: gawk's corners, such
  as bytes as characters in the C locale and RS matches of the empty string.
- **ls takes GNU's remaining options** (the user, 2026-10-17): the times and sizes
  (`-c`, `-u`, `--time`, `-s`, `-k`, `--si`, `--block-size`), name quoting (`-Q`,
  `--quoting-style`, `-b`, `-q`, `-N`), the script helpers (`--zero`, `-i`, `-H`, `-T`,
  `--hyperlink`) and the poor fits for Windows (`-n`, `-Z`, `--author`, `-D`), so copied
  scripts run. Turned down: leaving any group refused.
- **`tail -f` ends when the reader of its output is gone** (the user, 2026-10-17): tried
  with the pipe's state between polls, as GNU tail does on Linux. Turned down: leaving
  it as Git Bash's tail has it, waiting for the file's next line.
- **ls quotes names on a terminal as GNU ls does** (the user, 2026-10-17): shell-escape
  (`'a b'`, `"it's"`, `'c'$'\177''d'`) and `?` for a control character left as it is,
  as GNU coreutils does since 8.25; into a pipe or a file, names as they are. Turned
  down: names as they are on a terminal too, as cash's ls showed them.
- **`ls --hyperlink` links to `file:///C:/…`, with no host** (2026-10-17): GNU ls writes
  `file://HOSTNAME/path`, and Windows Terminal opens a `file:` link only when its host
  is empty, `localhost` or WSL's (`TerminalPage::_IsUriSupported`: ShellExecute cannot
  open the host form), so GNU's form would get Terminal's "could not open" dialog. The
  drive's colon stays as it is, as RFC 8089 writes a Windows path; a share is
  `file://server/share/…`. `-T` is checked as GNU checks it and changes nothing: cash
  lines columns up with spaces, never tabs.
- **The phase after 1.22.0** (the user, 2026-10-17): `timeout` and cash, the clipboard
  on a locked workstation, `ls -l`'s read moving the access time, `stat %b` from the
  allocation, the shebang interpreter looked up twice, and the slow PATH search of
  `type -a`, `which -a` and `hash NAME`. Turned down for now: the archiver that hung
  after a quick `kill` in CI, which stays in TODO.md.
- **The phase after 1.23.0** (the user, 2026-10-17): the TERM trap that never runs on
  cash's own `kill -TERM`, the background jobs that hung after a quick `kill` in CI, and
  four costs paid on each call: the completion menu's PATH scan on every keystroke,
  `which <builtin>`'s walk of PATH, the search for pwsh on each `.ps1` run, and
  `erasedups`' scan of the whole history. The user published 1.23.0 to Scoop and asked
  for a benchmark against the last big one and the first one, step by step.
- **A TERM trap from another process is a limitation, not a fix** (the user, 2026-10-18):
  a cash script that another process sends `TERM`, `HUP`, `INT` or `QUIT` ends at once
  with 128 + N, its `TERM` and `EXIT` traps not run; documented in spec.md §4 #31,
  `help differences` and `help trap`. Ctrl-C and `kill -TERM $$` run the traps. Turned
  down: a channel of cash's own (a named event per cash process) to carry signals
  between cash processes.
- **`ls -la`'s 20% at 1.23.0 is won back first** (the user, 2026-10-17, after the
  benchmark, `research/benchmarks-1.23.0.md`): the file facts are taken from the folder
  listing (one directory query) instead of a call for each file; and for keeping the
  access time while reading for `#!`, a cheaper way is looked for before it is kept.
  Turned down: leaving the 7% of the per-file facts; reverting the access time unseen.
- **The phase after 1.24.0** (the user, 2026-10-18): the DLLs completion offered as
  commands. The user published 1.24.0 to Scoop.
- **The phase after 1.25.0** (the user, 2026-10-18): the busy background jobs that
  starved the foreground, and growing the differential corpus; the first was taken into
  1.25.0 when 1.24.0's release gate timed out on it. 1.25.0 is tagged and taken to
  Scoop once CI is green, as 1.24.0 was. Turned down for now: taking lzma-rust2 in for
  7z's and xz's speed and a prompt stop, zstd's real levels, the winget submission, and
  the four leftovers.
- **The phase after 1.26.0** (the user, 2026-10-18): the corpus's three open findings,
  aliases with operators, extglob on from the start, and `GLOBIGNORE` in `shopt
  dotglob`, with the clipboard test that puts back only text, and more corpus rounds.
  1.26.0 is tagged and taken to Scoop once CI is green. Still turned down for now:
  lzma-rust2, zstd's levels, the winget submission, the four leftovers.

## 8. Archives and compression

### D77 — The compatibility corners follow Bash and fish

`/dev/tcp` and `/dev/udp` as Bash has them; `command_not_found_handle` as Bash runs it,
with D75's install hint only when none is defined; abbreviations kept across sessions as
fish keeps them; `getconf` and `locale` with Windows' values; and refusals that say why for
`suspend`, `mkfifo` and `stdbuf`.

### D78 — Archive and compression tools, on shared parts

`gzip`, `bzip2`, `xz`, `zstd`, `tar` (GNU tar 1.35) and `zip`, `unzip`, `zipinfo`
(Info-ZIP) each keep their original's interface, messages and statuses, tested against it
in WSL; `tar` shadows Windows' `tar.exe`, still reached by its path. Two crates that know
no shell hold the shared parts (`cash-archive`, `cash-getopt`). Every codec is pure Rust.
Not wanted: the z-tools, `cpio`, `lzip`. Turned down: uutils/tar; `xz2` and `liblzma`,
which build C; the `tar` and `zip` crates' headers and records; `libzstd-rs-sys` until it
has a Rust API.

### Decided in working sessions

- **7z's archives are as small as 7-Zip's before they are fast** (the user, 2026-10-11):
  LZMA2 keeps 7-Zip's block of four dictionaries by default; `c=` trades size for speed
  for whoever asks. Turned down: a block per dictionary across the threads by default,
  which compressed a 155 MB tar in 12 s instead of 47 s but 2–4% larger.
- **`7z` and `7za` are builtins that shadow 7-Zip** (the user, 2026-10-07): 7-Zip 26.03's
  interface, dash switches only, on sevenz-rust2 made cash's own.
- **7z shows a time by its own date's offset** (the user, 2026-10-11): 7-Zip applies the
  offset of the day it runs, so a time across daylight saving reads an hour off. Turned
  down: following 7-Zip.
- **`rar` and `unrar` are learned by observation only** (the user, 2026-10-07 and
  2026-10-08): from running WinRAR 7.23's tools and reading `Rar.txt`, never from UnRAR's
  source, whose licence forbids it, nor from 7-Zip's unRAR-restricted code. RAR 5 only
  (`-ma4` refused); solid order by extension, honouring `rarfiles.lst`.
- **A damaged archive needs a faulty status, not the oracle's exact words** (the user,
  2026-10-10): the same failure, or the same repair, is enough.
- **zip's phase-30 choices stand** (the user, 2026-10-11): deflate on `miniz_oxide`; the
  RID as the numeric owner, as `id`, `stat` and tar give it; a zip made on Unix, so Linux's
  UnZip restores the modes. Turned down: another deflate engine, "made on NTFS", and no
  owner field.
- **zip stores what deflate cannot shrink, at any size** (the user, 2026-10-11): Info-ZIP's
  zip 3.0 keeps a member past deflate's first block (about 32 KB) deflated though it
  grows. Turned down: following zip 3.0.
- **unzip reads more than Info-ZIP's** (the user, 2026-10-07): WinZip AES, PPMd, PKZIP 1's
  methods and split archives.
- **What the archive tools leave out** (2026-10-07): zip refuses `-R`, `-U`, `-A`, `-J`,
  `-DF`, `-sp`, `-AC`/`-AS` and the logs; unzip skips tokenized, PKWARE DCL, Terse, LZ77
  and WavPack members. Each can be added when someone needs it.
- **zstd compresses on every core by default** (the user, 2026-10-07).
- **7z's speed and xz's instant stop are parked** (the user, 2026-10-17): both need
  lzma-rust2 taken in as cash's own (a match-finder thread as 7-Zip's, a stop between
  pieces of a block), and neither is worth that now: 7z is half 7-Zip's speed at the same
  size, and a stopped `xz` of a large file ends within about 12 s, losing nothing.

## 9. sudo and elevation

### D42 — Elevated programs are outside D6

An elevated program is not cash's child and cannot join its job object, so it outlives
cash; `elevate` says so before it asks UAC (`-q` drops the warning). Turned down: refusing
elevation; ending the program by pid at exit, which cannot cross the boundary.

### Decided in working sessions

- **`sudo` elevates cash itself, in this terminal** (the user, 2026-10-10): UAC starts an
  elevated cash that takes the caller's terminal and standard handles, so keys, Ctrl-C,
  redirections and the status work, and the command reads the terminal. cash chooses what
  runs, as the shell would (`sudo ls` is cash's `ls`). `sudo -n` fails unless already
  elevated. Turned down: gsudo; Windows' own `sudo`, which found WSL's `bash.exe` for
  `sudo bash`; UAC's new window.
- **`sudo -u USER` and `su USER` are cash's own too** (the user, 2026-10-11): cash asks
  USER's password and starts as USER with `CreateProcessWithLogonW`, relaying the console,
  in the system temp folder when USER cannot enter the shell's. `-v`, `-k` and `-K` are
  accepted with no credentials cache behind them. Turned down: gsudo and `runas`.
- **Files an elevated `sudo` makes are yours, and an approver who is not you is named**
  (the user, 2026-10-04): approved with your own account, your account owns what the
  command creates; approved by another account, cash says `running as ADMIN, not you`.
  Turned down: Windows' default (`Administrators` owns them), and a system-wide policy.
- **`elevate` stays, beside `sudo` and `su`** (the user, 2026-10-04): it starts a program
  elevated in a new window without waiting. Turned down: removing it.
- **Doctor's fix for a cash other accounts cannot read is a grant, not a reinstall** (the
  user, 2026-10-12): a cash under the profile (Scoop's, the installer's) is readable by
  you and administrators only, so `su USER` and `sudo -u USER` cannot start it. Doctor
  suggests `icacls` granting Users read and execute on the folder that holds every
  version, inherited, so each update keeps it: no administrator needed. Tried with a
  standard test account: refused before, run after, and a version folder made later
  inherits it. Turned down: a machine-wide `scoop install -g`, which needs an
  administrator and moves cash out of the profile.

## 10. Packaging, installing and updating

### D38 — Shipped through Scoop, an installer, winget and a zip, with a Terminal profile

cash ships from the `tomcoolpxl/scoop-bucket` bucket, as a per-user Inno Setup installer
with winget made from it (D75), and as a portable zip; nothing needs admin. Every install
adds a Windows Terminal profile fragment, edits only cash's own entry in `settings.json`,
and names a Nerd Font the machine has, installing none. Turned down: a portable winget
package, which cannot upgrade a running `cash.exe`.

### D65 — `cash --link-tools`: hard links programs outside cash can run

Virtual paths (D58) run only inside cash, so hard links to `cash.exe`, one per carried
tool, let any program run them; a hard link costs no space or admin and keeps the hash
AppLocker checks. `PATH` changes only on request (`--add-to-path`), kept `REG_EXPAND_SZ`.
Turned down: printing a PowerShell line, which rewrote the user `Path` as `REG_SZ`.

### D75 — A per-user installer, `cash --update`, and winget

cash installs as a complete Bash with its tools, without Scoop: a per-user Inno Setup
installer (unsigned), winget made from it, and a portable zip share one tested finishing
step. Each version has its own folder behind a `current` junction, so an upgrade never
overwrites a running `cash.exe`; tool links go on the user `PATH` by default; uninstalling
keeps the rc file, history and config. `cash --update` upgrades only on request. A
portable cash offers once to set itself up and changes nothing without a yes. At an
interactive prompt a missing command names its `winget` and `scoop` packages. Turned down:
Velopack and a per-user MSI; background polling and silent self-replacement, since a shell
that changes overnight surprises script authors; a paid certificate or SignPath.

### Decided in working sessions

- **Scoop updates cash while it runs** (the user, 2026-10-02): the user's Scoop ignores
  running processes, so `scoop update cash` goes through from within cash and running
  shells keep their version; other users get the manifest's note.
- **The release zip's name never changes**: `cash --update` and Scoop depend on it.
- **The blank first Terminal tab after an update stays** (the user, 2026-10-06): it is
  Windows Terminal's first launch of a rewritten fragment, not cash's start.

## 11. Building, testing and releasing

- **Release builds unwind panics**, so an unexpected panic returns to the prompt instead
  of ending the shell. Turned down: `panic = "abort"`.
- **A release stops when cash grows** (2026-10-10): the shipped `dist` build must stay
  within the last release's size plus 2 MiB and a 60 ms start; raising the budget is a
  commit of its own that says why.
- **The MSRV is the pinned toolchain's minor** (the user, 2026-10-04): the only Rust CI
  builds with is the only one promised; it rises with `rust-toolchain.toml`. Turned down:
  a lower floor with a CI job of its own, and no `rust-version` at all.
- **The git-prompt goldens are frozen against the installed Git for Windows** (the user,
  2026-10-04): the test fails and says to re-freeze when the scripts' hashes change.
  Turned down: vendoring the GPL-2.0 `git-prompt.sh`, and a by-hand differential.
  Changed (the user, 2026-10-16): when the hashes changed, the cases run in the installed
  Git Bash and cash is held to that, with a note to re-freeze; CI's runners updated Git
  without notice and the build stayed red until someone had the same Git. Turned down:
  updating the local Git to the runners' and re-freezing, and re-freezing on CI.
- **The upstreams cash took in are watched weekly** (the user, 2026-10-16):
  `upstreams.toml` lists each with what it was taken from, `cargo xtask upstreams` shows
  upstream's releases, commits since and RustSec advisories, and the weekly Upstreams
  workflow runs it with `cargo deny check advisories` and keeps one `upstreams` issue.
  Turned down: the command alone, and a TODO line looked at by hand.
- **Dependabot's patch and minor updates merge themselves once CI is green** (the user,
  2026-10-16); majors wait for a person. Turned down: majors too, and merging by hand,
  which left an Actions update unmerged for nine days.
- **Timing-sensitive tests run on an idle machine, and failures under load are never
  fixed** (the user, 2026-10-02): such a test is run again with the CPU free and judged
  from that; no timeout is lengthened and no test rewritten for load.
  Exception (the user, 2026-10-17): uutils' sed tests get a 30 s period in every profile,
  where one CI run's starved runner killed one at 45 s. They only start processes and
  cannot hang, so the longer allowance hides nothing, and a test that finishes takes the
  same time.
- **The corpus copies only from sources whose terms allow it** (2026-10-18): each
  source and its terms are in `tests/corpus/SOURCES.md`, with the notices they ask to
  be kept (pure-bash-bible's MIT, Lucent's for the One True Awk). Greg's Wiki states no
  licence, so its snippets stay as they are and no more are taken; the Advanced
  Bash-Scripting Guide's licence restricts modified copies, so none are taken from it.
