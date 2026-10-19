# Done

What TODO.md held until it was done, as it was written down there: the history of the
first phases, every finished phase from 16 on, and the findings that were fixed or
settled. New entries are added at the end of their part when they are done.

## The first phases and the releases

Releases so far: 1.3.0 is what was on `main` on 2026-09-30, 1.3.1 the test suite (phase
1), 1.3.2 Ctrl-C everywhere (phase 2), 1.3.3 process substitution (phase 3), 1.3.4 the
`/dev` names and the bundled tools (phase 4), 1.3.5 jobs, `wait` and the tools beside
them (phase 5), 1.3.6 the Tab menu opening on the history hint's candidate (asked by the
user on 2026-10-02, outside the phases), 1.3.7 the shared part on the first Tab and the
grid on the second (phase 7), 1.3.8 crashes, security and CI hardening (phase 8), 1.3.9
the shell's own working directory, environment, PATHEXT and umask (phase 9), 1.3.10 awk,
sed and bc (phase 10), 1.3.11 pipelines, jobs and process substitution (phase 11), with
the two parser bugs phase 11 found (`( ( … ) )` and `--file=<(…)`), fixed while CI was
put right, 1.3.12 the language (phase 12), 1.3.13 builtins and error output (phase 13).

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
| 20 | 1.4.0 | `croot`: a built-in file and directory picker on Alt-E |
| 21 | 1.5.0 | Console, clipboard and the remaining small tools |
| 22 | 1.6.0 | A per-user installer, `cash --update`, and winget |
| 23 | 1.7.0 | `grep`, `diff` and `cmp`: the standing rule reversed |
| 24 | 1.8.0 | `gzip`, `gunzip` and `zcat` |
| 25 | 1.8.0 | Compatibility corners: `/dev/tcp`, `command_not_found_handle`, persistent `abbr`, kinder refusals, `getconf`, `locale` |
| 26 | 1.9.0 | `cash-getopt`, and every option parser on it |
| 27 | 1.9.0 | One Unix face for a Windows file |
| 28 | 1.9.0 | `cash-archive`'s codecs, the compressor driver, `bzip2`, `xz`, `zstd` |
| 29 | 1.9.0 | `tar` |
| 30 | 1.9.0 | `zip`, `unzip`, `zipinfo` |
| 31 | 1.10.0 | `7z` and `7za` |
| 32 | 1.10.0 | Every core, where the format allows it |
| 33 | 1.11.0 | RAR: `7z` reads it, `rar` and `unrar` builtins |
| 34 | 1.12.0 | The prompt, paths, and a guard on size and speed |

---

## Phase 16. sudo, su and `cash help`

Found writing the help catalogue (2026-10-04); all fixed, released as 1.3.17.

---

## Phase 17. `lsof` lists everything

Chosen by the user on 2026-10-04, after 1.3.17:

Bare `lsof` and the open files of `-p`/`-c`/`-u` come from the handle walk
(`cash_win32::handles`, spec D50) since 2026-10-05, and so do `+D`, `+d` and a directory
named: `lsof +D /tmp` over `TEMP`'s 87,411 directories took over two minutes asking the
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

## Phase 20. `croot`: a built-in file and directory picker on Alt-E

The user used broot on Alt-E for two weeks (set up 2026-09-27 in `~/.bashrc` and broot's
`cash-cd.hjson`) and wants it built in, as a picker only: no search language, no
editor integration, no verbs. Broot takes up to two seconds to open, being a separate
program; built in, the first frame comes from one directory read. Chosen on 2026-10-05:

- **Inline, below the prompt** (as fzf's `--height`): 40% of the window, at least 8
  rows, `CASH_PICKER_HEIGHT=50%` or `=15`; the scrollback above stays visible, the
  screen scrolls up when the prompt is near the bottom, a tiny window gets full screen.
- **Broot's tree**: several levels open, trimmed to fit with `… N more`; Right makes the
  selected directory the root, Left goes to the parent; at a drive's root, Left shows the
  drives (C:, D:, mapped drives, `~`).
- **Typing is a fuzzy filter that also searches deeper**, in the background, best
  matches first, capped in time and entries; Esc clears it, then closes.
- **Context from the command line**:
  - directories only for an empty line, `cd`, `pushd`, `rmdir`, `mkdir -p`; directories and
    files for any other command; Alt-F switches;
  - the word at the cursor, when it is part of a path, sets the start (`cd ~/src/fo`
    starts in `~/src` filtered by `fo`) and is replaced by the pick;
  - an empty line, `cd` or `pushd`: the pick goes in and runs at once; any other
    command: the pick is inserted with a space after it and nothing runs;
  - a command that takes several paths (`cp`, `mv`, `diff`, …) keeps the picker open
    for the next argument; one known to take one closes it; unknown commands stay open.
    Esc, or picking with Ctrl-Enter, closes.
- **Path form**: relative below the current directory, `~/…` under home, else `C:/…`;
  quoted only when needed; directories end in `/`.
- **Extras in v1**: drives above `C:/`; a directory-history view (Alt-H) from cash's own
  directory history (`cdh`); hidden entries (dotfiles, the hidden attribute) and
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

After the user's first use: 1.4.1 lets Down and PgDn scroll through a long directory,
1.4.2 shows each pick on the command line while picking, 1.4.3 stops the flicker while a
search runs. Then (2026-10-06): the first frame measured at a median of 17-19 ms
(`tests/croot_latency.rs`, spec D73), a resize works the height out again, and Alt-S
sorts newest first with each entry's age. Nothing left open.

---

## Phase 21. Console, clipboard and the remaining small tools

Asked by the user on 2026-10-06, after a second look at what a bare Windows machine with
cash still lacks (`research/busybox-gap-analysis.md` has the first look). Every name
below resolves to nothing on a clean Windows install; several resolve to Git for Windows'
`usr/bin` or a Scoop shim when those are on `PATH`, and those stay reachable by path,
as `type -a` shows. No Windows program in System32 has any of these names, so none of
them hides one. The tools, in the order they are built:

1. **`tput`** (ncurses 6.6's for `xterm-256color`): the string, numeric and boolean
   capabilities scripts use (`setaf`, `setab`, `bold`, `sgr0`, `smul`, `rev`, `cup`,
   `civis`, `cnorm`, `sc`, `rc`, `el`, `ed`, `clear`, `smcup`, `rmcup`, `cols`,
   `lines`, `colors`, …), written as VT sequences without terminfo, as `clear` and
   `reset` already are; `cols` and `lines` from the console, also when standard output
   is a pipe; `-S` from standard input; `-T` accepts xterm- and VT-like names only;
   ncurses' exit codes (an unknown capability is 4, a false boolean 1).
2. **`stty`** (GNU coreutils 9.11's words): `-echo`/`echo`, `raw`/`cooked`/`sane`,
   `-icanon`, `isig`, `size`, `-a`, `-g` and restoring from its string, `rows`/`cols`;
   each setting mapped to the console modes cash owns (echo, line input, processed
   input); settings Windows has no counterpart for (`erase`, `intr`, `ixon`, speeds)
   are accepted so scripts run; `-F FILE` refused. Cash puts the console back before each
   prompt, so a change lasts for the current command or script.
3. **`iconv`** (glibc's options): `-f`, `-t`, `-l`, `-c`, `-s`, `-o`, `//TRANSLIT` and
   `//IGNORE`; every code page Windows has (`CP437`, `CP850`, `CP1252`, `LATIN1`,
   `ISO-8859-*`, `KOI8-R`, `SHIFT_JIS`, `GBK`, `BIG5`, `EUC-KR`, …) through
   `MultiByteToWideChar`, plus UTF-8, UTF-16LE/BE and UTF-32LE/BE with BOM handling;
   glibc's "illegal input sequence at position N" and status 1; the default charset
   is UTF-8.
4. **`column`** (util-linux 2.42.3): `-t`, `-s`, `-o`, `-c`, `-x`, `-n`, `-L`, `-N`,
   `-R`, `-H`, `-J`; widths in display cells; a CRLF line stays CRLF. `-T`, `-W` and
   `-E` are refused by name.
5. **`xxd`** (vim's): `-p`, `-r`, `-i`, `-l`, `-s`, `-c`, `-g`, `-u`, `-b`, `-e`, `-o`,
   `-C`, `-n`, `-d`, `-a`, `-R`; no colour unless `-R always`; `-E` refused.
6. **`hexdump`** (util-linux 2.42.3): `-C`, `-c`, `-d`, `-o`, `-x`, `-b`, `-n`, `-s`,
   `-v`, and `-e FORMAT` with the format language (`_a`, `_A`, `_c`, `_p`, `_u`).
7. **`uuidgen`** (util-linux): `-r` (the default), `-t`, `-m`/`-s` with `-n` and `-N`
   (`@dns`, `@url`, `@oid`, `@x500`), `-x`, `-C`.
8. **`xdg-open`**: `start` under the name cross-platform scripts try first; xdg-open's
   exit codes (1 syntax, 2 no such file, 4 the handler failed).
9. **`pbcopy`** and **`pbpaste`**: the clipboard as Unicode text. `pbcopy` reads UTF-8
   and turns lone LF into CRLF, so Windows programs paste it right; `pbpaste` writes UTF-8
   with CRLF turned into LF, so `pbcopy < f; pbpaste | diff f -` is quiet. A clipboard
   holding no text gives nothing, status 0. `clip.exe` stays as it is: it writes the
   console code page, and Windows has no paste command.
10. **`watch`** (procps-ng 4.0.7): `-n`, `-t`, `-d[=permanent]`, `-e`, `-g`, `-c`,
    `-x`, `-b`, `-p`, `-r`, `-w`; the command runs through cash each time (procps runs
    `sh -c`, which is cash anyway), on the alternate screen; `q` and Ctrl-C end it;
    procps' exit codes.
11. **`free`** (procps-ng 4.0.7): `-b -k -m -g -h --si -t -w -s N -c N -l`; `Mem:` from
    the same call `top` uses; `Swap:` is the page file (used and total from the system's
    page-file information). A `Commit:` row is not added: scripts parse `free` by `Mem:`
    and `Swap:`.
12. **`nice`** and **`renice`** (coreutils, util-linux): niceness to a priority class:
    `-20…-11` HIGH, `-10…-1` ABOVE_NORMAL, `0` NORMAL, `1…10` BELOW_NORMAL, `11…19` IDLE;
    REALTIME is never set. Bare `nice` prints the niceness that maps back from the
    shell's own class. `renice -n N -p PID…` and `-u USER`; `-g` refused (no process
    groups).
13. **`flock`** (util-linux 2.42.3): `-s`, `-x`, `-n`, `-w`, `-u`, `-o`, `-E`, `-c`,
    `FILE CMD…` and the `FD` form through cash's own descriptor table (`exec 9>lock;
    flock -n 9`), with `LockFileEx` on one byte far past the end of the file, so
    readers of the lock file are not blocked. The lock lives on the shell's handle and
    is released when the file descriptor closes or the command ends. A directory is
    refused: Windows cannot lock one.
14. **`nc`** (OpenBSD netcat's flags, Debian's default): connect and pump standard input
    and output, `-z`, `-v`, `-w`, `-n`, `-u`, `-l`, `-p`, `-k`, `-N`, `-q`, `-4`, `-6`,
    port ranges; `-e` and `-c` refused, as OpenBSD refuses them.

Each tool: a module in `cash-builtins` (Win32 calls in `cash-win32`), an entry in
`builtins.md` under its kind, a help page where its `--help` is not enough, its names in
doctor's `CARRIED` list, unit tests, and an integration test in `crates/cash/tests/it`;
where a Linux original exists it is the oracle, run in WSL (archlinux: util-linux
2.42.3, procps-ng 4.0.7, coreutils 9.11, ncurses 6.6, glibc 2.44, vim's xxd), with cash's
deliberate differences replaced in the test, as `small_tools.rs` does. Help pages and
messages name no spec items. Spec D74 records the decisions; ROADMAP gets item 19.

Decided while building (2026-10-06, by Claude, open to the user): the `free` Swap row,
the `nice` mapping, `flock`'s byte and its refusal of directories, and `nc` without
`-e`, each as written above; the gap analysis asked these as Q5–Q8.

---

## Phase 22. A per-user installer, `cash --update`, and winget

Chosen by the user on 2026-10-06, by pick list (spec D75). The goal: install cash and
have a complete native Bash with the tools scripts need, without Scoop. Scoop stays a
channel; the installer is the second, on the releases page, and winget takes it from
there. The research is in `research/packaging-evaluation.md` ("The installer, for winget
later").

1. **Inno Setup, per-user** (`PrivilegesRequired=lowest`, no UAC), built on the GitHub
   Windows runner where it is preinstalled, attached to each release as
   `cash-vX.Y.Z-setup.exe` beside the zip. Silent for scripts and winget:
   `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART`, `/DIR=` and `/TASKS=`.
2. **Layout as Scoop's**: `%LOCALAPPDATA%\Programs\cash\<version>\cash.exe` and a
   `current` junction, so an upgrade never overwrites a running `cash.exe`; open windows
   keep the old file and new tabs get the new one. Inno copies into the version directory
   and runs `cash --install-finish` (new), which moves the junction, writes the Terminal
   profile, refreshes the tool links, runs `--init-rc --once`, and sweeps old version
   directories nothing runs. `CloseApplications=no`: never Ctrl-C the user's shells.
3. **Tool links on PATH by default**: the install task "Make the tools programs on PATH"
   is on, so `ls.exe`, `sed.exe`, `awk.exe`, … are on the user PATH ahead of Git's;
   a checkbox and `/TASKS=` turn it off. `cash.exe` itself goes on the user PATH always.
   PATH is written by cash (`--link-tools --add-to-path`), never by Inno's `[Registry]`.
4. **Uninstall** (Apps & features entry, per-user): `cash --unlink-tools`,
   `--remove-terminal-profile`, the PATH entries, the directory; the user's `~/.bashrc`,
   history and config stay.
5. **`cash --update`**: asks GitHub's releases API for the latest version, downloads the
   zip, verifies its `.sha256`, unpacks into a new version directory, moves the junction and
   runs the same finish step; `--check` only reports. A cash installed by Scoop says
   `scoop update cash` instead (its exe lives under `scoop\apps`). No background polling
   and nothing automatic: a shell that changes itself overnight surprises script authors.
6. **winget** from the installer (`InstallerType: inno`, `Scope: user`, the silent
   switches above): the first manifest by hand, later versions by `winget-releaser`.
   The installer type cannot change later, so this waits until items 1–5 have shipped
   in a release and been installed on a clean machine.
7. **Unsigned**: a browser download of the setup gets SmartScreen's "unknown publisher"
   once; winget and Scoop downloads do not. Documented in `help installing` and the
   README. Signing stays postponed (the user, 2026-10-06: no paid certificate; SignPath
   not wanted).
8. **An optional Scoop task, off by default** (the user, 2026-10-06): the setup offers
   "Also install Scoop, a package manager for command-line tools" (`/TASKS=scoop`); when
   chosen it runs Scoop's own installer (`irm get.scoop.sh | iex`, in a PowerShell with
   `-ExecutionPolicy Bypass`) as the last step, and says what it did. Installing Scoop by
   default was turned down: it changes the machine beyond what installing a shell
   implies. The installer refuses to run beside a cash that Scoop installed (its exe
   under `scoop\apps`), pointing at `scoop update cash`.
9. **A hint for a missing command, at an interactive prompt only** (the user,
   2026-10-06): after Bash's `jq: command not found`, one line `install it: winget install
   jqlang.jq, or scoop install jq`, from a curated table of about 50 common tools (jq,
   rg, fd, fzf, bat, delta, gh, git, node, python, go, rustup, docker, kubectl, helm,
   terraform, aws, az, make, cmake, ninja, 7z, curl's friends wget and aria2, nano, vim,
   neovim, starship, zoxide, direnv, shellcheck, shfmt, …) with both ids; the table is
   markdown in the help docs (`help tools`), and a test checks each id against
   `winget show` and `scoop info` output recorded once. Scripts and `-c` get Bash's
   message and status 127 only. `cash doctor` names the same two commands for what it
   finds missing.
10. **The zip is the third channel, portable** (the user, 2026-10-06): the release zip's
    `cash.exe` runs from anywhere on its own, and README, the release notes and `help
    installing` say so ("Portable"). A portable cash (one that is neither Scoop's nor the
    installer's) **offers once, at its first interactive prompt**, to put itself on the
    user PATH (so `cash` is a command in any window; the user, 2026-10-06), to add the
    Windows Terminal profile and to put its tools on PATH for other programs; the answer is
    remembered in `%LOCALAPPDATA%\cash\portable-offer`, Enter means no, and nothing on
    the machine changes without a yes. Only at a console, never for `-c`, a script or a
    non-interactive shell; `CASH_NO_OFFER=1` skips it. The alternative, changing nothing
    unless asked, was turned down for friendliness, with the noted risk that a shell
    started interactively by a script sees the question once.
11. **`cash doctor` says when cash itself is not on PATH** (the user, 2026-10-06): its
    `cash` line only reports in-shell resolution, which is always cash, so a portable
    cash whose directory is on no PATH still reads `ok`. Add a `path` line: `ok` when the
    directory of the running `cash.exe` (`current` for an installed one, Scoop's shims for
    Scoop's) is on the user or machine PATH, else `note  path  C:\…\cash.exe is not on
    your PATH: new windows will not find cash` with the fix `cash --add-to-path`. That
    command (and `--remove-from-path`) adds or removes the exe's own directory on the user
    PATH, the same writer the offer and the installer use, and the offer's first
    question calls it.
12. **F1: a one-screen help** (the user, 2026-10-06, by pick list). For Windows users
    who know Bash. The title line is `cash <version>  ·  help`, the version read from the
    running binary, with `F1 or Esc closes` at the right. Layout "Paths, Keys, More" — a line of prose and six path examples
    with a reason beside each (`C:/Users/me/src`; `"C:\Users\me\src"` quoted;
    `'C:\Program Files\Git'`; `/c/Users/me` and `~/src`, which cash reads but other
    programs do not; `tool.exe "$(winpath -w "$d")"`; `$PATH` as the one colon-separated
    variable); seven keys (Alt-E, Alt-←/→, Tab, →, Ctrl-R, Alt-., Ctrl-X Ctrl-E); and one
    block on scripts running unchanged, the built-in tools, `help NAME`, `help topics`,
    `cash doctor`, `start FILE`, `sudo CMD`. Styled in the prompt's colours (section
    names in the accent, keys and examples in the highlighter's yellow and green, notes
    dim; plain under `NO_COLOR`). Drawn on the alternate screen like `less`; F1, Esc or
    `q` brings the prompt back as it was; a short window scrolls with the arrows. F1 is
    a Readline function (`cash-help`) that `bind` can move. The starter `~/.bashrc`
    prints `cash: F1 for help` once per window after the banner. The screen is its
    own, kept to one page; `help` stays the catalogue. Its text lives as markdown
    beside the help topics so it reviews as text and the no-spec-references test covers
    it.

Built on 2026-10-06 (items 1–5, 8 and 9; 6 and 7 follow the first release that carries
the installer): `packaging/inno/cash.iss` and its step in the release workflow;
`cash --install-finish`, `--install-remove` and `--update [--check]`
(`crates/cash/src/installer.rs`, junctions in `cash-win32/src/junction.rs`); the
`installing` and `tools` help topics; the hint in the shell's error formatter; doctor's
`install` line. Two details differ from the plan above: Windows renames a directory whose
exe is running (only the delete fails), so a version directory is judged in use by a
write-open of its `cash.exe` before anything is touched; and `ln -s DIR` makes a real
symbolic link, which needs a privilege, so the `current` junction is set by
`FSCTL_SET_REPARSE_POINT` in place, never half-moved. The installer compiles here (Inno
Setup 6.7.3 from Scoop, 11.7 MB from the 1.5.0 files); an end-to-end silent install,
`--update --check` and uninstall were exercised with HOME, LOCALAPPDATA and the registry
key for the user PATH pointed at a scratch directory, since this machine's cash is Scoop's
and the installer refuses to run beside one. Found there: Inno's uninstaller runs with
Windows' redirection-trust mitigation, inherited by what it starts, so no process of its
may cross the `current` junction ("the path cannot be traversed because it contains an
untrusted mount point"); the uninstaller therefore runs `--install-remove` from a version
directory it finds itself. Windows Terminal, the shell and every ordinary process cross the
junction as before. For item 6, `packaging/winget` holds the three manifest templates and
RELEASING step 8 the procedure; `scripts/sync-bucket.py` copies the Scoop manifest's notes
and hooks into the bucket (step 7). Later the same day: the bucket pushed at 1.5.0 with
the quiet notes (its default branch is `master`; a first push to `main` was taken back),
the first start after the real update measured (see "Found along the way"), items 11
(doctor's `path` line, `--add-to-path`) and 12 (F1) built, and 1.6.0 tagged by the user
on commit 252f6354. The release workflow now installs, runs and uninstalls the setup on
the runner every release (the clean-machine run item 6 asked for). Item 6 done: the
first winget submission, `tomcoolpxl.cash` 1.7.0 from the installer, is
[microsoft/winget-pkgs#447760](https://github.com/microsoft/winget-pkgs/pull/447760)
(2026-10-06, `wingetcreate submit`); once merged, later versions can go through
`winget-releaser` in the release workflow. Left: a look at the F1 screen in the user's
own Terminal profile.

---

## Phase 23. `grep`, `diff` and `cmp`: the standing rule reversed

Chosen by the user on 2026-10-06, by pick list, reversing the 2026-09-25 decision
("cash does not carry grep or diff"). That rule was made when cash lived beside Git
Bash; under the vision of 2026-10-06 (install cash, have everything, no Scoop) `grep` is
the single most common failure on a bare machine and `diff` the second.

1. **`grep`, `egrep`, `fgrep`** with GNU grep 3.x's options, messages and exit codes
   (0 match, 1 none, 2 trouble): `-E -F -G -P`? (`-P` refused, naming `-E`), `-i -v -c
   -l -L -n -h -H -o -q -s -w -x -r -R -a -b -A -B -C -m -e -f --include --exclude
   --exclude-dir --color --null -z`, BRE as the default (`\( \) \{ \} \| \? \+` translated
   to the regex crate's syntax), ERE with `-E`, fixed strings with `-F`. Built on
   ripgrep's library crates `grep-regex`, `grep-searcher`, `grep-printer` and
   `grep-matcher` (BurntSushi, MIT or Unlicense) as dependencies, not copied source:
   ripgrep's engine, GNU grep's interface. A pattern the regex crate cannot take, a
   backreference above all, runs through `fancy-regex` (already a dependency) behind the
   same `Matcher` trait, so the fast path is the common one and nothing is refused. CRLF:
   a line's `\r` is not part of the line (D20), so `grep 'x$'` matches on a CRLF file and
   `-o` never prints a CR; `--binary-files` as GNU. Oracle: GNU grep 3.12 in WSL, case by
   case (`grep_cases`), with `--color` sequences compared too.
2. **`diff`** (unified, context, normal, `-q`, `-s`, `-r`, `-N`, `-i`, `-w`, `-b`, `-B`,
   `--color`, `--strip-trailing-cr`, `-u0`, `-L`) and **`cmp`** (`-s`, `-l`, `-b`, `-i`,
   `-n`) from uutils diffutils (MIT), bundled like `sed`: imported, hardened, exit codes
   0/1/2, oracle-tested against GNU diffutils 3.12 in WSL (`diff_cases`, `cmp_cases`).
   `diff` output is what test scripts compare byte for byte, so the golden file is the
   gate.
3. **The rest follows the decision**: doctor's `EXPECTED` entries for grep and diff
   become `CARRIED`; the `tools` table loses its grep and diff rows; README's "It does not
   carry grep or diff" paragraph goes; `help differences` and spec D48/D35 get the
   reversal; the gap analysis records it; `type -a grep` shows Git's and Microsoft's
   behind the builtin, as for every shadowed tool.

Built on 2026-10-06, for 1.7.0. `grep` reads its patterns through the bundled sed's
translator (`cash_sed::sed::compiler::translate_posix`, made public) with sed's GNU error
wording, so the two tools accept one dialect; what GNU grep reads differently from GNU
sed (a repetition at the start of an ERE, an unmatched `)`, a `{` that opens no
interval) is added around it, and a test holds eleven patterns to the same translation
through both. Backreferences, `\<`/`\>`/`\b` and `-z` run on fancy-regex behind
ripgrep's matcher trait; the rest on grep-regex's prefilters. 714 oracle invocations
against GNU grep 3.12 in `C.UTF-8`; the deliberate differences are the CRLF rule and
name-ordered directory walks. `diff`/`cmp` are a vendored `cash-diffutils` (uutils
0.6.0; CASH-PATCHES.md): GNU's option parser, a change-script engine with `similar`'s
linear-space Myers where upstream's never finished a 100k-line pair, the five printers,
`-r`/`-N`/`-x`, `--strip-trailing-cr`, colour; 102 oracle sections byte for byte against
GNU diffutils 3.12. Doctor's `EXPECTED` list is empty now; the install-hint table lost
its grep and diff rows; README's paragraph says the three are built in. The winget
submission (phase 22 item 6) waits for 1.7.0, so the first winget version carries these
three (the user, 2026-10-06).

---

## Phase 24. `gzip`, `gunzip` and `zcat`

Chosen by the user on 2026-10-06, by pick list (the gap analysis's Q3, reopened): the gzip
family only; `xz` and `bzip2` stay with `tar.exe`. On this tooled machine `gunzip` and
`zcat` resolve to nothing, and `tar.exe` reads archives, not a bare `.gz`.

1. **`gzip`, `gunzip`, `zcat`** with GNU gzip 1.14's options, messages and exit codes
   (0, 1 error, 2 warning): `-c -d -f -k -l -n -N -q -r -S -t -v -1..-9 --fast --best
   --rsyncable`(accepted), `--help`, `--version`; `gunzip` = `gzip -d`, `zcat` = `gzip -dc`;
   files replaced in place with the `.gz` suffix added or removed, times and modes kept
   where Windows has them, the original name and time in the header (`-N`), `-l`'s
   table, `-t`, stdin and stdout, a refusal to write compressed data to a terminal
   without `-f`, `-r` over directories, the `.tgz`/`.taz` suffix rules, `GZIP` environment
   variable accepted with GNU's warning. Pure Rust: `flate2` with `miniz_oxide` (already
   in the dependency tree). CRLF: bytes are bytes. Oracle: GNU gzip 1.14 in WSL
   (`gzip_cases`), byte-for-byte on the compressed output where gzip's output is
   deterministic (`-n`, fixed mtime) and on the decompressed side everywhere.
2. The entries in `builtins.md`, a page, `CARRIED`, the gap analysis's Q3 row.

## Phase 25. Compatibility corners

Chosen by the user on 2026-10-06, by pick list, as the block after gzip; each item is
script-visible Bash behaviour or a kinder refusal.

1. **`/dev/tcp/HOST/PORT` and `/dev/udp/HOST/PORT`** in redirections (`exec 3<>
   /dev/tcp/host/80`, `cat < /dev/tcp/…`, `echo > /dev/tcp/…`), as Bash's: a socket in
   the descriptor table, both directions, Bash's messages on failure
   (`bash: connect: Connection refused`, `bash: /dev/tcp/h/p: Connection refused`).
2. **`command_not_found_handle`** run as Bash runs it: with the command and its
   arguments, its status the command's, the hint only when no function is defined
   (found 2026-10-06).
3. **Persistent `abbr`**: fish keeps abbreviations across sessions; cash keeps them in
   `%APPDATA%\cash\abbreviations` (one `name=expansion` per line), written by `abbr -a`
   and `abbr -e`, read at startup after the rc files, so `abbr -a gco git checkout` once
   is enough; `abbr --no-save`? No: fish has no such flag; a `~/.bashrc` that sets them
   keeps working and wins.
4. **Kinder refusals**: `suspend` (no stop signal on Windows; Bash's words for a login
   shell as the model), `mkfifo` (named pipes are not files on Windows; point at process
   substitution), `stdbuf` (no preload on Windows; `-o0`/`-oL` accepted as a no-op with
   a note when the program is cash's own, refused otherwise) — each a builtin with a
   one-line message and Bash's status, not "command not found".
5. **`getconf`** (`_NPROCESSORS_ONLN`, `_NPROCESSORS_CONF`, `PAGESIZE`, `LONG_BIT`,
   `PATH`, `ARG_MAX`, `NAME_MAX`, `PATH_MAX`, `-a`) and **`locale`** (`-a` listing
   `C`, `POSIX`, `C.UTF-8` and the Windows locales as `en_US.UTF-8` names; bare `locale`
   printing `LANG`, `LC_*`), the values Windows has, GNU's output shapes.

---

## Phases 26 to 30. Archive and compression tools, on shared parts

Chosen by the user on 2026-10-07, by pick lists, after asking for "a common thing"
rather than tar alone: the design is `research/archive-tools-design.md` (D78). The
family: the `bzip2`, `xz` and `zstd` commands with their aliases, `tar`, and `zip`,
`unzip`, `zipinfo`; the z-tools, `cpio` and `lzip` are not wanted now. Two new library
crates, `cash-archive` and `cash-getopt`. Everything in pure Rust; the bzip2-1.0.6
license accepted for `libbz2-rs-sys`. Order: the groundwork, the compressors, tar, zip.
Passed over: uutils/tar (0.0.1, less than `tar.exe` does), `libzstd-rs-sys` (a
prerelease without a Rust API), `xz2` and `liblzma` (C).

## Phase 26. `cash-getopt`, and every option parser on it

1. **The crate**: `cash-diffutils::getopt` lifted into a leaf crate and grown to the
   `getopt` builtin's reach: optional values, the three orders (permute, stop at the
   first operand, in place), long-only, items returned in order, typed problems that
   each tool words; GNU tar's old-style keys (`tar xzf a.tgz`) as a helper.
2. **Onto it, each under its own tests and oracle**: diff and cmp; gzip; grep (with its
   `-NUM`); column; hexdump; ss; uuidgen; flock (stops at the first operand); iconv;
   watch (POSIX order); free (which takes no prefixes today, though procps's does);
   nc; and the `getopt` builtin itself where its canonical output allows.

## Phase 27. One Unix face for a Windows file

1. **cash-win32**: `unix_view` (mode, owner and group with their RIDs, link count,
   times to the nanosecond, identity), with `ls`'s rule for the mode (the access list
   decides `w`); `replace` (gzip's `Target`); `symlink` (Developer Mode or elevated, a
   typed refusal otherwise), `hard_link`, `set_times` (directories too), `set_attributes`;
   `check_name` (reserved names, `<>:"|?*`, a trailing dot or space).
2. **Onto it**: `ls` (the group read for the first time, not the owner again), `stat`
   (its mode follows `ls`'s rule, so it changes for some files), `dos2unix` (gains the
   read-only handling it lacks), gzip's in-place replacement.

## Phase 28. `cash-archive`'s codecs, the compressor driver, `bzip2`, `xz`, `zstd`

1. **`codec`**: gzip (flate2), bzip2 (`bzip2` on `libbz2-rs-sys`), xz, lzma and lzip
   (`lzma-rust2`), zstd (`ruzstd`, writing at its fast level); recognised by magic and by
   suffix, concatenated streams, `-l`'s facts, typed errors; gzip's member code and its
   `Input` reader moved in. TODO, later: `libzstd-rs-sys` once it has a Rust API.
2. **The driver**: one in-place compress/decompress engine with a `Profile` per tool
   (names and presets, options, defaults, suffixes, messages, exit statuses, `-l`); gzip
   moved onto it first, its oracle unchanged.
3. **`bzip2` `bunzip2` `bzcat`** (bzip2 1.0.8), **`xz` `unxz` `xzcat` `lzma` `unlzma`
   `lzcat`** (XZ Utils 5.8), **`zstd` `unzstd` `zstdcat`** (zstd 1.5.7): options,
   messages and statuses from WSL oracles; pages, `builtins.md`, `CARRIED`.

## Phase 29. `tar`

cash's own `tar`, GNU tar 1.35's interface, messages and exit codes, on cash-archive's
`member`, `names`, `walk`, `extract`, `select` and `listing`, built in this phase, and on
the `tar` crate's `Header` and PAX parser with a block loop of cash's own. Windows' own
`tar.exe` (bsdtar 3.8.8) stays reachable by its path and by `enable -n tar`. Needs
cash-core's `Pattern` path flags and cash-sed's one-`s///` entry point (design 3.9, 3.10).

1. **Reading: `-t` and `-x`**: ustar, GNU (long names and links, sparse members read)
   and PAX headers; `-f FILE` and `-` (default `-`, or `TAPE`), refusing a terminal as GNU
   does; compression found by its magic when reading a file; `-C` in order, member
   operands with GNU's matching (`--wildcards`, `--anchored`, `--ignore-case`,
   `--wildcards-match-slash`, `--occurrence`), `--exclude`, `-X`, `--strip-components`,
   `--transform` (cash-sed's `s` compiler), `-O`, `-k`, `--skip-old-files`,
   `--keep-newer-files`, `--overwrite`, `--overwrite-dir`, `-U`, `-m`, `-p`,
   `--delay-directory-restore`, `-K`, `-N`; `-v` and `-vv` listings and `--totals`
   column for column, names quoted in GNU's escape style; `..` and absolute names as GNU
   refuses and strips them.
2. **Writing: `-c`**: GNU format by default, `--format=gnu|oldgnu|ustar|pax|posix|v7`;
   `-T`, `--null`, `--no-recursion`, `-h`, `--hard-dereference`, hard links as links,
   `-P`, `--exclude-vcs`, `--exclude-backups`, `--exclude-caches*`, `--owner`, `--group`,
   `--mode`, `--mtime`, `--numeric-owner`, `--sort=name|none|inode`, `--remove-files`,
   `-a`; "Removing leading `/'", and a drive letter removed as GNU's DOS builds remove it;
   byte for byte GNU tar's archive where GNU's is deterministic.
3. **Compression** from phase 28's codecs: `-z`, `-j`, `-J`, `--lzma`, `--lzip`,
   `--zstd` (archives somewhat larger than GNU's, whose zstd writes at level 3), `-a`;
   `-Z`, `--lzop` and `-I PROGRAM` said plainly.
4. **Changing archives**: `-r`, `-u`, `-A`, `-d` (`--compare`), `--delete`, on
   uncompressed archives as GNU allows them.
5. **Windows**, from phase 27's Unix face: modes, owners and their ids, read-only,
   times; symbolic links made when Windows allows it, else GNU's "Cannot create symlink"
   and status 2; names Windows cannot hold reported, not mangled; long paths.
6. The page, `builtins.md`, `CARRIED`, `DELIBERATE_SHADOWS` (System32's `tar.exe`), the
   README's tool list, and an oracle: `tests/oracle/tar_cases.sh` under GNU tar 1.35 in
   WSL, with GNU gzip, bzip2, xz and zstd beside it; archives cash makes read by GNU tar
   and `tar.exe`, and theirs by cash.

## Phase 30. `zip`, `unzip`, `zipinfo`

1. **On the `zip` crate** (zip-rs, MIT, 8.6) with phase 28's codecs (deflate, deflate64,
   bzip2, lzma, xz; zstd and PPMd as they allow without C), on cash-archive's `select`
   and `listing`, and on tar's walk, names and extraction, moved into cash-archive as
   zip comes to share them (design 3.12): Info-ZIP zip 3.0's and UnZip 6.00's options,
   messages and exit statuses, `zipinfo`'s listings; Info-ZIP's extra fields (extended
   times, Unix modes) so archives round-trip with Info-ZIP, Explorer and `tar.exe`;
   writing to standard output streamed; traditional encryption read (`-P`).
2. The pages, `builtins.md`, `CARRIED`; the GnuWin32 `zip` and `unzip` rows leave
   `help tools` and `tests/fixtures/tools-ids.txt`; oracles under Info-ZIP in WSL.
3. Decided in the phase: one deflate backend for all (`miniz_oxide` or `zlib-rs`), the
   numeric owner (RID or MSYS's mapping), and what host a zip says it was made on.

## Phase 31. `7z` and `7za`

Chosen by the user on 2026-10-07, by pick list, after 1.9.0: a builtin under both names
(as 7-Zip ships `7z`, `7za` and `7zr`), shadowing Scoop's 7-Zip inside cash, which stays
reachable by its path and `enable -n 7z`; doctor lists it as a deliberate shadow.

The design is `research/archive-tools-design.md` 3.14, set by a second pick list the
same day: 7z and the formats cash has, Windows 7-Zip's words with LF and `/`,
`sevenz-rust2` taken in as cash's own code, volumes, `h`, links, and `-sns`/`-sni` as
7-Zip has them (WIM only, so "Not implemented"). For 1.10.0. Each step ends in a commit.

1. **`cash-archive::sevenz`**: `sevenz-rust2` 0.23.0's source taken in, with its tests;
   the license in `licenses/` and `NOTICE`; brotli, lz4, wasm and the C zstd out; the
   codecs shared; raw block copies; 7-Zip's archive facts and method names; entries in
   order; typed errors, no panics on the two hostile inputs found.
2. **`7z` and `7za` reading** (done 2026-10-07): the parser, `l` (with `-slt`, `-ba`),
   `t`, `x`, `e`, the overwrite question and `-ao`, `-o`, `-p`, `-so`, `-i`/`-x`, `-r`;
   the oracle (`7z_cases.sh`, 7-Zip 26.03 from Scoop). Left for later in the phase: the
   progress line on a console (`-bsp`, `-bd`; done 2026-10-07, 7-Zip's
   `CPercentPrinter` and its hooks in scanning, opening, extracting, adding and `h`,
   matched against `-bsp1` into a pipe), `-si` reading (done with step 4), `-bt`,
   `-spe`, `-snz`, `-ai`/`-ax` beyond names (all done 2026-10-07: archives are found by
   the scan `a` uses, directories entered, `-air`/`-ax` honoured, "Cannot find archive" when
   none; `-bt` reports the command's own times and cycles, the shell process's memory
   peaks), and `i`'s listing is cash's own short one, on purpose: 7-Zip's lists formats
   and codecs cash does not have.
3. **Writing 7z** (done 2026-10-07): `a`, `u`, `d`, `rn`, the `-u` matrix and `-u!name`,
   `-m` (levels, methods, solid limits, filters chosen by a file's head as 7-Zip's
   analysis does, `-mhe`, `-mhc`, times, `-mqs`), `-p` typed or given, `-sdel`, `-stl`,
   `-w`, `-sa`; old blocks copied as they are or repacked; the oracle's update sections
   match 7-Zip 26.03 line for line and its stored archives byte for byte. `-si` came
   with step 4. Left: `-spf` for update, 7-Zip's BCJ2 (it filters x86
   executables with BCJ2 from `-mx8`; cash has no BCJ2 encoder and uses BCJ), analysis
   levels other than the default (`-myx`), memory limits (`-mmemuse` is taken, not
   applied). Compressed archives are not 7-Zip's bytes (other LZMA, PPMd and bzip2
   encoders), so the oracle compares their listings without sizes.
4. **The other formats** (done 2026-10-07): reading gzip, bzip2, xz, zstd, lzma, tar
   and zip; writing tar (GNU and pax headers, long names), zip (7-Zip's records: NTFS
   times, OEM names with the UTF-8 one beside, Store when compressing does not help;
   Deflate, BZip2, LZMA, PPMd, xz; ZipCrypto and AES; kept items copied, renamed ones
   under a new local header), gzip, bzip2 and xz (one file); the format by extension or
   `-t`, an archive there opened as that format only; times compared at the precision
   the format keeps; `-si` for every format, `-so` for tar and the streams; `-si -t`
   reading for `l`, `t`, `x` and `e` as 7-Zip's one-pass opening shows it (tar and the
   streams, "Not implemented" for 7z and zip; standard input kept in a temporary file
   meanwhile, where 7-Zip reads it as it goes); `-mm=` and
   `-mNAME-` as 7-Zip takes them (the 7z path too). `7z_formats.sh` matches line for
   line, its tars and stored zips by `cksum`; 7-Zip tests every archive cash writes.
   Left, where cash differs:
   - zip with Deflate64 says "Not implemented" (no encoder); 7-Zip writes it.
   - xz is one block; 7-Zip on several threads cuts blocks of four dictionaries
     (phase 32). xz's `-mf` filters say "Not implemented".
   - Zip names in code page 437 where 7-Zip uses Windows' OEM code page (850 in
     Belgium); `-mcp` other than 65001 is taken as 437. tar's `-mcp` is not applied.
   - A zip's self-extractor stub is kept but not checked against 7-Zip; Zip64 headers
     follow 7-Zip's one-thread path (its threads decide a file near 4 GiB otherwise).
   Not yet checked against 7-Zip: tar names that are not ASCII (7-Zip prints its UTF-8
   check's status after "UTF8"); zips 7-Zip reads by their local headers (no central
   directory, "Local" in Characteristics), split zips, Unix-made zips' names and modes.
5. **Volumes, `h` and `-scrc`, links, `-sns`/`-sni`**. `h` and `-scrc` done 2026-10-07:
   7-Zip's ten hashes (BLAKE2sp from `blake2s_simd`), its columns and its sums for data
   and for data and names, `-ba`, `-si`; `t` and `x` sum what they read (`t` directories
   too, as 26.03 does). Volumes done 2026-10-07: `-v` writes `NAME.001` and on in every
   format (7z, zip and tar to 7-Zip's bytes), "Volumes: N"; `NAME.001` reads as 7-Zip's
   "Split" level over the archive inside, the volumes not opened again, their sizes in
   the totals. The spanning is cash-archive's `volumes` (`Spanned`, `numbered`,
   `SpannedWriter`), which zip's split parts use too. Links and `-sns`/`-sni` done
   2026-10-07: `-snl` keeps a junction or a symbolic link as one (not entered, its
   reparse data read: a symbolic link in tar, relative to its directory; in 7z and zip the
   reparse attribute, and for a file the data as 7-Zip passes it; `h` hashes the data);
   `-snh` makes a file met again under another name a tar hard link; extraction makes a
   tar's links after every item, from empty placeholders, with 7-Zip's `-snld` danger
   checks and words, `-snl-` leaving symbolic links out; `-sns`/`-sni` "Not implemented"
   when writing, nothing when extracting (no format here has streams). Also fixed: the
   overwrite question now comes before the item's `-` line, a skipped file shows as
   skipped, and a break still reports "Sub items Errors". 7-Zip's links kept in an
   item's data (`is_SymLink_in_Data`) done the same day: a zip's Unix symbolic link
   (mode `S_IFLNK`) and an item with the reparse attribute, under 4 KiB, read whole and
   made a link at the end, or written as they are with "Incorrect reparse stream" (the
   oracle checks a Unix zip's links 7-Zip refuses; a symbolic link actually made needs
   the right this machine lacks, so it is checked only by `cash-win32`'s test, which
   takes either outcome). A made symbolic link does not get the item's times (7-Zip's
   `SetLinkFileTime`). A file answered "No" or skipped: 7-Zip's tar handler
   neither shows nor counts it, its 7z handler shows it at `-bb2` and counts it; cash
   does the latter for every format. `h` on hash files (7-Zip 23's `-thash`) is not
   taken; 7-Zip's split handler also reads `NAME.aa`, `NAME.ab` volumes, cash's only
   numbered ones.
6. The page, `builtins.md`, `CARRIED`, `DELIBERATE_SHADOWS`, doctor (done
   2026-10-07). `CARRIED` has `7z` and `7za`; doctor's new note names a 7-Zip on `PATH`
   outside System32 that cash's `7z` goes ahead of. The phase's full suite passed the
   same day (4374 tests).

---

## Phase 32. Every core, where the format allows it

Asked by the user on 2026-10-07 ("if the algorithm allows it, parallelization is used"),
decided by pick list the same day: after phase 31, released with it as 1.10.0. A survey
the same day found no compressor in cash using more than one thread.

1. **xz** (done 2026-10-07): `XzWriterMt` (lzma-rust2) on all cores by default, as XZ
   Utils 5.8 does (`-T0`), with its block size (three dictionaries, at least 1 MiB) and
   as many threads as a quarter of the memory holds; `-T1` the one-thread stream; `-T N`
   and `-T +1` honoured, `--block-size` still exact. A file of one stream and several
   blocks decodes with `XzReaderMt`; others (lzma-rust2 0.21's threaded reader loses a
   second stream) go through the stream decoder. 64 MB of text: 23.7 s on one thread,
   9.7 s by default (three blocks), XZ Utils 6.9 s.
2. **zstd** (done 2026-10-07): the 4 MiB frames compressed `-T` at once
   (`codec::parallel`), the same bytes for any count; `-T`, `--threads`,
   `--single-thread`, `ZSTD_NBTHREADS` honoured. Every core by default, where zstd
   1.5.7 uses one (the user's choice, 2026-10-07: the bytes do not change); tar's
   `--zstd` too. 64 MB: 0.40 s → 0.12 s.
3. **gzip** (done 2026-10-07): one member of 1 MiB chunks deflated apart on all cores,
   each but the last ended with a sync flush, as pigz lays it out without its shared
   window; input of one chunk keeps today's bytes, and the layout is the same on any
   machine. No new option (GNU's gzip has none). 64 MB: 0.41 s → 0.09 s.
4. **bzip2** (done 2026-10-07): input cut at the block size (`-1` to `-9`, 100 to
   900 kB), each a bzip2 stream compressed on its own core, concatenated (pbzip2's
   layout); one block's input keeps bzip2 1.0.8's bytes. The oracle's `BZIP2=-1` case
   on 300 kB is now three streams (216 bytes, not 134), a divergence in the test.
   64 MB: 2.4 s → 0.39 s.
5. **zip** (done 2026-10-07): members compressed in parallel, written in order, the
   same bytes (`compress_ahead`, `add_ahead`; a test compares with `add`). Batches of
   twice the cores, files up to 64 MiB, 256 MiB in all; encrypted, `-l`-converted and
   larger files as before. 48 files of 1.4 MB: 0.09 s (Info-ZIP 0.86 s).
6. **tar** gets the codecs' threads through `-z`, `-j`, `-J`, `--zstd` (done
   2026-10-07): `codec::writer_on`, each codec's own default (xz as many as memory
   holds, the others every core). `tar -cj` of 64 MB: 12.5 s → 0.56 s.
7. The pages (`gzip.md`, `bzip2.md`, `xz.md`, `zstd.md`, `zip.md`, `tar.md`) say
   what runs on several cores (done 2026-10-07); the oracles pass. The timings above
   were taken by hand on an idle machine; no timing test was added, since one would fail
   on a busy CI machine.
8. Release 1.10.0 with 7z and this: the version raised 2026-10-07, after the full suite
   (4384 tests), clippy with every feature, the doc tests and `cargo doc` passed; the tag
   is yours to push once CI is green.

---

## Phase 33. RAR: `7z` reads it, `rar` and `unrar` builtins

Asked by the user on 2026-10-07 ("can we do rar?", then the link to
[bitplane/rars](https://github.com/bitplane/rars)), decided by two pick lists the same
day: after phase 32, so 1.10.0 ships without it. rars is a pure-Rust RAR library under
Apache-2.0 that reads RAR 1.3 to 7 (encrypted too) and writes RAR 2.9 and 5/7; young
(begun 2026-04, 0.10.0 on 2026-10-03), slower than WinRAR by its own account. 7-Zip
reads RAR and writes none; 7-Zip's own RAR code is under the unRAR restriction and is
not used.

rars is in, as `cash-archive::rar` (its tests in the slow lane). What is left:

1. **`7z l`, `t`, `x`, `e` on `.rar`**, as 7-Zip 26.03's `Rar5Handler.cpp` and
   `RarHandler.cpp` do (their sources in the scratchpad's `7zsrc`, fetched with the
   user's leave on 2026-10-07). `l`, `t`, `x` and `e` are in, RAR 1.5 to 7
   (`sevenzip/rar7`; `7z_rar.sh` and `7z_rar4.sh` check them against 7-Zip on 160 of
   rars' fixtures). Left:
   - 7-Zip's strictness where rar's decoders are WinRAR's: closed 2026-10-10 by the
     user's rule (a faulty archive needs a faulty status, not 7-Zip's words). A RAR 1.5
     volume opened in the middle is a data error in 7-Zip and a CRC error in rar
     (`rar154/random.r00`): both faulty. A RAR 3 VM program that is none of the standard
     filters now fails in 7z as well (`Unpack29::with_standard_filters_only`, as the rar
     builtin has it), a data error where 7-Zip says "Unsupported Method"
     (`7z_rar4.sh`). A match reaching before the start of the data is a data error in
     7-Zip; in 7z too for RAR 5 (finish mode), zeros for RAR 2.0 and 2.9 (`zero_fill/`,
     which WinRAR reads as valid): left so.
   - Copy links (`-oi`): done 2026-10-10. A link points at the last file before it of
     its target's name, version and size (`FillLinks`). One to a file that starts its
     stream decodes that file again; one to a solid file takes a copy kept as that file
     was decoded, up to 4 GiB. 7z now plans RAR from the files asked for, as 7-Zip's
     handler does, and skips a link inside a chain undecoded. `t -scrc` extracts to the
     hasher, and the summary's `Size` counts what a file or the hashes took. 7-Zip's
     own slips are kept: a link decoded inside a solid stream breaks the file after it,
     and a link to a solid file that no stream goes on from comes out empty, "Everything
     is Ok" (`copy_links_*.rar` in `7z_rar.sh`).
   - Output on a data error, RAR 5: done 2026-10-10. The RAR 5 decoder has 7-Zip's
     finish mode (`Unpack50Decoder::set_finish_mode`), found by damaging one byte of
     WinRAR archives at a time and comparing 7z with 7-Zip's verdict, bytes and
     checksum:
     - what decoded before the fault is written;
     - tables must be complete codes or empty;
     - a match before the data, or a repeat never set, gives zeros and fails the file;
     - a main code past a block's end, or a symbol read to the end of its bytes,
       stops; extra bits in the last byte's unused bits, or those bits set, fail the
       file at its end;
     - the data is decoded to its last block, past the size into the window only;
     - a filter of no known type, more than 4 MiB on, or inside the last filter's range
       is skipped as "Unsupported Method" (a data error wins over it, it over CRC);
     - a filter past the file's end is kept, and nothing from its start is written;
     - in a solid stream the window and tables carry on after a fault, the next file
       starting at the size's end at the soonest.

     Every offset of the sweeps matches (text, an executable with E8 filters, Delta,
     ARM, solid; `7z_rar.sh` keeps a handful). The rar builtin keeps the old decoding:
     WinRAR's own on damaged data is not observed.
   - Output on a data error, RAR 1.5 to 4: closed 2026-10-10 by the user's rule that a
     faulty archive needs a faulty status, not 7-Zip's exact bytes. Both give one;
     7-Zip's old decoders flush their window in large chunks, so a small damaged file
     gets nothing written in either, and the error word differs on some offsets (CRC
     against data error, both ways; 5 of 30 offsets of `rarvm/filter_bsdcat_exe.rar`
     and `rar250/unpack20_keep_tables.rar`).
   - Alternate streams (`STM`) and ACLs: done 2026-10-10. A stream is listed with
     `-sns` only, its count and sizes on lines of their own in `l`'s totals and in
     `t`/`x`'s summary ("Alternate Streams"), wanted with its file, and extracted into
     its file's stream unless `-sns-` (cash wrote `a.txt_note`). An ACL is decoded at
     open, its file's "ACL" characteristic, and its `NT Security` is 7-Zip's own text
     for a descriptor (owner, group, `s:`/`d:` entry counts, size; not SDDL as this
     line said), checked on 48 made-up descriptors (`nt_security_crafted.rar`). Left:
     `x -sni`, which 7-Zip uses to set the ACLs on what it extracts.
   - An SFX's archive inside its `.exe`: done 2026-10-10. A file that opens as nothing
     at its start (a tar whose first checksum is wrong does not), and is not named as
     RAR, is searched for a RAR 5 signature starting at most 8 MiB in (7-Zip's limit,
     to the byte), opened there, and listed with `Offset`; a `.exe`, `.dll` or `.sys`
     that is no PE adds 7-Zip's "Cannot open the file as [PE] archive" warning. A RAR
     name is tried as RAR only, as 7-Zip does (no tar or lzma guess after it). Left: a
     RAR 1.5 to 4 SFX (WinRAR 7.23 writes none to observe), and other formats after an
     SFX module (7z's own `.sfx`, zip's).
2. **`rar` and `unrar` builtins** with RARLAB's console interface, reading and writing:
   designed in `research/archive-tools-design.md` 3.15 and picked by the user on
   2026-10-07/08 (the fuller set, RAR 5, Scoop's WinRAR 7.23 as the oracle, built by
   observation only). Done: the command line (`RARINISWITCHES`, `rar.ini`, every switch
   read), the banner and `-?`, and the listings `l`/`lt`/`lta`/`lb`/`v…` with `-v`
   (`rar_list.sh`, 12,470 lines like WinRAR's); `t`, `x`, `e`, `p` with the overwrite
   question and `-o`, passwords asked per file, the path switches, volumes from any
   one, damaged headers (rars' lenient read), recovery records and `.rev` files
   (`rar_extract.sh`, 2,880 lines); `a`, `u`, `f`, `m`, `mf`, `d` with the archiving
   switches, RAR 5 only (`-ma4` refused, the user's pick, 2026-10-08), stored archives
   and volume sets WinRAR's byte for byte (`rar_write.sh`, 1,634 lines). For that rars
   gained a WinRAR layout (`rar50::Layout`: padded sizes and offsets, the locator, skip
   flags, CRC32 or BLAKE2, the quick-open threshold, volumes counting their headers) and
   carried members (`Builder::carry`: an archive's packed data copied on update, RAR 1.5
   to 7, as rar copies it). Left, in order:
   - `t` of a RAR 3 `.rev` file: done 2026-10-09. Damaging them under Rar.exe showed
     the newer naming's last four bytes are a CRC32 of the rest (and the three before
     them its counts and number), which `t` checks; the older naming's are not tested
     by rar, nor by cash (`rar_reconstruct.sh`).
   - Extraction: `-f`/`-u`, `-ad1`/`-ad2`, directories' times and attributes (`-ai`
     keeping the times only), and one summary for the archives a wildcard matches,
     their errors added up: done 2026-10-09 (`rar_extract_switches.sh`, 363 lines; this
     line said "built" of them, but extraction read none, set no directory's time, and
     summed up each archive alone). `-ts`, `-tsc`, `-tsa` and `-tsm-` on extraction:
     compared with Rar.exe by hand, not in the oracle.
   - Links on extraction: done 2026-10-09 (`rar_links.sh`, 176 lines; extraction wrote
     each link as an empty file, saying OK). Hard links, file references, `-ol-`, and
     symbolic links skipped as unsafe (absolute, or more `..` than the link's own
     directories) are in the oracle, with two archives of cash's own fixtures
     (`crates/cash/tests/fixtures/rar`: `copies.rar` made by Rar.exe `-oi`,
     `links_unsafe.rar` rars' `symlink.rar` with its targets changed). Left out of it,
     compared by hand: a symbolic link Windows refuses for want of the right ("Cannot
     create symbolic link ... You may need to run RAR as administrator", a directory's
     link leaving an empty directory, as rar's does) and one it makes, which needs that
     right.
   - A file reference whose file is not asked for: done 2026-10-09 (in
     `rar_links.sh`, now 319 lines). The file is unpacked first to a temporary
     `__tmp_reference_source_N.N.rartemp` in the destination, as typed (`x//` for `x/`),
     the reference copied from it, the temporary file removed at the end; in a solid
     archive too, where cash had nothing left to copy.
   - A solid archive's files after one not asked for: done 2026-10-09
     (`rar_extract_switches.sh`, now 449 lines). cash decoded a skipped file only when
     it continued the stream, so a solid archive's first file, when not asked for, was
     not decoded and every file after it failed its checksum: `rar x solid.rar
     dir\later` gave "checksum error". Now each file not asked for is decoded on the
     way, a "Skipping" line said for it in `x`, `e` and `t` (none in `p`), and nothing
     is read after the last file asked for; with none asked for, every file is skipped,
     as rar does.
   - Junctions on extraction: done 2026-10-09 (in `rar_links.sh`, now 226 lines).
     cash made a junction a plain directory, whatever the switches. Now a junction is a
     link: skipped as unsafe (its target is absolute) without `-ola`, left out with
     `-ol-` ("No files to extract", rc 10), and made a junction with `-ola`, its line
     ending in rar's "   ? " after the line of the directory made for it. A link's directory
     is made only once the link is to be made, after its line, as rar does.
   - `-ol` on `a`: done 2026-10-09 (in `rar_add_links.sh`, now 613 lines). A junction
     is followed without it (its target's files under its name, its own entry a plain
     directory with its own time), left out with `-ol-`, and with `-ol` or `-ola` stored as
     a type-3 link: a directory entry with the reparse attribute (0x410), the substitute
     name its reparse data holds (`\??\C:\...`) with `/`, flag 1, taken by a mask
     without `-r` as a file is. A tree holding a junction used to fail whole ("Access
     is denied"). `lt` named a junction "Windows junction", where rar says "NTFS
     junction point", and gave a directory link sizes, which rar leaves out. Symbolic
     links (type 2, the substitute name likewise) are not observed: making one needs a
     right this desktop lacks; rar's word for them, "Windows symbolic link", was seen by
     listing a junction archive retyped to 2.
   - `-oh`, hard links: done 2026-10-09 (in `rar_add_links.sh`, now 422 lines). A
     file's other names put in after its first are type-4 links to it, their sizes not
     padded, with the times the directory's entry gives (a name not used since the file
     changed keeps old ones there, and rar stores those). `-oi` counts them among the
     identical files, and they stay hard links. A file's own size and times are now
     taken from the file opened, not its directory's entry, as rar's are.
   - `-oi`, identical files as references: done 2026-10-09 (`rar_add_links.sh`, 328
     lines, the archives WinRAR's byte for byte). Files of the least size or more
     (64 KB, or `-oi:SIZE` with rar's units) with the same size and the same bytes,
     among those put in: each set's first in archive order is stored, the rest as
     type-5 references to it, their sizes padded as a file's. "Searching for identical
     files" and the count, `-oi2` listing the sets first (by size, smallest first, a
     blank line between), `-oi3` and `-oi4` listing only, no archive and for `-oi4` no
     banner. rars' builder has `add_link` for any kind now. `-oi` with volumes: done
     2026-10-10 (in `rar_add_links.sh`, now 676 lines, each volume WinRAR's byte for
     byte): rars' WinRAR volume framing now carries a link's record (a hard link's
     sizes unpadded), where cash refused ("symbolic links are not supported in volume
     output"). A solid archive's reordering with `-oi` was compared by hand, its
     listing Rar.exe's. A solid set's later volumes say "Creating solid archive" (cash
     said "Creating archive"); where a compressed set splits stays cash's own.
   - `-ver`: done 2026-10-09 (`rar_versions.sh`, 315 lines, the archives WinRAR's byte
     for byte): rars reads and writes the version record, `a`/`u`/`f` keep the file
     replaced as `name;N` with `-ver`, `-verN` drops the oldest beyond N and numbers
     the rest again, listings show `;N` and lt "File version", `x`/`t`/`p` take an older
     version only named exactly, all with `-ver`, version N plainly named with `-verN`,
     `d` matches the `;N` name. Versions in a solid archive: done 2026-10-09 (in
     `rar_versions.sh`, now 330 lines): its members, written again rather than carried,
     lost their numbers, and a second `-ver` update failed on "duplicate archive entry
     name"; rars' builder's `set_version` now numbers any member, carried or not. Left:
     a file comment on a member with older versions (set by name, it may land on the
     wrong one).
   - A solid archive's update: done 2026-10-09 (`rar_solid.sh`, 458 lines, the
     counters' backspaces and shares kept). Rar.exe says "Updating solid archive" and
     shows its repacking as it goes through the members: with a file replaced or a
     member dropped, every member kept is repacked, counted run by run after
     "Repacking archived files:" (seven columns, backspaced over, and the share of the
     bytes written so far after each holding data, not in `d`); a member replaced or
     dropped is said, and before the next kept one "Analyzing archived files:" counts
     those since the last; a file added leaves the run going, its next count written
     after the file's OK. With files only added, the old members before the first are
     analyzed, those after repacked. `d` with nothing to delete repacks every member
     before "No files to delete". `-idp` shows none of it. A file's line under `-idp`
     keeps four spaces where its share would be (cash kept none: no oracle had `-idp`
     on `a`).
   - A damaged header that still parses: done 2026-10-09 (in
     `rar_extract_switches.sh`, now 568 lines). rars' lenient read stopped at the first
     header failing its checksum; past the main header it now keeps it, marked
     `damaged`, as Rar.exe does: `t`, `x`, `l` and `lb` say "Corrupt header is found"
     and "NAME - the file header is corrupt" on standard error, two errors each, rc 3,
     and use the header as it stands. A time past 2185 is read wrapped around, as rar
     keeps times as nanoseconds in 64 bits (a damaged one showed as 1899). Its hour
     before 1914 is the zone's own of the day in cash, Windows' present one in Rar.exe
     (masked in the oracle; a 1795 date was 42 minutes apart).
   - `-ts` on `a`: done 2026-10-09 (`rar_times.sh`, 73 lines, twelve kinds and
     precisions, the archives WinRAR's byte for byte; PowerShell sets the times first).
     One precision a file: whole seconds only when every time kept asks for them, kept
     then as 4-byte Unix seconds (rars' `FileTimestamp::UnixSeconds`, so such a record
     also reads and is carried as it was); else all at full precision. lt now shows
     "Created" and "Accessed". RAR 1.5 to 4's creation and access times: done
     2026-10-09 (in `rar_times.sh`, now 132 lines). WinRAR 7.23 writes no RAR 4, so
     rars wrote one with them in its extended-time field (cash's fixture
     `times_rar4.rar`): Rar.exe lists them as "Created" and "Accessed" and restores
     them with `-tsc`, `-tsa` and `-ts`, local DOS times to the 100 ns; rars decoded
     only the modification time. rars' RAR 1.5 to 4 header has `ctime` and `atime`.
   - `-df`, `m` and `mf` with a directory left holding files left out ("NOT DELETED",
     "Cannot delete ... The directory is not empty."), and `-r` with a plain file name,
     looked for in every directory below its own as a mask is (not with `-r0`, and a name
     found nowhere is then no error): done 2026-10-09 (`rar_add_names.sh`, 152 lines;
     the first was already rar's, the second is new).
   - The locator's padding when an archive is changed: done 2026-10-09 (in
     `rar_add_names.sh`, now 326 lines). WinRAR's bound counts the old archive's
     members, dropped ones too, and for `a`, `u` and `f` every file named, written or
     not; cash counted the members written, so an update or `d` near a width's edge
     came out a byte short (rars' `Layout::offset_bound`, given by cash). An old member
     counts by its packed size (done 2026-10-09, in `rar_add_names.sh`, now 391 lines:
     seen with compressed members near a width's edge); and a compressed member copied
     has its data size written as wide as its unpacked size takes, a stored one's not
     padded, where cash wrote both unpadded.
   - A file another program has open for writing: done 2026-10-09 (in
     `rar_add_names.sh`, now 192 lines). Rar.exe cannot open it, as it opens what it
     adds letting no one write ("Cannot open NAME / The process cannot access the
     file because it is being used by another process", then "WARNING: Cannot open N
     files", rc 6), unless `-dh`; cash read it, and its writer stopped on "entry source
     size changed while reading". Names not there now give rc 10, as rar's do, not 1.
   - Volume sets with `-rr`, `-p` and `-hp`: done 2026-10-09 (`rar_volumes.sh`, 130
     lines). They kept rars' own payload-sized layout, so a set with a recovery record
     or encrypted headers came out larger than the size asked; now WinRAR's: each
     volume's recovery record after its quick-open one, covering what is before it and
     sized from the room left, and `-hp` headers encrypted in every volume, no
     quick-open record. The quick-open room kept for a file going in is its header and
     18 bytes, for each earlier one in the volume its header and 16 (seen in twelve
     `-rr` sets and a `-p` one, all WinRAR's to the byte). A volume's number has the
     digits of the volumes foreseen from the bound, as Rar.exe's. A directory among
     encrypted files has no encryption record, in volumes and single archives alike, as
     WinRAR writes it (it was 49 bytes larger).
   - `-hp`'s quick-open block: done 2026-10-09 (`rar_volumes.sh`, now 162 lines;
     `rar_modify.sh`). WinRAR keeps one under encrypted headers, in one archive and in
     each volume: the cached headers as they lie encrypted, encrypted again with the
     headers' key under an encryption record of its own (the archive's salt, an IV, a
     password check of eight zero bytes and their SHA-256's first four), padded to 16
     bytes, the padded length its size. In a volume it is kept 16 bytes more room, and
     an encrypted recovery header 54 (it takes 48). The locator's offsets count from
     the main header, after the encryption header: cash's counted from the signature,
     38 bytes off under `-hp`. A split part's encryption record does not say its
     checksums are keyed to the password (only the last part's does, and none under
     `-hp`): WinRAR's `t` called every part of cash's `-p` sets "packed data checksum
     error". cash refused to change an archive with an encrypted quick-open block, so
     WinRAR's `-hp` archives could not be updated; it changes them now, its own too.
   - A change drops the recovery record unless `-rr` asks again: done 2026-10-09
     (`rar_modify.sh`, now 607 lines). `a`, `u`, `m`, `d`, `c`, `rn`, `k` and `ch` all
     write the archive anew without one, as WinRAR does; cash kept it. `-rr` on `d`,
     `k` and `c` adds one, its line before "Locking archive" for `k`. `ch -tl` alone
     leaves the archive's bytes alone and sets its time only; `ch -k` says "Locking
     archive". The locator's padding when changing counts the old archive's recovery
     and quick-open records as members too (a byte short before).
   - Writing, not yet in the oracle or not yet WinRAR's: (updating
     an archive whose files are encrypted without its password done 2026-10-09, in
     `rar_add_names.sh`: a non-solid RAR 5 archive's members are carried by rars'
     new `carrying_builder`, which asks the password only for encrypted headers and
     comments; a solid one's, read back and written again, still need it); (`-log` done 2026-10-09, `rar_log.sh`: A, F, P, U
     for a, x, t, l, lb and d, in UTF-8 where Rar.exe writes ANSI unless -sc...g says; `-ag` done 2026-10-09,
     `rar_agname.sh` and unit tests from the names Rar.exe gave; `-as` done
     2026-10-09: what no name gives taken out, its "Deleting" lines in their places; and
     `-dw` and
     `-dr` done 2026-10-09: `-dw` in `rar_add_names.sh`; `-dr`, a directory archived whole
     recycled whole and lines without a share, compared with Rar.exe by hand, out of
     the oracle not to fill the Recycle Bin);
     `-md` done 2026-10-09 (in `rar_add_names.sh`): the dictionary asked
     (32 MB without), halved while the largest file, or all in a solid archive, fits
     twice, to 128 KB (1 MB solid), as the headers Rar.exe writes record it; rars' builder's
     `rar50_dictionary_size` is public now. Sizes above 4 GB (RAR 7's) are not tried.
   - `c`, `cw`, `rn`, `k`, `rr` and `ch`: done 2026-10-09 (`rar_modify.sh`, 474 lines then,
     the rewrites WinRAR's byte for byte). Left of them: `ch` takes `-cl`, `-cu`, `-z`,
     `-k`, `-tl`, `-rr`, `-ts`, `-ams`, `-amr`, `-qo` (its three forms, seen WinRAR's
     byte for byte on 2026-10-09) and `-hp` (below). `-ep`, `-htb`, `-s`, `-p` and
     `-df` change nothing there; `-ma4` is refused as by cash.
   - `-hp` on the commands that change an archive: done 2026-10-09 (in
     `rar_passwords.sh`, now 791 lines, by size and details, the salts being
     random). `a`, `u`, `f`, `m`, `d`, `c`, `k`, `rn`, `ch` and `rr` encrypt a plain
     archive's headers with it; cash did in `a` alone. An archive whose headers are
     plain and some files encrypted is refused: "Cannot change the header encryption
     mode in already encrypted archive", rc 2, after `a`'s "Updating archive" and
     `c`'s comment read, before `k`'s "Locking archive". A comment written under
     encrypted headers is encrypted as a file is, with the headers' salt and key, its
     own IV and the password's check, zero-padded to 16, the padded text's CRC its
     own; cash stored it plain inside them (60 bytes short). A comment carried keeps
     its form (rars' builder's `comment_kept_plain`). An encrypted file is never
     stored for want of compression: Rar.exe packs seven bytes into 32 with `-m3`
     where cash stored them (rars' compression plan's `keep_method`). With `-idq`,
     "Program aborted" is hidden, as a message; a run that stops leaves standard
     error's line as it is, and one that ends ends standard error with a line's end
     whenever it wrote there, an ended line getting another.
   - Passwords typed, and archives that do not open to be changed: done 2026-10-09
     (`rar_passwords.sh`, 676 lines then). A bare `-p` or `-hp` is asked for as rar reads
     it, before its banner and the other switches' words, unless a password came
     before it (`-hp` takes `-p`'s; `-p` asks after `-hp`'s); cash asked after its
     banner. A password from a pipe is all one read gives, its line ends trimmed
     (`x\ny` is wrong). After a password typed for the headers or a file, no line
     ends. A wrong one typed is "The specified password is incorrect." and asked for
     again until it is right or the input ends; cash said "Incorrect password". After
     a password typed for a file, each encrypted file asks "NAME - use current
     password? [Y]es, [N]o, [A]ll" (No asks for another, All stops asking and makes a
     wrong one an error). The end of the input at a question for a file's or the
     headers' password says "The pipe has been ended." for a pipe. `-p-` asks nothing
     and finds encrypted headers' password wrong. `c`, `k`, `rn`, `ch` and `rr` say
     "Processing archive" before the headers' question, and of a wrong password
     "Incorrect password for X / ERROR: Bad archive X", rc 11; `a`, `u`, `f`, `m`, `d`
     and `cw` stop with "Incorrect password for X" and "Program aborted", rc 13 (Bad
     archive, in Rar.txt). A file that is no RAR archive: "ERROR: Bad archive X" with
     "Program aborted", rc 13, for `a`, `u`, `f`, `m`, `d` and `cw`; with "Processing
     archive" and Done, rc 0, for the others that change; "No files to extract", rc
     10, after `x`, `e`, `t` and `p`'s "is not RAR archive"; nothing from `lb`. `r`
     on these: done 2026-10-09 (in `rar_repair.sh`, now 373 lines). It asks no
     password: headers it cannot read, encrypted or no archive's, are searched for a
     recovery record by its chunks' marks (rars' `recovery_by_marks`), which mends
     them; without one, encrypted headers give "No files found", rc 10, and a file
     that is no archive a second search, "Reconstructing", "Building", corrupt-header
     and "No files found" words, rc 10, and nothing written. Encrypted headers are
     never rebuilt, read or not. The order of switches: done 2026-10-10 (in
     `rar_passwords.sh`): rar asks at a bare `-p` before reading an `-inul`, `-idq`
     or `-ierr` after it, so the question and "Program aborted" heed only those
     before; cash's heeded all.
   - `-ams`, `-tl` and `ch -amr`: done 2026-10-09 (in `rar_modify.sh`, now 981 lines,
     WinRAR's byte for byte but a recovery record's bytes). `-ams` saves the archive's
     file name and the time it is written (with `-tl` its newest file's, directories left
     out) in its main header, for `a`, `u`, `f`, `m`, `d`, `c`, `k`, `rn`, `ch` and `rr`;
     cash saved neither. `-tl` sets the archive's own time to its newest file's after
     every one of them; cash did it for `ch` only. `ch -amr` sets the archive's
     creation and modification times to the saved one, then renames it to the saved
     name ("X is renamed to Y"), asking "Overwrite NAME?" with Yes, No, All, nEver and
     Quit when the name is taken (`-o+` and `-y` replace, `-o-` leaves, `-or` fails as
     Rar.exe's does: "Cannot rename X to NAME(1).rar", rc 0); the file replaced is
     deleted first, so the archive takes its creation time as Windows gives it; other
     switches are ignored. Rar.exe deletes the archive itself when the saved name
     differs only in case; cash renames it (in "Found along the way"). `lt`'s
     "Original time" now reads a saved time stored as Unix seconds or nanoseconds too.
     `rn` of two files to one name kept the first only and said "duplicate archive
     entry name"; rar keeps both under the name, and so does cash (rars' builder's
     `rename_by_id` honours `allow_duplicate_names`).
   - A change stores the kept files' times again by `-ts`: done 2026-10-09 (in
     `rar_times.sh`, now 118 lines, WinRAR's byte for byte). `a`, `u`, `f`, `m`, `d`,
     `c`, `k`, `ch` and `rr` write every member kept with the times the switches keep,
     at their precision: by default the modification time alone, its creation and
     access times dropped; `rn` keeps them as they are. cash carried them as they were.
     rars' builder gains `last_entry_id` and `set_mtime_by_id`.
   - `c`, `k`, `rn`, `ch` and `rr` on a volume: done 2026-10-09 (in `rar_modify.sh`, now
     757 lines, WinRAR's byte for byte but a recovery record's bytes). cash refused a
     volume ("Cannot modify volume"); rar changes the volume named by itself, whichever
     it is, the others left as they are (`rn` renames that volume's part only), and
     writes it without its zero fill; `d` it refuses still. rars' builder writes one
     volume of a set (`volume_of`: its number, and more to follow in its end header),
     carrying split parts, and rewrite's `volume_builder` sets it up; a volume's zero
     fill and "more volumes" end flag no longer count as unsupported. `t` now tests a
     volume's recovery record as it leaves that volume, where it tested them all at the
     end.
   - Recovery records above 100%: done 2026-10-09 (in `rar_modify.sh`, now 639 lines).
     cash took anything above 100 as the default 3%; now up to 1000%, more recovery
     shards than data ones, as WinRAR writes them; `-rr0` none; above 1000 rar's
     "Adjusting -rrN value to 1000." on standard error and, as Rar.exe was seen to,
     200%'s record. A service's sizes take room for twice them and a kilobyte, without
     a file's five bytes from a mebibyte on (a 1.2 MB record took four). Reading,
     testing, repairing and changing such an archive work. With `-idq`, rar ends
     standard error's open line at the end ("Cannot open ..." too); cash does now.
   - `i`: done 2026-10-09 (`rar_find.sh`, 264 lines then). Its tables and a solid
     archive's names: done 2026-10-09 (`rar_find.sh`, now 369 lines). `t` looks in the
     OEM code page too, whose control codes show as the console's pictures (`◙` for a
     line feed); ANSI is Windows' code page (1252: 0x82 a low quotation mark), where
     cash read Latin-1; letters fold in every table (`CAFÉ` finds UTF-8's `café`). A
     file named in a solid archive: each file decoded on the way writes over a
     progress's place, five backspaces and five spaces, as Rar.exe does.
     `cash_win32::codepage::decode_glyphs` decodes with the console's pictures.
   - `r`: done 2026-10-09 (`rar_repair.sh`, 253 lines; `t` now tests a recovery record
     by its own chunks' checksums, and finds an archive "ended early" only where its
     locator points at a missing quick-open record, as rar does). Damage the record
     cannot mend: done 2026-10-09 (in `rar_repair.sh`): each damaged block "cannot
     recover data", "0 blocks are recovered", and the archive rebuilt instead; cash
     said nothing of them and wrote nothing. A recovery record whose header fails its
     checksum: done 2026-10-09 (in `rar_repair.sh`). Rar.exe reads on past a service
     header as past a file's, "RR - the file header is corrupt" in `l`, `t` and `x`
     (two errors), the record still tested by its chunks; rars' lenient read now keeps
     it, where it ended the walk ("Unexpected end of archive"). `r` mends with it and
     leaves it out of `fixed.NAME`, which ends where it began with an end header, as
     Rar.exe's does byte for byte. A damaged main header: done 2026-10-09 (in
     `rar_repair.sh`, now 494 lines): a record looked for by its marks, then
     "Main archive header is corrupt" twice and "The archive header is corrupt. Mark
     archive as solid? [Y]es, [N]o" (Yes alone says solid; `-y` does not answer; the
     input's end aborts, rc 12), the new main header `03 01 04` with the solid flag or
     none, the rebuilt archive Rar.exe's byte for byte; cash wrote header flags 0 and
     asked nothing. Left of it: RAR 1.5 to 4 rebuilds, unobserved (WinRAR 7.23 writes
     no RAR 4 to damage).
   - `rc`: done 2026-10-09 (`rar_reconstruct.sh`, 249 lines, RAR 5 and both RAR 3
     namings, the volumes rebuilt WinRAR's byte for byte); `s` and `rv` refused in
     words of their own. Left of it: a RAR 3 volume that is there but damaged (its
     recovery volumes keep no checksum of it; what rar does then is unobserved), and
     old-style volume names (`NAME.rar`, `NAME.r00`: WinRAR 7.23 names RAR 5 volumes
     `.partN` even with `-vn`).
   - The `expect(dead_code)` at the top of `rar/mod.rs`: gone 2026-10-09, with the
     items it covered that nothing used; extraction now restores the creation and
     access times `-ts`, `-tsc` and `-tsa` ask for, and `-tsm-` leaves the modification
     time as written, as Rar.txt says.
3. **The page, `builtins.md`, doctor** (done 2026-10-09): `help rar` covers both names;
   `CARRIED` and doctor's note on a program cash goes ahead of have `rar` and `unrar`,
   so a WinRAR on `PATH` is named with the way to reach it; 7z's page already says it
   reads RAR.
4. **rar's spool files outlive a killed process**: done 2026-10-08. Spools and reader
   scratch files are opened delete-on-close, a spool keeping that handle while it lives,
   and a writer with no temp directory named spools in the system's, not the process's
   working directory. `rar a` passes `-w`'s directory when given.
5. **rar's AES key schedules are not wiped**: done 2026-10-10, the user's pick (a
   download of `zeroize`, in the end found in cargo's cache). The `aes` crate's
   `zeroize` feature is on, so `Aes128`/`Aes256` wipe their round keys when dropped,
   for zip and 7z as well as RAR; the passwords, keys and IVs stay `crypto::wipe`'s.

---

## Phase 34. The prompt, paths, and a guard on size and speed

Asked by the user on 2026-10-10, after 1.11.0, on what they like most: small and fast,
every way of writing a path working, and the prompt's helpers. Design in
`research/prompt-history-and-jump-design.md`; the choices made by pick list the same
day (spec D79, D80). In order:

1. **Ctrl-R: a history picker** (D79, done 2026-10-10): Alt-E's inline list over the
   history, fuzzy, newest first, Enter to the line and Tab to run, Ctrl-R again for what
   ran in this directory (a directory record of cash's own beside the history file,
   `%LOCALAPPDATA%\cash\history-directories`; `CASH_NO_RECORDS` keeps none, and the tests set
   it). Keys typed straight after Ctrl-R or Alt-E now reach the picker, not the line
   after it (reedline patch 10). The directory list shows only commands still in the
   history, so `history -c` empties it too. `reverse-search-history` is Reedline's search
   again, which `bind` can put back on Ctrl-R. Help: `keys`, `history`, `vars`,
   `differences`, F1, README.
2. **`z`: the directory jump** (D80, done 2026-10-10): every change of directory ranked across
   sessions as zoxide ranks, `z WORDS`, `z -l`, `z -i`, Tab, and Alt-E's Alt-H (and
   `croot`'s) on the same record, `%LOCALAPPDATA%\cash\directories`. A gone directory is
   skipped and forgotten after 90 days without a visit, as zoxide does, rather than at
   once (an unplugged drive keeps its directories). Tab replaces all the words typed, since
   a full path would not match them in turn. Tests: `it/z_jump.rs`, and two sessions in
   `conpty_z_jumps_to_directories_a_session_before_went_to`.
3. **Every way of writing a path, everywhere** (done 2026-10-10): `it/path_spellings.rs`
   runs a directory named `my dir` in ten spellings (`C:/`, `c:/`, `C:\`, mixed, `/c/`,
   `~/`, relative, `./`, `//localhost/C$/…`, `\\localhost\C$\…`) through 17 places
   (`cd`, `pushd`, `cdh`, `CDPATH`, `z`, `<`, `>`, `>>`, `source`, `test`, `ls`, globs,
   a script by its path, `PATH`, `cat`, `git`); Tab and Alt-E's start have tables of
   their own. Fixed: a glob whose fixed start is `/c/…`, `/tmp/…` or UNC matched
   nothing (now read as `cd` reads it, matches in `C:/` spelling); Tab on a word typed
   with an open quote and backslashes (`'C:\x\in`) completed nothing, and Tab gave
   `C:/` for a word typed `C:\` (now kept); Alt-E took a `C:\…` word's backslashes for
   escapes. `winpaths` now covers `\\server\share` too (the user's pick, spec D53).
   `/c/…` given to a program stays as written, with the warning, as documented.
4. **A guard on size and speed** (done 2026-10-10): `cargo xtask perf` measures a
   build's size and cash's own start (median of 30 `cash -c true` less that of a bare
   `cmd /c exit`) against `perf-budget.toml`: 1.11.0's 48,838,656 bytes plus 2 MiB, and
   60 ms. The release workflow runs it on the dist build before packaging, the figures
   in the job summary; over a limit stops the release. The local release build of
   2026-10-07: 50.8 MB, 27.9 ms.
5. **`rm -rf` on junctions** (done 2026-10-10), from "Found along the way": a junction
   inside a directory gave "Permission denied", and one whose target went first failed
   too. `rm` (cash's own since item 6, from `uu_rm` 0.12.0) now takes `RemoveDirectoryW`
   where it removed a directory link as a file (`DeleteFileW`): the link goes, never
   followed. Tests: `it/rm_links.rs` (junctions to
   a directory inside and outside, and a dangling one). `rar_add_links.sh` still takes its
   junction away with `cmd /c rmdir`, which it may now leave to `rm`.
6. **The five patched uutils tools become cash's own code** (the user, 2026-10-10, by
   pick list; done the same day): `rm`, `tee`, `sort`, `uniq` and `shuf` moved from
   `vendor/uutils` into `crates/cash-uutils`, licence kept, credited in NOTICE, the
   changes in its README; `vendor/uutils` is gone. Their Unix, Linux and WASI code went.
   Each reads its own `en-US.ftl`, compiled in, which fixes the message ids a release
   printed (`it/uutils_messages.rs`). The workspace lints apply, the style ones allowed
   as for `cash-sed`, the panic ones in full: about 50 sites, each an error the tool
   now reports or a stated reason it cannot fail.
7. **reedline becomes cash's own code** (done 2026-10-10): `vendor/reedline` moved to
   `crates/cash-reedline` (`git mv`, its history kept), the package `cash-reedline`,
   its library still `reedline`. The features cash never turned on went with their
   code and dependencies (clipboard, bashisms, SQLite history, external printer, idle
   callback; 1,400 lines, `arboard`, `rusqlite`, `crossbeam`). The workspace lints
   apply: style ones allowed, panic ones met (history errors returned, a `todo!()`
   that could crash the prompt gone, `Menu::settings` required); string slicing at
   positions the editor keeps on boundaries is stated per module. Its 1,548 tests run
   with the workspace's; the patch list is the README's "changes made while it was a
   patched copy". Left for later: Helix mode (on by default, so compiled in, never
   offered by cash; its code runs through the core editor) and the 2021 edition.
8. **crossterm becomes cash's own code** (done 2026-10-10): `vendor/crossterm` moved to
   `crates/cash-crossterm`, keeping crossterm's name and version so the `[patch]` still
   gives `uu_more` cash's copy. Its Unix code and the `event-stream` and `osc52`
   features went (with `libc`, `rustix`, `mio`, `signal-hook`, `futures-core`,
   `base64`); the workspace lints apply, every unsafe block with its reason; its tests
   run with the workspace's, and `vendor/` and xtask's vendored-test step are gone.
   Left for later: the 2021 edition, as for reedline.

## Phase 35. Small fixes, `command_not_found_handle`, and `sudo` in this terminal

Asked by the user on 2026-10-10, after 1.12.0: the small items of "Found along
the way", Bash's `command_not_found_handle`, then a start on `sudo` elevating in
the terminal it was typed in. Each item as TODO.md had it, then what was done.

1. **`pwd -W` is refused though the usage offers it** (found 2026-10-07): `pwd -W` says
   "-W: invalid option" then "usage: pwd [-LPW]". MSYS2's Bash has `-W` (the Windows
   path); either take it (cash's paths are Windows paths already) or drop it from the
   usage.
   Done 2026-10-10: `pwd -W` is taken and prints what `pwd` prints, with `-L` and `-P` as
   before. `pwd -P` printed the resolved directory as `\\?\C:\…`; it is `C:/…` now, as every
   path cash prints. Tests: `it/pwd_forms.rs`.

2. **`sort` never asks Windows how much memory there is** (found 2026-10-10, making it
   cash's own): uutils sizes its buffer from `sysinfo` on Linux and gives up elsewhere,
   so on Windows the buffer is always the fixed default. `GlobalMemoryStatusEx` (through
   `cash-win32`) would let a big sort use more memory and fewer temporary files;
   `physical_memory_bytes` in `crates/cash-uutils/src/sort/buffer_hint.rs` is the place.
   Likewise `tee -i` ignores no Ctrl-C on Windows (uutils' is Unix-only).
   Done 2026-10-10: `sort` takes a quarter of the memory Windows can hand out
   (`cash_win32::memory::counts`, `GetPerformanceInfo`, as `free` reads it), else of all of
   it, as uutils takes `MemAvailable` on Linux, within its 512 KiB to 1 GiB; the place
   named above is gone. `tee -i` ignores Ctrl-C through a console handler, which programs
   it starts do not inherit (`cash_win32::console::ignore_ctrl_c`). Tests: a unit test in
   `buffer_hint.rs`, and `ctrl_c::tee_i_lives_through_ctrl_c`, where `tee -i` keeps what
   the shell before it writes after a Ctrl-C ended its `ping`.

3. **A `TEMP` in 8.3 short form is not under the home directory** (found 2026-10-10 by CI,
   run 38043418786): Windows sets `TEMP` to `C:\Users\RUNNER~1\AppData\Local\Temp` for
   a long user name, while `HOME` is `C:/Users/runneradmin`. A directory under `TEMP` then
   shows whole in `z -i`, the pickers and `tilde_shorten`, never as `~/…`, and two
   spellings of one directory count as two in the records. Taking `TEMP` and `TMP` to their
   long form (`GetLongPathNameW`) as cash starts would mend both; check what a program
   started by cash then gets, and Git Bash's own `TEMP`.
   Done 2026-10-10: cash takes `TEMP` and `TMP` to their long form (`GetLongPathNameW`,
   `cash_win32::path::long_form`) first thing in `main`, so `/tmp`, its temporary files,
   `$TEMP`, `$TMPDIR` and the programs it starts all see the long form. Git Bash shows
   `$TEMP` as `/tmp`, which `pwd -W` gives in long form, and hands programs the short form
   it was given; the long one names the same directory. Test: `it/long_temp.rs`, a directory's
   8.3 name as `TEMP`, under a `HOME` in long form.

4. **CI's annotations show four `ENOENT ... opendir 'D:\a\cash\cash\target\doc\cash_core\tests\trybuild'`**
   (and `...\tests\target`) errors on every green run, 1.8.0's and 1.8.1's included
   (seen 2026-10-06). The job passes; the cleanup of the Rust Cache step
   (`Swatinem/rust-cache`, `.github/workflows/ci.yml`) walks `target\doc` and trips on
   directories that are gone. Find what makes `target\doc\cash_core\tests` and keep it out of
   the documentation, or exclude `target\doc` from the cache.
   Done 2026-10-10: `cash_core::tests`, the `test` builtin's expressions, was a public
   module named `tests`, whose rustdoc directory the cache's cleanup took for a build directory
   and looked for `target` and `trybuild` in; it is `cash_core::test_expr` now. As a
   restored cache still holds the old directory, the quick job removes `target\doc` before
   the cache is saved. Found in the same annotations: crossterm's console tests
   (`*_winapi`) each make a screen buffer the console's active one, and two failed on a
   first try in CI; their `#[serial]` orders one process only, so they share a nextest
   group of one (`console-buffer`).

5. **Ten oracle tests keep their own copies of the oracle helpers** (found 2026-10-07,
   phase 28): `oracle_dir`, `run_oracle_script`, `golden` and `with_divergence` are
   written out again in `gzip_builtin.rs`, `grep_builtin.rs`, `column_builtin.rs` and
   seven more under `crates/cash/tests/it`. `common.rs` has them now, and the bzip2, xz
   and zstd tests use those; move the ten onto them.
   Done 2026-10-10: nine files still had copies (`clipboard_and_small_tools`, `column`,
   `flock`, `grep`, `gzip`, `hexdump`, `iconv`, `tput`, `xxd`), each the same as
   `common.rs`'s; they use those now, `with_divergence` with its count of 1. 288 lines
   fewer.

6. **cash never runs a user's `command_not_found_handle`** (found building the install
   hint, 2026-10-06). Bash calls that function, when defined, with the command and its
   arguments instead of printing `command not found`, and its status becomes the
   command's. cash prints the message regardless; the hint is suppressed when the function
   is defined, as decided, but the function itself does not run. Implement Bash's
   behaviour, with the hint only when no function is defined.
   Done 2026-10-06, the same day, in phase 25 (942abad4), with its tests
   (`it/command_not_found_handle.rs`, and `install_hint.rs` for the hint); this item stayed
   behind. Checked again 2026-10-10 against Git Bash 5.3: the handler's arguments and
   status, in a substitution and a pipeline, and `command not found` with 127 once it is
   unset, are the same.

7. **Candidate: `sudo` elevating in this terminal by itself** (asked for by the user on
   2026-10-10, not yet chosen): `sudo` is cash's builtin, but the elevation is handed to
   gsudo, else Windows' own `sudo` (Windows 11, once turned on), else a plain UAC
   request, which runs the command in a new window that cash does not wait for: no
   output here and no status. In-terminal elevation is what gsudo adds: an elevated
   helper that attaches to the caller's console. cash could do it itself: start an
   elevated copy of `cash.exe` through UAC with a hidden helper argument, which
   `AttachConsole`s to the calling cash's console (or relays it over a pipe), runs the
   command there and returns its status, so `sudo CMD` needs no other tool. To find
   out, by observation: whether an elevated process may attach to an unelevated
   console on Windows 11 (gsudo's "attached" mode says so); Ctrl-C and the console
   modes across the two; how the job object (spec D42) sees the elevated helper; and a
   credentials cache, as `sudo -v` gives with gsudo, if it is wanted at all. Design
   with a pick list before any code.
   Done 2026-10-10, designed in `research/sudo-in-terminal-design.md` with the user's
   picks: cash's own elevation, always (gsudo only for `-u USER` and its cache), and the
   command reads the terminal. `sudo` starts `cash --invoke-bundled --sudo-elevate`, an
   ordinary program to the shell, which asks UAC (`ShellExecuteExW`, `runas`, its window
   hidden) for `cash --invoke-bundled --sudo-attach`; that one attaches to the caller's
   console, takes its other standard handles with `DuplicateHandle`, changes to its
   directory, ends when the caller ends, and runs the command under `--sudo-owner` as before.
   Declined, `sudo: not run: the request to run it as an administrator was declined`, 1.
   Windows' sudo and UAC's new window are no longer used by `sudo`; `sudo -n` fails unless
   elevated; `cash doctor`, `sudo -l`, the help pages and the spec say so. Seen by hand,
   with the user at UAC's prompts: the status, the console, the directory, files in and out,
   Ctrl-C, and keys (`it/sudo_in_terminal.rs`, ignored in the suites as they ask UAC).
   Not built: the environment through a handle; `-E` still passes `NAME=value` words.

8. **`sudo -u USER` and `su USER` without gsudo** (the user, 2026-10-11, by pick list;
   built and seen with a test account the same day): cash asks USER's password at the
   console, echo off, then starts `cash --invoke-bundled --sudo-owner - COMMAND` as USER
   with `CreateProcessWithLogonW` (`research/sudo-in-terminal-design.md` section 8). Unlike
   the elevated side, USER's cash cannot open the caller or attach to this console (both
   `Access denied`), so a redirected handle is handed over by inheritance
   (`STARTF_USESTDHANDLES`) and a console handle is relayed: USER's cash reads and writes a
   pipe, the caller relays the bytes to and from the real console on threads of its own, as
   gsudo does. It is put in a kill-on-close job before it runs, so a shell killed mid-run
   takes it too (seen); and USER is granted the window station and desktop while the command
   runs (`winstation.rs`), put back after, so a windowed program can show. When the shell's
   directory is under the caller's profile, which USER cannot enter, the command runs from the
   system temp instead, with a note. gsudo and `runas` are no longer used; `cash_win32::gsudo`
   is gone; `sudo -v`, `-k` and `-K` are kept for scripts with no cache behind them. Tests:
   `sudo_in_terminal::ctrl_c_at_the_password_prompt_runs_nothing` (in the suites),
   `::sudo_u_runs_the_command_as_the_account`, `::sudo_u_shows_console_output_in_this_terminal`
   and `::sudo_u_dies_when_the_shell_is_killed` (ignored; a second account and its password).

9. **`net user NAME * /add` fails: the star becomes the directory's files** (the user,
   2026-10-11, by pick list; built the same day): a line that works in cmd and PowerShell
   failed in cash as it fails in Git Bash. `shopt winglob` (D81), on at the prompt and off
   in scripts like `winpaths`: when the command resolves to a program under `System32` or
   `SysWOW64`, whose programs read `*` and `?` themselves since cmd never globs for them,
   the words after it are not pathname-expanded; `sudo`, `command`, `exec`, `nohup`,
   `nice`, `env` and `timeout` are looked through to the command they run, each by its
   own parsing (`Registration::command_operand`). Tests: `it/winglob.rs`.

---

## The phase after 1.24.0 and 1.25.0: the DLLs, busy jobs, and the corpus

The user's picks after 1.24.0 and 1.25.0 (decisions.md), released as 1.25.0 and 1.26.0;
each item's entry is under "Found along the way, done" below.

1. **Command completion offered DLLs**: done, 1.25.0.
2. **Busy background jobs starved the foreground**: done, 1.25.0, taken in when
   1.24.0's release gate timed out on it four times.
3. **Grow the differential corpus**: four rounds, 1.26.0. 47 new cases: pure-bash-bible
   and pure-sh-bible (MIT), semver-tool (Apache-2.0), JSON.sh (MIT), spark (MIT),
   getoptions (CC0), and seven of cash's own on the Bash manual, with
   `tests/corpus/SOURCES.md` for every source's terms. They found eight differences,
   all fixed: quoted alias names, aliases in aliases and after a blank, a subshell's own
   traps, `trap -p`'s listing, `.` taking the script's options, its usage's name,
   `GLOBIGNORE`, and a last `**`. Three were left for the next phase.

## The phase after 1.26.0: reading a command at a time

The user's picks after 1.26.0 (decisions.md), released as 1.27.0; each item's entry is
under "Found along the way, done" below.

1. **Two small fixes**: `GLOBIGNORE` in `shopt dotglob`, and the clipboard tests that
   put back only text.
2. **extglob on from the start**: off now, `[[`'s patterns extended either way.
3. **Aliases with operators**: expanded on the tokens before parsing; a script read a
   command at a time, and the rest parsed whole again once a command leaves the
   options as they were.
4. **More corpus rounds**: one, mo, ansi, nvm and the alias case.

Also in 1.27.0: "directory", never "folder", everywhere (the user); a `case` of many
options matched without a regular expression per pattern, which held the release when
the ansi case timed out under load; `croot`'s top line keeping its mode in sight.

## Found along the way, done

- **cash's `printf` writes a line's end in a write of its own** (found on 2026-10-09
  with `rar_passwords.sh`): done 2026-10-10. `printf 'v\n'` into a pipe whose reader
  was waiting gave it `v`, then `\n`, and Rar.exe took the empty line for a cancelled
  password. Each pass of the format is now held and written at once, as bash's printf
  writes it (a Python reader sees `v\n` whole; unit test in `printf.rs`).
- **WinRAR matches an archived name typed with `/` to nothing** (found on 2026-10-09
  with `rar d`): Rar.exe 7.23 on Windows takes `\` alone between directories in the names
  after the archive's, so `t`, `p`, `e`, `x` and `d` given `src/a.txt`, `*/a.txt` or
  `src/*` say no files and return 10, where `src\a.txt` works. cash's rar takes both,
  which in a shell where `/` is typed is what a user means. Kept so, the user's pick
  on 2026-10-10; the oracles use `\`.
- **`ch -amr` on a saved name differing only in case deletes the archive in Rar.exe**
  (found on 2026-10-09): `M2.RAR` saved as `m2.rar` is asked about as taken (it is
  itself), and on Yes or `-o+` Rar.exe deletes it, then cannot rename it ("Cannot
  rename M2.RAR to m2.rar / The system cannot find the file specified.", rc 0). cash
  renames it instead, a difference kept on purpose and out of the oracle.
- **Rar.exe's recovery record of a tiny archive differs between runs** (found on
  2026-10-09): `rr` on the same 156-byte archive twice gave chunks with different
  checksums and parity, the data shards' checksums alike; what it pads a short shard
  with seems not to be zeros. Larger archives' records are the same each run, and
  cash's match them; the oracles show a small archive's record by its size.
- **`rm -rf` stops at a junction** (found on 2026-10-09 with `rar_add_links.sh`; done
  2026-10-10, phase 34 item 5): a directory holding a junction gave "rm: cannot remove 'top\junc': Permission denied",
  rc 1, in 1.10.0 too. It does not follow the junction (the target's files survive); it
  fails to take away the link itself, which a directory link needs `RemoveDirectoryW` for.
  `rm` is uutils' `uu_rm` 0.12.0 from crates.io: the fix means a near-upstream copy in
  `vendor/uutils` (as `uu_sort` and the others), or seeing whether a newer uutils has it.
  The oracle takes its junction away with `cmd /c rmdir` meanwhile.
- **`seven_z_matches_7_zip_26_03` failed twice** in nextest runs of the twelve 7z tests
  on 2026-10-07, each right after a build, and passed alone and in seven runs after,
  three of them beside the other three oracle scripts; the difference was not caught.
  Run it with `--failure-output final` until it fails, and see what moved.
  Settled 2026-10-10: `7z u` took `d/` as changed when the directory's time fell
  near the end of a second; the directories now get whole seconds (see the 1.12.0
  gate item).
- **7z asks for a password before extracting, 7-Zip when it gets to the first
  encrypted item** (found on 2026-10-07 with RAR, true of zip too): with no password to
  type, 7-Zip has written the items before it and leaves that item's empty file. In the
  page's notes; asking at the item would mean handing the password to a decoder already
  under way.
- **The first cash window after a Scoop update is blank for 1 to 3 seconds** (the user,
  2026-10-06); after that every start is fast. Measured here: a copy of `cash.exe` with
  a changed hash takes 1.2 to 2.3 s on its first run and about 50 ms after, which is
  Defender's cloud check of an unknown unsigned binary, cached by hash; 130 hard links
  to it, and a start in a new console, do not bring the check back. A forced reinstall
  of the same version (same hash) shows no delay. So the check is the likely cost, but
  the install's own three runs of the new exe (`--terminal-profile`, `--link-tools`,
  `--init-rc --once`) should already pay it, and cash keeps nothing per version (no
  cache on disk; checked). To settle: at the 1.5.0 update, time the first `cash -c true`
  and the first `cash -ic true` right after `scoop update cash`, then bisect. The
  lasting fix is code signing (ROADMAP 18, SignPath); a Defender exclusion needs admin.
  **Measured at the real 1.4.4 → 1.5.0 update (2026-10-06, idle machine):** first
  `cash -c true` 51 ms, first `cash -ic true` 76 ms, the second 69 ms. The shell's own
  first start after an update is as fast as any other, so the blank first tab is not
  cash's: it is Windows Terminal's first launch of a profile whose fragment was just
  rewritten, or Defender in Terminal's launch context. Left as is (the user: "we live
  with it"); if it is ever pursued, time a Terminal tab, not the shell.
- **The Scoop update output** (the user, 2026-10-06): the warning and the process table
  are Scoop's own (`test_running_process`, printed whenever a cash runs from
  `scoop\apps\cash`, which the window typing `scoop update` always does); only the notes
  are cash's. They now say the listed cash is that window; the bucket has them
  (pushed 2026-10-07).
- **`rm -rf` fails on a junction whose target it removed first** (found 2026-10-07,
  phase 31; done 2026-10-10, phase 34 item 5, though `rm`'s other messages still name
  a path inside the directory with `\`): in `t/` holding `sub/` and `jn` (a junction to `sub`), `rm -rf t` removes
  `sub` and then says `rm: cannot remove 't\jn': Permission denied`, leaving `t` and the
  dangling junction (status 1). `rm -rf t/jn` alone works and keeps `sub`. A junction
  left dangling seems to be removed as a file (`DeleteFileW`, refused) where it is a
  directory link (`RemoveDirectoryW`). Its `\` in the message is not cash's `/` either.
- **The vendored uutils print message ids in a release** (found 2026-10-10; done the
  same day, phase 34 item 6): Scoop's
  1.10.0 says `sort: sort-cannot-read`, `Usage: tee-usage`, `Usage: shuf-usage`,
  `Usage: uniq-usage`. uucore's build script embeds a tool's messages from the
  `uu_<tool>-<version>/locales` directories it finds beside itself in the cargo registry;
  a tool built from `vendor/` is never unpacked there on a clean machine (CI, the
  release job), so its messages are missing. A local build looks right only because
  this machine's registry still holds the upstream copies. No test checks those
  messages.
- **Two tests were made robust in the 1.12.0 gate**: the 7z oracle's `u` of `h.tar`
  took `d/` as changed when its time fell near a second's end (the directories now get whole
  seconds), and `watch -e`'s key, typed as soon as the message showed, could be lost on a
  busy machine (the test now types until `watch` ends).
- **LZMA2's `c=` sets the block size, as in 7-Zip** (2026-10-11, from the 1.10.0
  benchmark in TODO.md): a bare number is a power of two, as for `d`, and each block's
  dictionary is cut to the block (`Lzma2EncProps_Normalize`); the blocks go to the
  threads, so a smaller block is faster and larger. The default stays 7-Zip's block of
  four dictionaries (128 MiB at `-mx5`), the same bytes as 1.13.0: splitting every file
  past one dictionary was tried, and the user chose smaller archives. Best of two on 16
  threads, one run against 7-Zip 26.03:

  | | 7-Zip | default | `c=` one block per dictionary |
  | --- | --- | --- | --- |
  | 49 MB `cash.exe` | 9.3 s, 11.55 MB | 20.5 s, 11.55 MB | 10.8 s, 11.79 MB (`c=24m`) |
  | 155 MB tar of Git's mingw64 | 28.9 s, 31.41 MB | 47.2 s, 31.47 MB | 12.1 s, 32.76 MB (`c=30m`) |

  Two blocks of 74 MiB cost the tar 3.4%, those of 128 and 20 MiB 0.4%: what a block
  boundary costs depends on where it falls in the data. Real 7-Zip tests every archive.
- **zip -v's space before the tab** (phase 30, 2026-10-07; done 2026-10-11): zip prints it
  where its progress dots would start; cash printed it for a stored member and for one of
  64 KiB or more, which matched every case tried. Bisected under Info-ZIP's zip 3.0: it
  starts at 65,274 bytes for deflate at every level and any content (the 64 KiB window
  less the 262 bytes deflate keeps ahead), at 65,536 for bzip2, and storing prints it for
  every member but an empty one. Cash missed it from 65,274 to 65,535 bytes and printed
  it for an empty stored member; `zip_cases.sh` now has the four edges.
- **The 2024 edition for `cash-reedline` and crossterm** (phase 34; done 2026-10-11): both
  kept the 2021 edition they came with, and now take the workspace's. What 2024 asked:
  crossterm's colour test marks its changes to the environment unsafe; reedline's
  history completer keeps 2021's captures (`+ use<>`), `open_editor`'s pattern derefs
  before its `ref` bindings, and two values are returned without a `let` (clippy's
  `let_and_return` holds under 2024's drop order); a block left by crossterm's Unix code
  lost its braces. `cargo fix`'s `expr_2021` fragments and `if let` rewrites were
  dropped: no macro has an arm `_` or `const {}` could now match, and no condition holds a
  temporary whose drop moved. The rest is the 2024 style's import order.
- **UAC's prompt for `sudo` often only blinked in the taskbar** (phase 35, 2026-10-10; done
  2026-10-11): when the asking process is not in front, Windows shows the consent dialog
  as a flashing shield, and the user missed three prompts that way; left long enough, a
  prompt counts as declined. As gsudo does, a thread now looks for the dialog
  (`FindWindow("Credential Dialog Xaml Host")`) every 100 ms while the request lasts, up
  to three seconds, and brings it to the front; `elevate` gets it too, through the same
  `runas` call. Checked by the user from the app's terminal panel: the dialog came up and
  was approved.
- **The next release's notes listed a speed-up that was undone** (2026-10-11; done the
  same day): `cf74f601` `perf(7z): split a file past one dictionary across the threads`
  was taken back by the commit after it. `release-notes.ps1` now leaves out the commits
  listed in `.github/release-notes-skip.txt`, and that one is there.
- **A test in the full suite left `x.lnk` in the repository root** (found 2026-10-06; closed
  2026-10-11): a 0-byte named pipe as MSYS2's `mkfifo` makes them. `mkfifo` became a
  builtin that refuses the same day, and the 1.14.0 gate's full run, all 8,533 tests,
  left no `x.lnk`, as none of the nine release gates between had.
- **The research documents' open questions** (found 2026-10-11 gathering decisions.md;
  answered and built the same day, decisions.md): `bind -m vi-*` is refused by name
  where it was ignored (`declare -t` was too, and taken back: it works, as Bash's, and a
  test of the RETURN trap failed in CI); flock's help says the lock ends with the
  command; `chown` and `chgrp` warn that nothing changed and return 0; `complete -A
  service` gives Windows' `drivers\etc\services` names and `-o bashdefault` Bash's
  defaults, both as Git Bash prints them; `xargs -P N` runs N at once; `logger` writes to
  the Application event log. Already answered by the code: `xargs -I{}` attached, `ls
  --help`'s options, `grep -r`'s paths, doctor's one line for the shadows.
- **Helix mode in `cash-reedline`** (phase 34; done 2026-10-11): on by default in
  reedline, so compiled into cash, though cash never offered it. Gone: its state machine
  and key bindings, `PromptHelixMode` and the prompt's Helix variants, `HelixChangeMode`,
  the cursor shapes for it, the resting policy that put the caret on a line end, the four
  editing commands only its keys emitted (`SelectLine`, `EraseSelection`,
  `PasteAtSelectionEdge`, `LineStartNonBlank`), the `helix` feature, and their tests.
  The selection model Helix gave the core editor stays, as vi and emacs run on it; an
  operation on a selection now always drops it, as vim's visual mode does.
- **`TAR_OPTIONS` and the command line** (phase 29, 2026-10-07; done 2026-10-12): GNU tar
  drops the `TAR_OPTIONS` side of `--one-top-level` against `-P` given on the command line;
  cash refused the pair wherever they came from. Checked against GNU tar 1.35 both ways:
  the typed one is kept and `TAR_OPTIONS`' dropped, in either direction; given in one
  place, the pair is still refused. tar_cases.sh has the cases.
- **7z cases not yet checked against 7-Zip** (phase 31; checked 2026-10-12): Unix-made
  zips' names and modes (Info-ZIP 3.0, with and without `-X`) already matched. Tar names
  that are not ASCII did not: 7-Zip's Characteristics carry its UTF-8 check (`UTF8-ERROR`,
  `non-UTF8`, `SingleSurrogate`, `Escape`, `Truncated`, `MaxUnicode=N`), a byte that does
  not start a sequence reads as U+EF00 plus the byte, user, group and link names read
  the same way, and pax header names show byte for byte. Now as 7-Zip; 7z_formats.sh
  has the cases. Zips read by their local headers and split zips stay in TODO.md.
- **`cp src >(cat)` can print zeros** (found 2026-10-10 in the 1.12.0 gate; done
  2026-10-12): a `>(...)` handed to a tool not patched for pipes is a temp file read as it
  grows, and `cp` (through `CopyFileEx`) sizes its target before it writes it, so a read
  in between passed on NULs. Until the program is done, the follower now reads only
  what is written: up to the file's valid data length, which NTFS keeps for a file past
  its record's room (`FSCTL_QUERY_FILE_REGIONS`); and, for a small file, which is zeros
  at once when sized, the zeros the file ends in wait until more follows them. Two
  tests size a file before writing it, 7 bytes and 1 MiB; the acceptance test is back.
- **A bundled tool outlived its script and its pseudo console** (2026-10-10; done
  2026-10-12): after a `sudo_in_terminal` probe dropped its pseudo console, the script's
  last command, `cash.exe --invoke-bundled cat got.txt`, was still there and held
  `target\debug\cash.exe`. Closing the console runs the exit's release on a thread of its
  own while the script goes on: it sorted the session job's members (an image read each,
  slow when there are many), ended the console programs, and let go of every job, and a
  program started meanwhile was let go with them. It then hung on its console, whose
  host stays up for it (both found alive a quarter of an hour on). Now no program starts
  once the release begins, and the sorting is repeated until a round finds no new
  member. A test closes the console of a script starting programs in a loop: two or
  three were left each time before, none after.
- **7z: zips 7-Zip reads and cash refuses** (phase 31; checked and done 2026-10-12): a
  zip with no central directory now opens by its local headers, as in 7-Zip: "Local" for
  the archive and each item, "Unexpected end of archive", status 2, the members
  extracted; a member's data descriptor is found after its data when another header
  follows it, else its data runs to the file's end and the item fails. A split zip
  (`-s`: `.z01`, `.z02`, …, `.zip`) opens from any part, its parts read as one, with
  7-Zip's volume facts and footer; a missing part is said and its members fail. Same
  behavior as 7-Zip, not the same bytes (the user, 2026-10-12): a member cut short says
  "Unexpected end of data" where 7-Zip says "Data Error". 7z_formats.sh has the cases.
- **A builtin started with `&` blocked until it finished** (found and done
  2026-10-16, `research/in-process-tools-design.md`): `grep -r x dir &` returned after
  102 s, as `&` waited for the job's pid, which a builtin published only when it ended.
  A builtin that starts no command now says at once that no pid is coming; one that may
  (`nohup`, `nice`, `xargs`, `eval`, …, listed in the factory) still lets its command's
  pid be `$!`, and in a pipeline only the last stage reports, as in Bash.
- **Two Ctrl-C ended the shell during a long native builtin** (found and done
  2026-10-16): `grep -r` over a large tree did not stop, and a second Ctrl-C, which Windows
  takes, ended cash. Builtins that may run long ask a `cash_core::StopToken` between
  pieces of work and end through `ExecutionContext::stopped`, as a program Ctrl-C ended
  would (the INT trap, or 130): grep, find, `ls -R`, tree, the compressors' named files,
  and any builtin reading standard input. `kill` of a background job stops them too. A
  second Ctrl-C ends the shell only when the first has waited two seconds (D14 amended).
- **Parsing an `env` or `timeout` command ran uucore's setup in the shell** (found and
  done 2026-10-16): `command_operand` called `prepare_uutil_runtime` for every such
  command line, which wrapped the process's panic hook once more each time, filled
  uucore's process-wide message store with `env`'s, and could `exit(99)` the shell. Parsing
  needs no messages, and does no setup now.
- **A compressor stopped half way removed the file it was compressing** (found and done
  2026-10-16, the day the stop above came in, before a release): a read the stop ended
  looked like the file's end, so `gzip big` and `bzip2 big` stopped by Ctrl-C or `kill`
  wrote half an archive and removed `big`, and `zstd` wrote half an archive and said
  100%. A stop now fails the read (`cash_core::Stopped`), and the compressors give up as
  on any failed read, saying nothing and ending with the stop's status: the file is kept
  and no archive is left. Their standard input fails the same way, and xz's writes too,
  since its threads read ahead.

- **Shared parts for the builtins, 1: options** (the user's choice of 2026-10-16, done
  2026-10-17; `research/central-components-design.md`, decisions.md):
  - a repeated option overrides in every clap builtin (`builtins::parse_args`), so
    `alias ls='ls --color=auto'` then `ls --color=never` works;
  - cash-getopt gained a claim hook for `-10` and `-KILL`, the `POSIXLY_CORRECT` scope
    (`cash_getopt::posixly_correct`, honoured by every tool on it), and
    `Exclusive`/`long_name` for util-linux's exclusive options; cash-builtins gained
    `options` (the order and GNU's usage-error line);
  - moved onto it, each against Git Bash, WSL's procps, psmisc, util-linux, ncurses or
    iputils: basename (stops at the first operand), dirname, mkfifo, rev (`-V`), locale
    (`--`), stdbuf, nice, pkill (`-p`, `-O`, `--help`, `-V`), pidof (long options,
    `-S`), killall (one-dash long names, `-V`), tput (`-V` in order), ping (iputils'
    exact table);
  - flock, watch and nc act on help and version where they stand; watch keeps a leading
    `--`;
  - off clap: xargs (GNU findutils 4.10's options, `-L -d -E -a -i -l -x -p -o`,
    `--process-slot-var`, `--show-limits`), ls (GNU coreutils 9's whole table, its own
    options beside it, `--classify[=WHEN]`, `-f`, `--version`), stat (`-t`,
    `--cached`, and `-f`, which had done nothing, now reports the volume), pgrep
    (procps' table); find answers `-help` and `-version` and takes a leading `--`.

- **Shared parts for the builtins, 2: inputs** (done 2026-10-17):
  `ExecutionContext::open_input` (`cash_core::input`) opens a name as the shell's `<`
  does: the shell's directory, the devices, `/dev/stdin` and `/dev/fd/N` as the shell's
  descriptors; one open, the handle's metadata, "Is a directory" for a directory, the read
  through the stop token; `Input::rewinder` reads a file again without opening it again.
  A read the stop fails, passed up with `?`, ends a builtin as stopped, in silence.
  - grep, rev, column, hexdump, xxd, iconv, logger and the pagers read `/dev/stdin` and
    the rest, and word a directory as their upstreams do (checked against WSL);
  - gzip, bzip2, xz and zstd open once and decide from the handle; bzip2 `-f` no longer
    opens a process substitution's pipe a third time; dos2unix calls a device's name no
    regular file;
  - tar reads cash's spellings (`/c/x`) for names, `-C`, `-T` and `-X`, and devices for
    its lists and archive; 7z reads its list files from the shell's directory;
  - `< dir`, `> dir` and `source dir` say "Is a directory".

- **Shared parts for the builtins, 3: walks** (done 2026-10-17): `cash_builtins::walk`,
  on walkdir. Names shown as typed and joined once; one answer to what a link is (a
  symlink or junction; a cloud placeholder is the file or directory it holds); the stop
  token asked for each entry; every error handed to the tool, a directory that could not
  be listed apart from a name that is not there. Links below a root are followed by the
  layer, a loop found by the identity of the directories above: walkdir's own following
  opened each directory to read, and lost one that could not be read.
  - grep -r/-R, find (`-H` real, its loop message kept, a name it cannot read quoted),
    tree (a cloud directory is a directory, a linked root listed), gzip -r and zstd -r (links
    as upstream), chmod -R (a directory it cannot list is said), where (where.exe's order,
    through junctions, each directory listed once, one walk for every pattern), lsof +D
    and fuser;
  - tar, zip, rar and 7z: through links where upstream goes, each file stored once at a
    junction back to a directory above (upstream goes round until the path is too long,
    or, GNU tar, until its stack overflows); tar's and rar's words for a directory that
    cannot be listed; rar's archive masks in Rar.exe's order; 7z takes only symlinks
    and junctions as links; a stop in a walk fails the run, never an archive that looks
    whole;
  - 7z x -o makes its directory when nothing is extracted, as 7-Zip does (the old find
    had hidden it from the oracle);
  - ls -R joins names once (`ls -R foo/`), and ls --tree says a directory it cannot open.

- **Shared parts for the builtins, 4: the parts called most** (the user, 2026-10-17;
  done the same day, `research/hot-paths.md`). 30 loops timed with hyperfine against
  Git Bash's bash; most of the language was already ahead (arithmetic, function calls,
  `[[ ]]`, starting programs). What was behind, and now (seconds, before / after / bash):
  - a glob of System32's DLLs, 200 times: 2.20 / 0.92 / 1.08 (the pattern made once per
    component, each name read once, one sort);
  - `type -P cmd`, 2,000 times: 6.17 / 1.33 / 3.23 (one directory query per PATH directory
    where each name it could have was looked for);
  - `while read -r l` over 100,000 lines: 3.23 / 1.27 / 1.70 (a block of a disk file and
    the rest given back, as Bash's `lseek`, and the parse of a line seen kept);
  - `local x=1` in a function, 50,000 times: 1.21 / 0.30 / 0.46, `local -i x=1` 1.29 /
    0.31 (clap's parse, which sets a default for each of declare's two dozen options,
    kept for words seen before: `builtins::parse_remembered`);
  - `[ "$i" = 5 ]`, 100,000 times: 0.80 / 0.57 / 0.61 (`test` takes its words without
    clap).
  printf, the parse caches and `cd` are in TODO.md.

- **GNU ls's common options** (the user's choice of 2026-10-17, done the same day), each
  checked against Git Bash's coreutils: `-p`, `--file-type`, `--indicator-style`, `-m`,
  `-x`, `--format`, `-w`, `-o`, `-g`, `-G`, `-I`/`--ignore`, `--hide`, `-B`,
  `--full-time`, `--time-style` (and `TIME_STYLE`, in an exported `TZ`'s zone), `-L`
  (`-RL` says a junction loop, status 2). `-C` lays columns out as GNU does, each as wide
  as its widest name, where every column took the widest name's width; the width is
  `COLUMNS`, the terminal's, or 80 into a pipe, where the console's was taken. `ls -l` on
  a link to a directory shows the link, as GNU's does. `help ls` gives `ls --help`'s options.

- **The clipboard test failed while another program held the clipboard** (found
  2026-10-16, done 2026-10-17): an open of the clipboard tried for 200 ms, ten times
  20 ms apart, and a clipboard manager or a program copying can hold it longer. It tries
  for two seconds now, the wait doubling from 20 ms up to 250 ms; a unit test holds it
  from another thread for half a second.

- **`{ head -n1; cat; } < file` lost what head read ahead** (found 2026-10-16, done
  2026-10-17): uutils' head seeks a shared input back on Unix alone. head is taken into
  `cash-uutils` and leaves a file under its standard input just after what it printed,
  as GNU head does; a head that comes to the file under way reads what is left of it,
  also for `-n -N` and `-c -N` on a large file, which uutils measured from the file's
  start (`{ head -n1; head -n -1 | wc -c; } < file` counted 43352 of 108886 bytes). Each
  mode checked against Git Bash's head.

- **tail gave too much of a pipe and nothing of a file of 64 KiB** (found and done
  2026-10-17, while checking head): `head -n -1 big | tail -c 8` printed 2,000 to 10,000
  bytes, `tail -n 2` printed nothing of a file of exactly 64 KiB, and `tail -n 5000` gave
  4096 lines of one of 128 KiB, all in 1.19.0; and `tail -f --use-polling` printed a
  file again on each change, saying it had been replaced. tail is taken into
  `cash-uutils`, as head was, and each is fixed there; checked against Git Bash's tail.

- **`d=$(mktemp -d); trap 'rm -rf "$d"' EXIT; cd "$d"` left `$d` behind** (spec §4 #13;
  the spec's claim that an in-process `rm` could not fix it was found wrong 2026-10-16,
  done 2026-10-17): the bundled `rm` ran in the shell's directory, which Windows will not
  let a process remove. cash's own `rm` moves to the volume's root before it removes a
  directory that holds its own; a relative operand after that names nothing, as on Linux,
  and `..` is still refused. §4 #13 now names the other bundled tools (`rmdir`), and
  spec §8 says why an in-process tool would have fixed it.

- **The archivers finished their run before a Ctrl-C or `kill` was acted on** (found
  2026-10-16, done 2026-10-17): tar, zip, 7z and rar read a file they archived, and
  unzip, 7z and rar a member they extracted, to its end; tar of a 64 GiB file went on
  writing until the disk was full. Each reads through the stop token now and fails the
  run within a block, as it fails at a Ctrl-C: tar and zip in silence, rar "User break",
  7z "Break signaled", with no archive left (zip's temporary file, which any early
  return had left behind, goes with it: `cash_win32::unix::Replacement` removes it when
  dropped). rar's writer is cancelled through its own `WriteCancellation`.
- **`rar a` refused every file of 16 MiB or more** (found and done 2026-10-17, while
  testing the above; 1.19.0 too): "compression needs at least 456130560 bytes of
  working memory for a 16777216-byte dictionary, above the configured limit of
  268435456 bytes". RAR 5's compression takes many times its dictionary, and the writer
  ran on the library's default budget of 256 MiB; it gets half the machine's memory now.

- **The DEBUG trap missed a command that only assigns** (found 2026-10-16, done
  2026-10-17): cash fired it only before a command with a name, and after its words were
  expanded and its redirections made, so `echo hi > f` had the trap write into `f`. It
  runs before every simple command now, before expansion and redirections, as Bash runs
  it; checked against Git Bash's bash, `exit` and `return` in it included.

- **A closed descriptor said too much** (found 2026-10-16, done 2026-10-17): `echo hi
  >&-` said `echo: i/o error: I/O write error: standard output not available`, and
  printf an `I/O flush error`; they say Bash's `write error: Bad file descriptor` now.
  `read` on a closed standard input says `0: read error: Bad file descriptor`, and
  `mapfile` reads nothing from it and succeeds, as Bash's do; "invalid file descriptor"
  is kept for a descriptor `-u` names. Checked against Git Bash's bash.

- **ls's words for a bad `--color`, `--classify` or `--sort`** (found and done
  2026-10-17): one line of words and status 2, where GNU's ls lists the values a line
  for each group that means the same (`'always', 'yes', 'force'` and so on), with a
  `Try` line and status 1, as cash's `--format` already did. `--icons`, cash's own,
  says it as GNU's `--hyperlink` does. Checked against Git Bash's ls.

- **The tools taken from uutils gave cash's version as uutils'** (found and done
  2026-10-17): `sort --version` said `sort (uutils coreutils) 1.19.0`, a uutils release
  there never was; rm, sort, tee, uniq, shuf, head and tail say the 0.12.0 they came
  from now, as the bundled tools say theirs.

- **`printf` took 1.4 times Bash's time** (found 2026-10-17 in the hot-paths survey,
  done the same day): clap's parse of its arguments, which change from call to call, was
  most of what a call cost over `true`. Its one option is parsed by hand, as Bash's
  `internal_getopt` takes `-v NAME`, with clap's errors said in Bash's words as before:
  100,000 calls took 1.030 s, now 0.719 s, against 0.698 s for Git Bash's bash. A bad
  `-v` name says `` `1x': not a valid identifier ``, status 2, as Bash's does, where it
  said "failed to parse parameter", status 1.

- **`cd` took 62 µs** (found 2026-10-17 in the hot-paths survey, done the same day): it
  tells a directory with `GetFileAttributesW`, without the handle `std::fs::metadata` opens
  (a link or a junction still goes through `metadata`), and renders its path once:
  55.6 µs a call, against 233 µs in Git Bash. The rest is Windows': one attribute query
  costs 43 to 48 µs on this machine outside cash too, and `cd` must ask once.

- **The parse caches were 64-entry LRUs behind a global lock** (found 2026-10-17 in the
  hot-paths survey, done the same day): words, tokens, patterns, arithmetic and `eval`'s
  programs; each lookup copied its key and took the lock, and a loop over more than 64
  distinct words missed on every one, as an LRU cycled through does. They are a thread's
  own memo now (`cash_parser::memo`), looked up by the text as it is, 4,096 kept (256
  programs): 200,000 words over 101 distinct ones took 776 ms, now 647 ms. A hit still
  copies its value.

- **Every subshell copied the table of builtins** (from the 2026-10-16 survey of work
  done on every call, done 2026-10-17): `Shell::clone` copied its 200-odd entries for
  each `( )` and `$( )`. Subshells share it now, and the one that changes it (`enable`)
  copies it then: a `( : )` took 20.1 µs, now 15.7, and a `$(true)` 4.4 µs less of its
  74. `enable -n` in a subshell stays in it, as in Bash; a test says so.

- **Command completion listed each PATH directory whole** (from the 2026-10-16 survey, done
  2026-10-17): each directory is asked for the names beginning with the prefix
  (`FindFirstFileExW`), the long names still compared, and a prefix with a wildcard
  character lists it whole as before: `compgen -c no` took 24 ms, now 17.7, the same
  names for every prefix tried, the empty one's 7,963 among them.

- **reedline 0.52's fixes, against cash-reedline** (found 2026-10-16 by `cargo xtask
  upstreams`, done 2026-10-17): ported one by one with upstream's tests, the panics
  (vi counts, an empty `UntilFound`, the menus' parsing, slicing and divisions, a style
  range inside a character, the undo stack's index) and the behaviour fixes (vi `P` of
  a line, Enter over an empty menu, a history that refuses a line, a cursor query that
  gets no answer, the right prompt on exit, double-width characters at the margin, a
  resize's row). What was not taken, and why, is in the crate's README; 0.52.1 changes
  only Android's history locking, and `upstreams.toml` says reviewed to it.

- **uutils sed's fixes since the revision taken** (found 2026-10-16 by `cargo xtask
  upstreams`, done 2026-10-17): its 53 new tests run against cash-sed, GNU sed 4.9
  deciding each failure. Fixed: a range whose numeric start is already behind it
  (`3,6{ 1,4p }` printed nothing), `n` at the end of a file (`sed 'n;s/^/X/' - file`
  stopped after stdin's line), `Q`'s missing newline and queued text, `l` of an empty
  line, `r` after a line without a newline, `-ni.bak`, `-f -ifoo.sed`, `-i=.bak`, and
  `-l` read as GNU reads it. uutils diffutils' commits change only CI. What was not
  taken is in `research/uutils-sed-evaluation.md`.

- **posixutils-rs awk's fixes up to 0.9.1** (found 2026-10-16 by `cargo xtask upstreams`,
  done 2026-10-17): its 48 new tests run against cash-awk, gawk 5.4 deciding, and only
  what generic awk meets ported (the user, that day: no need to replicate gawk deeply).
  The worst: an expression's parse took time exponential in its nesting, `x =
  ((((1))))` five seconds and `a[a[a[a[1]]]]` over a minute; now linear, 100 levels
  through cash. Also `"cmd" | getline line > 0`, a newline after `&&` and `||`,
  `$NF++`, `exit` in BEGIN, `gsub` without a match, `%#x` of 0, FILENAME before the
  first input, and bytes that are not UTF-8 written back as read (a Windows-1252 file
  came out of `awk '{ print }'` as UTF-8). Not taken, in the crate's README: bytes as
  characters in the C locale and three gawk corners.

- **posixutils-rs bc's commits up to 0.9.1** (found 2026-10-16, done 2026-10-17): a
  comparison is an expression valued 1 or 0, as in GNU bc, so `echo "1.5 > 1.2" | bc`
  prints 1 where it was a syntax error; taken whole on the user's choice, though cash's
  bc is otherwise POSIX bc, with upstream's GNU bc 1.07.1 fixture. The other commits
  build on Windows or word a write to a full device. With this, every upstream in
  `upstreams.toml` is reviewed to its newest.

- **Three tests that failed once in CI** (the user picked all three, 2026-10-17):
  - **The installer's links test kept an old version once** (`ci full` before 1.20.0):
    the upgrade's sweep opens the old cash.exe for writing to tell whether it runs, and
    right after the step before had run that freshly copied exe, something still held
    it (Defender's scan of a new executable, or the image's teardown). The test waits,
    up to 20 s, until it can open it as the sweep does. A real upgrade sweeps such a
    directory the next time.
  - **A cash-sed test timed out once in CI** (run 39024431762): it starts 14 small
    seds and cannot hang; half a dozen other sed tests passed 15 s in the same minute
    on a starved runner. uutils' sed tests get a 30 s period, an exception the user
    made to the rule on load (decisions.md); a test that finishes takes the same time.
  - **The ConPTY tests that run PowerShell timed out now and then**: the script compiled
    its C# console helper with `Add-Type` on every start, 0.4 to 1 s here and, with
    Roslyn cold on a fresh runner, plausibly past the 30 s the tests wait. The helper is
    compiled once, before the ConPTY starts, into a DLL in the temporary directory named
    by its source and PowerShell, with its time printed; the script loads it.

- **Two costs paid on every call** (the user's pick of 2026-10-17, from the 2026-10-16
  survey):
  - **A missing command given arguments probed PATH twice**, at the prompt where
    `winglob` is on: the decision of who globs (D81) looks the name up as the second word
    is expanded, and the dispatch looked again. The first used the search that tries
    every `PATHEXT` extension in every directory, about 39 ms; it now uses the dispatch's,
    which reads each directory once, and remembers a miss, which the dispatch takes while
    `PATH`, `PATHEXT`, the working directory and the count of exit statuses are as they were
    (a command substitution in a later word can make the program, and it is found).
    `nosuchcommand x` 200 times with `winglob` on: 9.2 s, now 1.4 s; the first run of a
    program given arguments at the prompt saves the same.
  - **coolfetch asks for everything on each run**: looked into, about 30 ms over an empty
    command, once for each new window from the starter `.bashrc`; the user: fine as it is.

- **GNU ls's times and sizes** (the user's pick of 2026-10-17, the first group of the
  options cash's ls refused): checked against Git Bash's ls (coreutils 8.32) on NTFS and
  GNU coreutils 9.10 in WSL.
  - `-c`, `-u` and `--time=WORD` choose the time `-l` shows and `-t` sorts by, and
    without `-l` sort by it: `-c` is NTFS's change time (contents, name or attributes),
    which Git Bash shows; `--time` takes GNU 9's words, `mtime` among them.
  - `-s` shows each entry's blocks, the volume's allocation in whole KiB as Git Bash
    counts it, in every layout; `-k`, `--si` and `--block-size` (a number, a unit, both,
    `human-readable`, `si`) and `LS_BLOCK_SIZE`, `BLOCK_SIZE`, `BLOCKSIZE` and
    `POSIXLY_CORRECT` scale sizes and blocks as GNU's `human_options` does, with GNU's
    errors.
  - The totals count allocated blocks, where they counted sizes: a 5000-byte file is 8,
    not 5; `-h` rounds up as GNU does, so 2,000,000 bytes read `2.0M`, not `1.9M`.
  - `stat`'s Change is the change time too, where it was the modification time.
  - The facts come from the open `ls -l` already makes of each file, and otherwise by
    name (`GetFileInformationByName`, Windows 11 24H2) or an attributes-only open.

- **GNU ls's name quoting** (the user's pick of 2026-10-17, the second group): checked
  against GNU coreutils 9.10 in WSL, every style on names Windows allows and, through
  the quoting itself, on control characters it does not.
  - `--quoting-style` with GNU's ten words, `-N`, `-b`, `-Q` (the last given wins) and
    `QUOTING_STYLE`, a word it does not know said and passed over; `-q` and
    `--show-control-chars` (the last wins) show a control character the style left as
    `?`. On a terminal, names are shell-escaped and controls hidden, as GNU's are
    (decisions.md); into a pipe they are as they are, as before.
  - Where some names in `-l`, `-C` or `-x` came out quoted in the shell styles, the
    others get a space before them, as GNU lines them up; `-R` headers and link targets
    are quoted too.
  - Not followed: GNU's columns beside a name with a raw control character (DEL, or a
    C1 control, the only ones a Windows name can hold) in a style that leaves it raw.
    GNU pads such a name by a width of its own and lines the next column up wrong, 3 or
    4 spaces where every other name gets 2; cash keeps its columns straight.
  - Fixed after: c-maybe lines names up as the shell styles do, and `-C` and `-x`
    without a width (`-w 0`) line nothing up, as GNU has them.

- **GNU ls's script helpers** (the user's pick of 2026-10-17, the third group): checked
  against GNU coreutils 9.10 in WSL, and `-i` against Git Bash's ls.
  - `--zero` ends each line of entries with NUL in every layout, the totals too, while
    headers and the blank lines between directories keep their newline; it makes names
    literal, one to a line unless long, and colourless, each of which an option after
    it can undo, as GNU's does.
  - `-i` shows each entry's file index, NTFS's file ID, the number Git Bash's `ls -i`
    and `stat` show, padded as GNU pads it (`-m` not), before `-s`'s blocks.
  - `-H` follows the links named on the command line, and
    `--dereference-command-line-symlink-to-dir` those to directories, even with `-l`; the
    last of them and `-L` wins.
  - `-T` is checked as GNU's `xstrtoumax` reads it (decimal, hex, octal; GNU's errors,
    status 2) and changes nothing: cash writes spaces, never tabs.
  - `--hyperlink[=WHEN]` makes names, headers and link targets OSC 8 links to what they
    resolve to, the quotes of a quoted name outside the link where names line up, as
    GNU writes them; the URLs are `file:///C:/…`, with no host, the form Windows
    Terminal opens (decisions.md).

- **GNU ls's poor fits for Windows** (the user's pick of 2026-10-17, the last group; ls
  now reads every option of GNU coreutils 9's): checked against GNU coreutils 9.10.
  - `-n` is `-l` with the owner's and the group's numbers, their RIDs, as `id` and
    `stat` give them.
  - `-Z` shows `?` for each file's security context, as GNU ls shows it for a file
    without one: a field before the name, and a column after the group with `-l`.
  - `--author` shows the owner again after the group with `-l`, as on Linux, and not
    where `-g` leaves the owner out.
  - `-D` (`--dired`) is `-l` for Emacs: each line indented by two spaces, then
    `//DIRED//` with each name's start and end, `//SUBDIRED//` with each directory
    header's, in bytes from the start of the output, escapes not counted, and
    `//DIRED-OPTIONS// --quoting-style=…`; a later `-C`, `-x` or `-m`, or
    `--hyperlink`, turns it off, and with `--zero` it is refused, as in GNU ls.

- **`tail -f file | head -1` ran until the file grew** (the user's pick of 2026-10-17):
  GNU's tail asks between polls whether its output still has a reader
  (`check_output_alive`), and uutils does on Linux; on Windows tail heard of it only at
  its next write, and `tail -f g | head -1; echo done` waited for the file to grow, as
  Git Bash's tail does. The relay of a bundled tool's output keeps the real standard
  output, and tail asks it between polls, from the pipe's state
  (`NtQueryInformationFile`'s `FilePipeLocalInformation`, which the write end of a
  `std::io::pipe()` answers): once the reader has gone it ends with 141, within one
  `-s` interval (1 s by default). Only tail asks: a watcher on every bundled tool would
  end `cp a b | true` before it copied.
- **A link's target in `ls -l` had Windows' backslashes** (found 2026-10-17 with `-H`):
  `linkdir -> C:\Users\…\sub`, where `readlink` gives `C:/Users/…/sub` and D3 allows
  no backslash; it is in cash's spelling now, and a link's size is its target's length,
  as `lstat` gives it on Linux and Git Bash's ls shows it, where Windows says 0. `stat`
  shows it so too: `%N` and the report's `File:` line with ` -> ` and the target, as Git
  Bash's stat does, where they gave the name alone.

- **PIPESTATUS after a compound command** (found 2026-10-17 by the quoting comparison,
  whose `${PIPESTATUS[0]}` after an `if` read 0): a compound command that is the whole
  pipeline (`if`, `for`, `case`, `while`, `{ }`) and a function's definition leave
  PIPESTATUS as the commands inside set it, as Bash does: `if true; then false | true;
  fi` leaves `1 0`, where cash made it `0`. A subshell, `[[ ]]`, `(( ))` and a function's
  call still set it as a simple command does.
- **`stat` showed times in UTC to the second** (found 2026-10-17 with `ls -c`): `%x`,
  `%y`, `%z`, `%w` and the report's lines read `15:29:57.000000000 +0000` where Git
  Bash's stat reads `17:29:57.594045400 +0200`. They are in the zone an exported `TZ`
  names, else Windows', to the 100 ns NTFS keeps, as `ls` already showed them.
- **The ls times test failed on change times that tied** (found 2026-10-17): its fixture
  set the times of four files and made one read-only within one tick of the clock NTFS
  stamps changes with, as coarse as 15.6 ms, so all four changed at once and `ls -c`
  sorted them by name. The read-only change now comes 50 ms later.
- **The awk append test took 10 s of the quick lane** (found 2026-10-17): its two loops
  of two million appends, added with posixutils-rs awk's fixes, ran 10 to 12 s in a
  debug build, near the 15 s the quick lane gives a test, on this machine, faster than
  CI's. Half a million each still tells linear from copying: 3 s in all.
- **`timeout 2 cash -c 'sleep 5; echo done'` printed `done`** (the user's pick after
  1.22.0): settled, not cash's. Git Bash's `timeout` is an MSYS program, and a native
  program it runs is not reached by its signals as an MSYS one is: with TERM, cash's
  `sleep` (a child of cash) ended with 143 and cash went on, as `cmd /c "ping …& echo
  done"` goes on; with INT nothing is reached at all (`sleep=0`). Run from a file, from
  a pipe, by 1.18.0 and by 1.22.0, the same. In cash, `timeout` is cash's own and ends
  the command (`timeout 2 cash -c …` gives 124 and no `done`). What it showed on the way,
  that a TERM trap never runs on cash's own `kill -TERM`, is in TODO.md.
- **`pbcopy` and `pbpaste` on a locked workstation said "the clipboard is in use by
  another program"** (the user's pick after 1.22.0; found when the full lane's clipboard
  test failed while the screen was locked): Windows refuses `OpenClipboard` with
  ACCESS_DENIED while the session is locked, the answer it gives for a clipboard another
  program holds, and cash waited its 1.8 s and blamed one. At the first refusal cash now
  asks the session's flags (`WTSQuerySessionInformation`'s `WTSSessionInfoEx`, wtsapi32
  loaded only then) and says at once "the clipboard cannot be used while the session is
  locked"; the clipboard tests skip then. Seen unlocked (the flag reads so and the tests
  run); the locked case rests on Windows' documented flag, the session having been
  unlocked by the time it was built.
- **`ls -l` moved the access time of a file it read for `#!`** (the user's pick after
  1.22.0): `ls -l`, `ls -F` (and the colours) and `stat` read a file's first two bytes to
  tell whether it runs, and NTFS took the read for an access, so `ls -lu` showed ls's
  own read, as Git Bash's ls does; GNU's reads no file. The read now goes through a
  handle opened with `FILE_WRITE_ATTRIBUTES` and told to keep the access time
  (`SetFileTime` with all ones). Where the access list lends no such handle, the read
  is as before, and the rest of that directory is not asked again: asking each file of
  `C:/Windows/System32` made `ls -l` 40% slower (520 ms to 715 ms in a debug build),
  and with the directory remembered it is as before (530 ms).
- **`stat`'s blocks were the size's, not the allocation's** (the user's pick after
  1.22.0): `%b` and the report's `Blocks:` count the blocks of 512 bytes (`%B`) the
  volume gives the file, from the facts stat reads for the change time anyway, so `%b`
  times `%B` is its space on the disk: 16 for a file of 5000 bytes on 8 KiB, where Git
  Bash's stat says 8 of 1024; it was 10, the size's.
- **A script's shebang interpreter was looked up twice** (the user's pick after 1.22.0,
  from the 2026-10-16 survey): before starting a script the shell resolved its
  interpreter, searching `PATH`, only to answer one whose interpreter is `true` or
  `false` at once, and the start resolved it again. Those two are told without a search
  now (`interpreter_exit`, which a test holds to `resolve_interpreter`'s answers), and
  the start's is the one search: a `#!/usr/bin/env cash` script ran in 84 ms where it
  took 102 ms (best of 3 × 20, debug build).
- **The PATH search that checked each extension was slow** (the user's pick after
  1.22.0, found with the missing-command lookup): the search for every match
  (`pathsearch::search_for_executable`, under `type -a`, `which -a`, `hash NAME` and
  carapace's first look for itself) checked each `PATHEXT` extension in each directory, a
  file check each. It looks at each directory once now, as running a command does
  (`listed_names`, `resolve_executable_among`), and finds the same files: `type -a cmd`
  and `which -a cmd` take 8.5 ms where they took 43 over 79 directories, `hash cmd` 1.3 ms
  where it took 3.8 (debug build).
- **`ls -la` was 12% slower for keeping the access time** (found 2026-10-17 by the
  benchmark at 1.23.0, `research/benchmarks-1.23.0.md`; the user's pick of 2026-10-18):
  timed with the calls cash makes, 3,000 files, best of five: an open 36.3 µs, the read
  of two bytes 4.3 more, `FILE_WRITE_ATTRIBUTES` in the open 4.7 more, `SetFileTime` 7.1
  more. Windows has no cheaper way to keep it (the all-ones `SetFileTime` is its only
  one, and needs that open), so `ls` reads as Git Bash's ls does again, the access time
  moving, and `stat`, which reads one file at a time, keeps it
  (`is_executable_keeping_access_time`), as the user chose.
- **`ls -l`'s file facts from the directory listing** (found by the same benchmark; the
  user's pick of 2026-10-18): the change time, allocation and file ID `-l`'s totals,
  `-s`, `-c` and `-i` need were asked of each file, two calls more in `-l`'s open of it
  and a query or an open of its own otherwise, 7% of `ls -la`. One query of the directory
  (`cash_win32::fs::directory_facts`, `FileIdBothDirectoryInfo`, as Cygwin reads a directory)
  gives them for every entry now; a link `-L` goes through is still asked for what it
  names. The numbers are Git Bash's (`ls -is` the same). With both fixes `ls -la` of 210
  entries 50 times takes 697 ms, where 1.23.0 took 809 and the build before the
  regression 676 (the rest is the second query of the directory and the new options'
  bookkeeping).
- **A TERM trap never ran on cash's own `kill -TERM`** (found 2026-10-17 with `timeout`;
  the user's pick after 1.23.0): settled as a limitation. `kill -TERM` from another
  process ends a cash script at once with 143 (it sends a Ctrl-Break only to a group
  leader, and terminates any other console program), so its `TERM` and `EXIT` traps do
  not run; Ctrl-C and `kill -TERM $$` run them. A channel of cash's own to carry
  signals between cash processes was turned down; spec.md §4 #31, `help differences`
  and `help trap` say so.
- **Background jobs killed at once hung past 15 s in CI** (found 2026-10-17, runs
  39043122930 and 39066508176; the user's pick after 1.23.0): settled as load. Here,
  `tool & kill %1; wait %1` for `zip`, `rar` and `7z` on 3,000 files ran 30 times under
  16 busy loops without a hang, and the three tests 15 times under nextest's stress
  mode beside 8 busy loops, all passing (some past 5 s, none near 15). As the user
  decided on 2026-10-02, such a failure is not chased and no timeout is lengthened;
  the one remedy taken is the rule's own: the test that starts 40 busy loops, which the
  two others had timed out beside, runs alone now (`threads-required`), as the test that
  spins two threads per processor does.
- **`which <builtin>` walked all of PATH** (the user's pick after 1.23.0, from the
  2026-10-16 survey): for a bundled tool `which` looks along PATH for a `cash
  --link-tools` link to this cash. The search for every match looks at each directory once
  since the PATH search above (`which ls` 96 ms, now 16, bench builds), and the first
  link found ends it now, where every match was gathered before any was compared.
  Without a link the whole of PATH is still looked at once: a link can be in any directory.
- **Each `.ps1` run searched PATH for pwsh** (the user's pick after 1.23.0, from the
  same survey): one walk for `pwsh`, then `powershell`, before each script. The one
  found is remembered with the `PATH` and directory it was found for, and found again only
  when either changes or it has gone (a script run with Windows PowerShell's directory alone
  on `PATH` gets that one).
- **`HISTCONTROL=erasedups` scanned the whole history after each command** (the user's
  pick after 1.23.0, from the 2026-10-16 survey): measured in a release build with
  50,000 entries it was 12.9 ms after every command, not the 1–2 ms guessed, each entry
  looked up in a persistent map, and the list of entries rebuilt whole when one went
  (5 ms). An index from command line to entries, built the first time erasedups asks
  (18 ms, once a session; a shell that never erases does not build it) and kept through
  adding, trimming to `HISTSIZE`, `history -d`, editing and clearing, finds them now, and
  the entries' order is a vector shared until changed, from which one is dropped by a
  move: 98 µs after a command whose line was there, nothing when it was not.
- **The completion menu scanned PATH on each keystroke** (the user's pick after 1.23.0,
  from the 2026-10-16 survey): while the menu is open, each keystroke asked every PATH
  directory for the names beginning with the prefix, 17.7 ms for `compgen -c no`. The
  background index of PATH (`PathIndex`) keeps each directory's names as spelled too, sorted
  by their folded form, and the menu takes the prefix's range from it, a name running by
  a `PATHEXT` extension, a file of the name with one beside it, or its contents, read
  once and remembered until the directory changes; until the index is ready, the scan
  answers as before. `compgen -c no` takes 1.9 ms (debug build), and gives the same
  names as 1.23.0 for `no`, `a`, `g` and `s` (17, 355, 180, 765).
- **Command completion offered DLLs** (found 2026-10-18 with the menu's index; the
  user's pick after 1.24.0): a file without a `PATHEXT` extension runs by its contents,
  and a DLL is a PE image, so `compgen -c no` offered `noise.dll` and `type -a` found it,
  where running it fails. A PE image counts only when its COFF header does not mark it a
  DLL (`IMAGE_FILE_DLL`), from one read of the file's first 512 bytes, where the header
  lies in practice, instead of two: no slower. An `MZ` too short for the header still
  counts, as before.
- **Busy background jobs starved the foreground** (found 2026-10-18 in runs 39077538398
  and 39079333899, 1.24.0's release gate; the user's pick): with the test that starts
  40 busy loops running alone, CI still timed it out at 15 s on all four tries. Its own
  jobs were the load: 40 threads of cash at the foreground's priority, which on one
  processor here held the script 11 to 19 s (3.2 to 4.1 s on four, 1.4 to 3.4 s on
  sixteen), and Windows Server's longer time slices made it longer still. A job's
  threads (its own, its pipeline stages', its process substitutions') now run below
  normal priority, so the foreground takes a processor the moment it wakes; lowered
  alone, a job that held a lock the foreground wanted (the first write to standard
  output, made once) still kept it 0.6 to 1.3 s, waiting its turn behind 39 busy
  siblings' time slices, so a job's thread hands the processor on after a millisecond
  between its pipelines. The script takes 0.2 s on one processor, 0.12 s on four; a
  background loop alone is as fast as before (+1.2%, within the noise). The new test
  runs it on one processor (`start /affinity 1`): 12 to 14 s before, past its 8 s
  bound.
- **The corpus's first pure-bash-bible round** (2026-10-18, the user's pick after
  1.25.0): 23 cases from the sections the corpus lacked (strings, arrays, loops, files,
  variables, parameter expansion, conditionals, arithmetic, traps, internal variables,
  `printf %(…)T`, `read -t`, aliases), frozen against Git Bash 5.3.15, and
  `tests/corpus/SOURCES.md` with the terms of every source the corpus copies from,
  pure-bash-bible's MIT notice and Lucent's for the One True Awk among them, which were
  nowhere before. What they found, each fixed as Bash 5.3 has it:
  - **A quoted command name was alias-expanded**: cash looked the expanded word up, so
    `\say`, `'say'`, `"say"`, `s\ay`, `sa''y` and even `$cmd` ran the alias `say`.
    Only a word written with no quoting and nothing to expand may name an alias now.
  - **An alias's value was not alias-expanded**, and **the word after an alias ending in
    a blank was not either**: `alias e2='e1 second'` said "e1: command not found", and
    `alias sp='echo spaced '` left `sp say` alone. The first word of a value is expanded
    again unless it names an alias being expanded, and a blank at the end makes the next
    word one that may name an alias.
  - **A subshell never ran the traps it set itself**: `( trap … EXIT; … )`, `$( … )`, a
    `{ …; } | cat` stage and a `{ …; } &` job ended without their EXIT trap, and a
    subshell's own ERR and DEBUG traps were held back as if inherited. A trap now knows
    the copy of the shell that set it; a subshell runs its own, its EXIT trap as it ends
    (an `exit` there sets its status), and of its parent's only what `set -E` and
    `set -T` pass on.
  - **`trap -p` listed signals without `SIG`, out of order, and quoted nothing**:
    `trap -- '' INT`, in the order of a hash map, and `'echo it's'`. It lists as Bash
    does now: EXIT, the signals by number as `SIGINT`, then DEBUG, ERR and RETURN, each
    command single-quoted so that the line sets the trap again.
- **The corpus's second round** (2026-10-18): 10 pure-sh-bible cases (MIT; POSIX
  trims, splits on `IFS` with `set -f`, `key=val` files, read loops that keep a last
  line with no newline, dirname and basename by expansion, `eval` names, numbers
  checked by `printf %d` and `%f`), all as Git Bash 5.3 has them, and semver-tool 3.4.0
  (Apache-2.0) run whole from a fixture, its regexes, `BASH_REMATCH`, eval'd arrays and
  `set -euo pipefail`. It found:
  - **`.` took options after the script as its own**: `. ./script --help` printed `.`'s
    help and `. ./script -- a` dropped the `--`, where Bash passes both to the script;
    a missing path printed the usage line alone, without Bash's `filename argument
    required`; and `. --help` said `Usage: cash-builtins`, as clap names a command after
    the file name in its first word, which `.` has none of. Options count before the
    path only now, and every builtin's usage names it as it was called.
- **The corpus's third round** (2026-10-18): three more real scripts run whole from
  fixtures, all three as Git Bash 5.3 has them: JSON.sh (MIT; `sh`, `egrep -o` tokens
  through a `read` loop, every option and its errors), spark (MIT; `${@//,/ }`, `<<` in
  `(( ))`, arrays of multibyte ticks) and getoptions (CC0; an option parser generated
  as shell text and `eval`'d, compared to the byte with the generated code, sixteen
  command lines through it, abbreviations, and help from a quoted heredoc).
- **The corpus's fourth round** (2026-10-18): seven cases of cash's own with the Bash
  manual as their spec: `${var@Q}` and the other transformations, `declare -p` for every
  kind, `getopts`, `case` with `;&` and `;;&`, `mapfile` and `read` options,
  here-documents, and the glob options. It found, each fixed as Bash 5.3 has it:
  - **`GLOBIGNORE` was not read**: cash had no trace of it. A set, non-empty
    `GLOBIGNORE` takes out what a glob found that its patterns match, a part of a
    pattern for each part of the path but a pattern ending in `*` taking any parts
    after (`*.log` leaves `sub/x.log`, `s*` takes `s/b`), never `.` or `..`, and lets a
    glob find dot files, as dotglob does. A word with nothing to glob is left alone.
  - **A last `**` did not match its own directory, and `**/` gave an empty word**:
    `sub/**` gives `sub/` first, `./**` gives `./`, `*/**` the directories found without
    the slash, as in Bash; the working directory `**/` reaches is left out.
- **`GLOBIGNORE` did not show in `shopt dotglob`** (found 2026-10-18 by the corpus; the
  user's pick after 1.26.0): Bash turns dotglob on as `GLOBIGNORE` is given patterns
  and off as it is unset. cash has no hook from a variable to an option, so the shell
  notes what `GLOBIGNORE` was when last looked at and takes the same turn where it
  shows, before a word is globbed and when `shopt` reads the options. A `GLOBIGNORE`
  set and unset between two such looks is not seen.
- **Two clipboard tests put back only text** (found 2026-10-18 in the quick lane):
  `cash-win32`'s round trip, in every quick lane, and the `pbcopy` test borrowed the
  clipboard of whoever ran them and put back its text alone, so an image, a file list or
  formatted text there was lost. They skip now unless the clipboard holds only text
  (`clipboard::holds_text_only`), as they skip while the session is locked.
- **extglob was on from the start** (found 2026-10-18 by the corpus; the user's pick
  after 1.26.0): `shopt extglob` said `on` and `+(x)` matched before `shopt -s
  extglob`, a setting taken over from brush so that a script turning extglob on later
  would parse, as cash parsed a script whole. It starts off now, as in Bash. A script
  that turns it on fails to parse whole and is read a command at a time, each line
  parsed as the one before it left the options, as Bash reads every script; `[[`'s
  patterns are extended whatever extglob is, and the tokenizer reads them so inside
  `[[ … ]]`, as Bash's does. Six of cash's own tests used extended patterns without
  turning extglob on, which Bash refuses too; they turn it on, a line before.
- **An alias whose value held an operator ran it as words** (found 2026-10-18 by the
  corpus; the user's pick after 1.26.0, by reading a command at a time): `alias
  multi='echo m1; echo m2'` printed `m1 ; echo m2`, as cash expanded an alias after a
  command's words were parsed (a TODO taken over from brush). Aliases are expanded on
  the tokens now, before parsing, as Bash expands them as it reads (`cash_parser::alias`):
  their `&&`, `;` and `|` are operators, so `gs | less` pipes only the alias's last
  command, as in Bash; an alias may open a loop or an `if`; the first word of its text is
  looked up again, and the word after a text ending in a blank; patterns, `for`'s words,
  here-documents and a redirection's target are not. A script parsed whole runs a
  command at a time and has the rest parsed again when a command changes the aliases or
  extglob (`Shell::run_text`), so an alias defined on a line applies from the next, and a
  function keeps the alias it was defined with, as in Bash. The aliases go to the parser
  as a shared copy named by a number, so a memo's lookup hashes the number, not the table.
  After a change the next command is read by itself, and the rest parsed whole again once
  a command leaves the options as they were, so a block of aliases is a line's parse
  each; parsing the rest again for each had a `.bashrc` of 100 aliases at 265 ms. Timed
  (bench build, against 1.26.0's and Git Bash 5.3): 20,000 commands 106.5 ms (104.3,
  109.6); 100 aliases and 2,000 lines using them 31.8 ms (Bash 51.4; 1.26.0 took 13.9 s
  and failed it, an alias holding an assignment run as a command not found); 30
  functions of 100 lines 44.9 ms (43.8, 51.6).
- **The corpus's fifth round** (2026-10-18, the user's pick after 1.26.0): the alias
  case of cash's own, and three more real scripts run whole from fixtures: mo 3.1.0 (a
  Mustache template engine in Bash), ansi (option parsing and escape tables; its
  `ANSI_FORCE_SUPPORT` keeps `tput` out) and nvm's `nvm.sh`, 5,642 lines that are one
  `{ }` command, read in 74 ms (Git Bash 99, 1.26.0 69). All as Git Bash 5.3 has them
  but `nvm_strip_path`, a known difference (D3): its awk splits a list on `:` that holds
  `$PWD`, which cash spells `C:/…`. The same spelling in a `PATH` a script assigns
  splits there too; noted in TODO.md.
  Left as is: Bash expands the last word of an alias's own text once more, as its reader
  lets go of the text before it looks the word up (`alias echo='echo -n x; echo'`).
- **"directory", never "folder"** (2026-10-18, the user, emphatic: a folder is a GUI
  construct; the file system and Bash say directory): the word is swapped in 329 files,
  cash's messages, `--help` texts and help pages, the docs, comments, identifiers and
  test names, and three test files are renamed. The records in `%LOCALAPPDATA%\cash`
  are `history-directories` and `directories` now; a shell renames the old files once,
  when no record has the new name yet, so an older cash run afterwards starts records of
  its own. `7z` says directory where 7-Zip says folder too (the user's pick):
  `Directories: 2`, `Directory = +` with `-slt`; the oracle's recordings stay as 7-Zip
  wrote them, put in cash's words by the tests, which fail if a "folder" is left. Kept
  as they are: the 7z format's own term (`kFolder`, a group of coders), Windows
  Terminal's menu folders (a UI construct), the `notify` crate's `CreateKind::Folder`,
  the corpus's fixtures (others' scripts), and file names in recorded output.
- **A `case` of many options was slow** (2026-10-18, the corpus's `ansi-codes` timing
  out under load held 1.27.0): each pattern was compiled to a regular expression each
  time it was tried, as compiled ones are kept in an LRU of 64 and the ansi script's
  143 options (about 400 patterns) cycled through it, missing every one. A pattern of
  characters, `?` and `*` alone is now matched directly, as Bash's strmatch does,
  with what the regular expression would give (a test holds the two together). In
  debug builds 400 passes through 400 patterns took 88.8 s and take 1.2 s (Git Bash
  0.26), and `ansi-codes` 7 s and 0.6 s (Git Bash 1.1).
- **`croot`'s top line kept its place whole and cut what it shows** (2026-10-18, CI's
  longer temporary path cut `[directories]`): a deep directory now gives up its start,
  an ellipsis in front, and the picker's mode, flags and search state stay in sight.
- **`man`, step 1: the pages' model** (2026-10-18; the design's section 7): the crate
  `cash-man` reads a page's front matter (`name:`/`names:`, `summary:`, `kind:`,
  `see:`, `draft:`, a topic's `title:`) and its `##` sections, and checks the shape:
  a command's page has Syntax, Examples, How it works, Options, Advanced and Different
  in cash only, each once, in that order; no section is empty; no line of code is wider
  than 80 columns, as code is drawn as written. The pages are embedded deflated at
  build time. The first two are cd and sed, from the complete samples, with upper-case
  placeholders; the width check found four lines of them too wide.
- **`man`, step 2: rendering** (2026-10-18): a page is drawn as lines of styled spans,
  wrapped to a width, each line knowing its section (for the heading kept on the top
  line) and each See-also name a link; colour is chosen only when a line is written,
  so a search can mark the plain text. Code is drawn eight columns in, so the widest
  line of code a page holds is 76 columns with its markdown indent (seven lines of cd
  and sed shortened). In a table of forms or options the text after the gap is prose,
  its code spans drawn as code, as are the lines continuing it at its column; other
  code is what to type, a comment after two spaces grey. `cargo run -p cash-man
  --example draw -- NAME` prints a page as drawn, for whoever writes them.
- **`man`, step 3a: every builtin has a page** (2026-10-18): the help catalogue's 83
  other pages and 10 topics are in `cash-man` now, in its format: Description became
  How it works, Windows notes and the util-linux differences Different in cash,
  Examples came first, other headings subsections, fenced code indented; each is
  marked a draft until written in full. The 124 builtins with no page have a draft of
  their summary and kind. A test in `crates/cash` fails when a builtin has no page or a
  page names no builtin, and `cash-man`'s when a See-also link leads nowhere or a kind
  is unknown. 38 lines of code wider than 76 columns: example comments realigned or
  put above their command, four lines rewritten. The renderer now wraps a long topic
  title, breaks a code span longer than a line at its spaces, and wraps a table's last
  column under itself when the table is wider than the pane (the tools table).
- **`man`, step 3b: `help` on the manual** (2026-10-18): `help NAME` draws the
  builtin's page as `man` will, starting with the name asked for and its own summary
  (`help cmp` starts with cmp, and says the same page covers diff); a draft ends with
  the builtin's own `--help` before its See also, a page written in full (cd, sed) has
  its own Options. `help` alone lists the builtins by the pages' kinds, `help topics`
  and `help search` read the manual, and the install hint and `cash doctor` take
  package names from the manual's tools topic. A page covering several names keeps
  each name's summary (`summary egrep:`), as the old index did. The old catalogue's
  pages, topics, index and build script are gone; `helpdocs` keeps the F1 screen and
  the helpers of `help`'s own lists.
- **`man`, step 4: the browser** (2026-10-18): `man NAME` opens a full-screen browser,
  cash's topics (Getting started first, a new topic) and every command from A to Z on
  the left, the page on the right; typing narrows the list fuzzily by name and
  summary, Enter opens, Tab switches panes, `/` and `n`/`N` find in the page with the
  matches in reverse video, Ctrl-F finds in every page, Enter on a page lists its
  See-also pages on the left, Backspace and Alt-Left/Right go back and forward, `1` to
  `8` and `[` `]` jump between sections, `?` shows the keys, Esc leaves a search and
  then the browser. The section the top line is in stays on the pane's top border.
  Under 100 columns one pane fills the screen. A draft ends with the command's own
  `--help`. Not on a terminal, `man` prints the page as text; `man -k WORD` lists the
  names and summaries that hold WORD. A line of code wider than a narrow pane goes on
  below, never cut. `man` has its own page, written in full.
- **`help NAME` is the short form of a page** (2026-10-18, the design's step 4): the
  name and its summary, the syntax and the examples, and `The whole page: man NAME, or
  help -m NAME`. A draft with no syntax shows the `Usage:` lines of the builtin's own
  `--help` (GNU's `or:` lines too), not the generic `NAME [ARGS]` a bundled tool
  reports. `help -m NAME` is the whole page, a draft ending with its `--help`; topics
  are shown whole. The tests that read options or later sections read `help -m`.
- **`man`, step 5: every example is a test** (2026-10-18): each block of code under a
  page's Examples heading runs under cash in a copy of shared files
  (`crates/cash/tests/man-examples/fixture`, a CRLF file among them), and its output,
  status and the files it changed or removed are checked against what Git Bash 5.3 with
  GNU sed gave, frozen beside them as the corpus's oracle output is
  (`CASH_MAN_EXAMPLES_FREEZE=1`). A block that names a share or the reader's own
  directories follows `<!-- parse only: why -->`, which is not drawn, and is parsed
  with `cash -n`; the examples of cash's own commands run with status 0. Pages written
  in full are checked: cd's three runnable examples and sed's thirteen give what Git
  Bash gave, `sed -i`'s files and `config.ini.bak` included, and man's four run.
- **`find`'s `-execdir`, `-ok` and `-okdir`** (2026-10-18, found writing find's man page):
  `find --help` listed them and cash refused each as an unknown predicate. `-execdir`
  runs in each file's directory with `./name` for `{}`, once a directory with `+`;
  `-ok` and `-okdir` ask as GNU find asks, `< rm ... ./a.txt > ? `, and run on a `y`.
- **`man`'s keys, as the user set them on trying it** (2026-10-18): Up and Down choose a
  topic and its page shows at once, no Enter; PgUp, PgDn, Home and End move in the page,
  Ctrl or Shift with Up or Down a line, Ctrl with PgUp or PgDn a section; no panes taking
  turns and no Tab. Typing narrows the list by name, then summary, then the pages' text
  (which replaces Ctrl-F's search of every page); Ctrl-F finds in the page as it is
  typed, F3 and Shift-F3 go between matches; F1 shows the keys; Esc leaves the search,
  then the filter, then `man`. Under 100 columns the page fills the screen and Up and
  Down still choose.
- **`test -x DIR`, `-O`, `-G` and `-N`** (2026-10-18, found writing test's man page):
  `[ -x . ]` was false; `-O` and `-G` compared a stub uid of 0 with the account's and
  were false for the user's own files; `-N` stopped with "not yet implemented". A
  directory is now executable, `-O` and `-G` compare the file's owner and group with
  the process token's user (or default owner) and primary group by SID, and `-N` asks
  whether the last write is newer than the last access. Git Bash calls every file yours
  (its drives are mounted `noacl`); cash answers as `ls -l` shows.
- **`export` as Bash has it** (2026-10-18, found writing export's man page): `export U`
  on a name not yet set made nothing, so a later `U=5` stayed in the shell; it now makes
  the name exported and unset (`declare -x U`). `export -p` wrote values bare, `"` and
  `$` unescaped, and an array as its first element; it now writes what `declare -p`
  writes, or `export NAME="v"` in POSIX mode. `export -f` and `export -pf` with no names
  listed the variables; they list the exported functions. `export 1bad=x`, `'a b'` and
  `a[1]=2` were taken and dropped; they are refused, status 1, the rest still exported.
  `export -f nosuch` exits 1, not 2. An exported array no longer reaches a program as its
  first element: Bash passes none.
- **`read -t 0` on a closed standard input** (2026-10-18, found writing read's man
  page): it said "read error: Bad file descriptor"; `-t 0` only asks whether input
  waits, and Bash answers 1 with no message.
- **`alias` names as Bash checks and lists them** (2026-10-18, found writing alias's man
  page): `alias 'a b=1'` and `alias x/y=2` were taken; Bash refuses a name with a blank,
  a quote, `/`, `$` or one of `()<>;&|`, "invalid alias name", status 1, and so does
  cash. A name starting with `-` lists as `alias -- -x='...'`, so it reads back.
- **`command`, `exec` and `pwd` in Bash's words** (2026-10-18, found writing the alias
  and pwd pages): `command nosuch` said `command: nosuch: command not found`; Bash names
  the program alone, as for any command not found. `exec ./nosuch` said `exec: ./nosuch:
  not found`; for a path Bash says `./nosuch: No such file or directory` (a name not
  found is still `exec: x: not found`). `pwd extra` was refused as an invalid option,
  status 2; Bash ignores the operand.
- **The manual's first twenty pages written in full** (2026-10-18, man step 6): cd, sed,
  man, ls, grep, find, cp, mv, rm, cat, echo, printf, test, read, export, alias, pwd,
  mkdir, tar and awk, each with Syntax, Examples, How it works, Options, Advanced and
  Different in cash. Every example runs against Git Bash 5.3 with the GNU tools and its
  output is frozen in `tests/man-examples`; an archive the example makes is named, not
  shown, as its bytes hold the fixture's new times. Writing them found and fixed bugs
  in find, rm, test, export, read, alias, command, exec and pwd, and filed the rest in
  TODO.md.
- **A Windows variable spelled in another case is that variable** (2026-10-18, found
  writing export's man page; the user picked the fix and its limit): reading folded
  case already, but an assignment was exact, so `export Path=/x` made a second variable
  beside `PATH`; the shell went on looking commands up in `PATH`, and a program got one
  of the two in hash order (`mixed` three times in five). Now an assignment to a name
  that differs only in case from one inherited from Windows lands on it, unless the
  name is all in lower case: `for path in "$@"` and `local temp` stay a script's own,
  as in Bash, so its command lookup holds. A script's own `$Foo` and `$FOO` stay two.
  A program gets twins that remain in a fixed order, the inherited spelling last, so it
  wins. Creating 20,000 capitalised variables takes as long as before.
- **Eleven more uutils tools made cash's own, their errors in GNU's words** (2026-10-18,
  found writing the man pages; the user picked own copies of every tool that differed):
  cat, cp, ln, mkdir, mv, paste, pr, ptx, rmdir, shred and tac join rm, sort, tee, uniq,
  shuf, head and tail in `crates/cash-uutils`, about 15,000 lines, Windows only. A sweep
  of every bundled tool against Git Bash 5.3 found two kinds of difference, both gone:
  Windows' text for an error ("The system cannot find the file specified. (os error
  2)") where GNU prints `strerror`'s (`No such file or directory`), now from one map in
  `cash_win32::errno` that gives each Windows code Cygwin's errno, used by cash-core's
  `os_error_text`, the taken-in tools (sort and tee too) and cash's own `stat`; and
  messages shaped otherwise (`cp: cannot stat 'x'`, `cannot create regular file`, `mv:
  cannot move 'a' to 'b'`, `mkdir: cannot create directory 'a'`, `ln: failed to access`,
  `paste: x:`, `ptx`'s quoting, `shred`'s `failed to open for writing`). `mv` fell back
  to copying on any failed rename and called every failure of it `Permission denied`;
  it falls back only across drives or onto a held target, with the real error. `ln -s`
  makes a junction for a directory without the privilege for symbolic links, as spec
  D27 decided and uutils' `ln` never did, and says how to get the privilege for a file.
  `-i` answered no exits 1 in cp, mv and ln, as GNU coreutils 9.12 does (Git Bash's
  8.32 exits 0). Every possible panic clippy found in the copied code is rewritten or
  says why it cannot happen. Tests: each tool's error, the release each names,
  `ln -s`'s junction.
- **`cp` dates a copy now, and `cp -p` copies a read-only file** (2026-10-18, found
  writing cp's man page): Windows' `CopyFileExW` gave every copy its source's
  modification time, so `make` took a fresh copy for an old file; now a copy gets the
  present time unless `-p`, `-a` or `--preserve=timestamps` keeps the source's. `cp -p`
  of a read-only file made the copy and failed with `Access is denied`, as Windows sets
  no time on a read-only file; the times are set with the attribute lifted for the
  moment. Each case was checked against Git Bash.
- **An alias is read as text into the line** (2026-10-18, found writing alias's man
  page): aliases were expanded on the tokens after a line was read, each alias's text
  tokenized alone, so `alias q="echo it's"; q quoted'` stopped with an unterminated
  quote where Bash prints `its quoted`. The tokenizer now reads an alias's text back into
  its input as Bash does (`Tokenizer::push_alias`), the blank after the alias's word
  kept, while the parser reads the tokens as they come (`alias::Expander`); an alias
  whose text is being read is not looked up again, and a text ending in a blank has
  the next word looked up. An alias after a here-document's `<<EOF` on its line, whose
  tokens wait behind the body, has its text tokenized alone as before. Chained aliases,
  keywords, `case`, here-documents, functions and the end of the input were each
  checked against Git Bash.
- **The manual's second twenty pages written in full** (2026-10-18, man step 7, batch 2):
  head, tail, sort, uniq, wc, cut, tr, xargs, touch, ln, chmod, diff (and cmp), date,
  kill, jobs (and fg, bg), history, type, source, set and env. Every option the pages
  list was run in cash and in Git Bash, and every example runs against Git Bash, its
  output frozen. Two options did not work and now do: `chmod -c`, which says only what
  changed, and `chmod --reference=FILE`, both refused as modes.
- **`man`'s screen made plain, as the user asked on trying it** (2026-10-18): the list,
  one line, and the page, no boxes and no titles; the page's first line stays on top as
  it scrolls, and the bottom line is in reverse video. The list's top row says "type to
  filter", and shows what is typed. The chosen entry was one row under the screen
  whenever the line between the topics and the commands was in view, as the scrolling
  did not count it; it counts it. Topics are titled short enough for the list
  ("Configuration", "Elevation: sudo, UAC") and drawn as the commands are. A page's
  head reads `name - summary`, in `help` too: no em dash. Shift with Up or Down moves
  the page a line: Windows Terminal takes Ctrl with them (scroll to a mark). Pages are
  drawn once for a width, keys held down are taken before the next frame, and only the
  rows that changed are written, in one synchronized update.
- **The manual's third twenty pages written in full** (2026-10-18, batch 3): unset,
  declare, local, trap, wait, shopt, getopts, eval, exit, which, tee, less (and more),
  mapfile, printenv, rmdir, basename, dirname, seq, sleep and du. Every option the pages
  list was run in cash and in Git Bash, and every example runs against Git Bash, its
  output frozen. `less` lists only the options cash's pager has.
- **The manual's fourth twenty pages written in full** (2026-10-19, batch 4): readonly,
  return, shift, break, continue, command, exec, let, pushd, popd, dirs, umask, mktemp
  (with realpath and readlink), stat, timeout, nl, paste, tac, comm and uname. Every
  option the pages list was run in cash and in Git Bash, and every example runs against
  Git Bash, its output frozen. Two options did not work and now do: `readonly -f`
  printed the function and left it writable, and makes it read-only, as `declare -rf`
  does (defining it again or `unset -f` fails, and `declare -F` lists it `-fr`, an
  exported one `-fx`); `declare -x` listed every function among the exported variables,
  and lists none. `shift` past the end failed with 2, where Bash fails with 1. The `ln`
  page's last example linked to a target that was not there, which only a shell with
  the symlink privilege can do, so it passed here and failed on CI; it points a link
  at another directory, and is only parsed.
- **The manual's fifth twenty pages written in full** (2026-10-19, batch 5): true,
  false, `:`, hash, unalias, builtin, times, ulimit, caller, id (with groups), whoami,
  hostname, nproc, yes, base64, sha256sum (with md5sum, sha1sum, sha224sum, sha384sum,
  sha512sum and b2sum, one page now), rev, fold and join; `.` joined `source`'s page.
  Every option was run in cash and in Git Bash, and every example runs against Git
  Bash, its output frozen. `id` and `groups` did not do what their options said: `-G`
  and `groups` printed the account itself, not its groups; `-r`, `-z` and a USER were
  refused; `id -un` said `PC\user` where `whoami` says `user`. They list the groups the
  shell's token works with (Administrators only elevated), or another account's local
  groups, each id its SID's last part, and take GNU's options. A script being run was
  missing from `FUNCNAME`, `BASH_SOURCE` and `BASH_LINENO`, and `caller` named its
  frame `source`; it is `main`, from line 0, as in Bash.
- **The manual's sixth twenty pages written in full** (2026-10-19, batch 6): enable,
  help, disown, nice (with renice), nohup, ps, pgrep (with pkill, pidof, killall),
  tree, df, free, uptime, gzip (with gunzip, zcat), xxd, od, shuf, split, truncate, tty,
  clear (with reset) and expr; readarray joined mapfile's page, typeset declare's, base32
  and basenc base64's, dir and vdir ls's. Every option was run in cash, and in Git Bash
  where it has the tool; the examples of tools Git Bash lacks (tree, free, pgrep,
  uptime) or answers for MSYS2 (ps) are only parsed. `dir` and `vdir` were uutils' own
  `ls`, whose modes are not cash's (`-rwxrwxrwx` for every file); they are cash's `ls`
  with GNU's `-C -b` and `-l -b`. `split --filter` is refused by uutils on Windows: in
  TODO.md.
- **The manual's seventh twenty pages written in full** (2026-10-19, batch 7): column,
  fmt, expand, unexpand, cksum, sum, hexdump, dos2unix (with unix2dos), iconv, install,
  mkfifo, link, unlink, uuidgen, watch, factor, numfmt, pathchk, csplit and dd. Every
  option was run in cash and in Git Bash; the examples of tools Git Bash lacks
  (hexdump, uuidgen, watch) are only parsed. `install` refused every GNU option it did
  not have, so a Makefile's `install -c -o root -m 755 prog DIR` failed: it takes `-c`,
  `-p`, `-b`, `--backup`, `-S`, `-T`, and `-o`/`-g` said once, as chown says it; and the
  copy gets the time it is made, as GNU gives it, unless `-p`.
- **The manual's eighth twenty pages written in full** (2026-10-19, batch 8): 7z, bzip2,
  xz, zstd, zip, unzip (with zipinfo), rar (with unrar), chown, shred, sync, tsort, pr,
  ptx, arch, hostid, logname, users, who, pinky and logger. Every option was run in
  cash, and in Git Bash where it has the tool; the examples of tools it lacks (7z, rar,
  zip, zstd) are only parsed. Three tools did not do what their options said and now
  do: `sync` and `sync FILE` failed with status 5 unelevated, as uutils flushed every
  volume, and `-d` did nothing; cash's own `sync` flushes the file, and the drives
  Windows lets it. `pr -2` put its second column at 40 where GNU puts it at 36, `-s`
  padded, `-o` came before every column, and `pr -m -t` printed a row of padding to the
  page's end; its columns are GNU's bytes now. `who`, `pinky` and `users` read
  `quser.exe`'s text: in TODO.md, with GNU's options they lack.
- **The manual's ninth twenty pages written in full** (2026-10-19, batch 9): bind,
  compgen, complete, compopt, fc, suspend, logout, getopt, dircolors, locale, getconf,
  stty, tput, stdbuf, flock, fuser (with lsof), nc, ping, ss and top. Every option was
  run in cash and in Git Bash; the examples that act on the reader's console, network,
  processes or history, and those Git Bash cannot run (flock on a descriptor), are only
  parsed. Nothing needed a fix: the differences found are Windows' (ping's `-n` is
  Linux's, cash's flock locks a descriptor) or edges left alone.
- **The manual's last pages written in full** (2026-10-19, batch 10): abbr, bc,
  cashctl (with cashinfo), coolfetch, croot, detach, elevate, pbcopy (with pbpaste),
  prevd (with nextd and cdh), pstree, start, su, sudo, sudoedit, where, winpath,
  xdg-open and z. No page is a draft now. The options were run in cash, with APPDATA
  and LOCALAPPDATA a scratch directory, so `abbr` and `z` touched nothing of the
  user's; the commands that open windows, ask UAC or use the clipboard were asked for
  their help only, and their examples are only parsed. `start --help` had an em dash;
  a test now keeps every builtin's `--help` without one.
- **The two spellings a directory takes in `PATH`, documented** (2026-10-19, TODO's
  item 2, the user's pick): an entry keeps the spelling it was given, so
  `PATH="$PWD/bin:$PATH"` holds `C:/...`, which a script that splits `PATH` on `:`
  cuts in two (nvm's `nvm_strip_path` did); `PATH="$(winpath -u "$PWD")/bin:$PATH"`
  adds it in the Unix spelling. Spelling assigned entries `/c/` was turned down, as
  `case ":$PATH:" in *":$HOME/bin:"*`, the idiom that adds a directory once, would
  then never find what it added. In `help paths`, `help differences` and spec §4 row 2.
- **A corpus round: five real scripts** (2026-10-19, TODO's item 5, the user's pick of
  the sources): wait-for-it, shUnit2, shFlags, bashmarks and two git-extras commands,
  unchanged, with their terms in `tests/corpus/SOURCES.md`. Four fixes came of it.
  `exec mktemp` ran Git's GNU mktemp, whose `/tmp` name cash's tools cannot open: a
  bundled tool's name after `exec` is cash's tool. With a here-document pending on the
  line, the words inside a `${...}` or `$(...)` were held for after its body:
  `cat <<EOF >"${f}"` read `${}`. `/bin/rm` and `/usr/bin/env` were not found: such a
  path runs cash's command when no file is there (the user's pick, D7). Every variable
  was read in another case, so shflags' `FLAGS_HELP` read its `FLAGS_help`: only a
  variable from Windows or cash's own start is now (the user's pick, D31). A case may
  give itself more time with a `# timeout: N` line. git-summary's author column is
  matched without its padding: util-linux `column` pads a cell holding a tab as if the
  tab were its escape.
- **Three `cd` gaps closed** (2026-10-19, the phase after 1.32.0, item 1): `cd C:` set
  `$PWD` to `C:`; it is the drive's root, as in Git Bash. `shopt -s cdable_vars` had no
  effect; a name that is no directory goes to the directory its variable holds, which is
  printed, as Bash prints it. `cd -@` said "not yet implemented" with status 99; it is an
  invalid option with the usage line and status 2, as in Git Bash. `cd -P -e` said "not
  yet implemented" too: cash resolves the real name before it enters the directory, so
  `-e` changes nothing and is taken.
