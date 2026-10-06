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
| 21 | 1.5.0 | Console, clipboard and the remaining small tools |
| 22 | 1.6.0 | A per-user installer, `cash --update`, and winget |
| 23 | 1.7.0 | `grep`, `diff` and `cmp`: the standing rule reversed |
| 24 | 1.8.0 | `gzip`, `gunzip` and `zcat` |
| 25 | 1.8.0 | Compatibility corners: `/dev/tcp`, `command_not_found_handle`, persistent `abbr`, kinder refusals, `getconf`, `locale` |

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
   keep the old file and new tabs get the new one. Inno copies into the version folder
   and runs `cash --install-finish` (new), which moves the junction, writes the Terminal
   profile, refreshes the tool links, runs `--init-rc --once`, and sweeps old version
   folders nothing runs. `CloseApplications=no`: never Ctrl-C the user's shells.
3. **Tool links on PATH by default**: the install task "Make the tools programs on PATH"
   is on, so `ls.exe`, `sed.exe`, `awk.exe`, … are on the user PATH ahead of Git's;
   a checkbox and `/TASKS=` turn it off. `cash.exe` itself goes on the user PATH always.
   PATH is written by cash (`--link-tools --add-to-path`), never by Inno's `[Registry]`.
4. **Uninstall** (Apps & features entry, per-user): `cash --unlink-tools`,
   `--remove-terminal-profile`, the PATH entries, the folder; the user's `~/.bashrc`,
   history and config stay.
5. **`cash --update`**: asks GitHub's releases API for the latest version, downloads the
   zip, verifies its `.sha256`, unpacks into a new version folder, moves the junction and
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
    cash whose folder is on no PATH still reads `ok`. Add a `path` line: `ok` when the
    folder of the running `cash.exe` (`current` for an installed one, Scoop's shims for
    Scoop's) is on the user or machine PATH, else `note  path  C:\…\cash.exe is not on
    your PATH: new windows will not find cash` with the fix `cash --add-to-path`. That
    command (and `--remove-from-path`) adds or removes the exe's own folder on the user
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
`install` line. Two details differ from the plan above: Windows renames a folder whose
exe is running (only the delete fails), so a version folder is judged in use by a
write-open of its `cash.exe` before anything is touched; and `ln -s DIR` makes a real
symbolic link, which needs a privilege, so the `current` junction is set by
`FSCTL_SET_REPARSE_POINT` in place, never half-moved. The installer compiles here (Inno
Setup 6.7.3 from Scoop, 11.7 MB from the 1.5.0 files); an end-to-end silent install,
`--update --check` and uninstall were exercised with HOME, LOCALAPPDATA and the registry
key for the user PATH pointed at a scratch folder, since this machine's cash is Scoop's
and the installer refuses to run beside one. Found there: Inno's uninstaller runs with
Windows' redirection-trust mitigation, inherited by what it starts, so no process of its
may cross the `current` junction ("the path cannot be traversed because it contains an
untrusted mount point"); the uninstaller therefore runs `--install-remove` from a version
folder it finds itself. Windows Terminal, the shell and every ordinary process cross the
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
   without `-f`, `-r` over folders, the `.tgz`/`.taz` suffix rules, `GZIP` environment
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

## Phase 26. `tar`

Chosen by the user on 2026-10-07, by pick list: cash's own `tar`, GNU tar 1.35's interface,
messages and exit codes, on the `tar` crate (composefs/tar-rs, MIT or Apache-2.0), with
every common compression in pure Rust; reopens D77's "xz and bzip2 stay with `tar.exe`".
Windows' own `tar.exe` (bsdtar 3.8.8) stays reachable by its path and by `enable -n tar`.
uutils/tar (0.0.1: `-c -x -t -f -z --zstd -v -P` only) and `libzstd-rs-sys` (a
prerelease without a Rust API) were looked at and passed over; `xz2` and `liblzma` are C.

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
3. **Compression**: `-z` (`flate2`, already in), `-j` (`bzip2` on `libbz2-rs-sys`, the
   license `bzip2-1.0.6` accepted for it), `-J`, `--lzma`, `--lzip` (`lzma-rust2`),
   `--zstd` (`ruzstd`: complete reading, writing at its fast level, about zstd's level
   1, so archives somewhat larger than GNU's level 3); `-Z`, `--lzop` and
   `-I PROGRAM` said plainly. TODO: move `--zstd` to `libzstd-rs-sys` once it has a Rust
   API and a release.
4. **Changing archives**: `-r`, `-u`, `-A`, `-d` (`--compare`), `--delete`, on
   uncompressed archives as GNU allows them.
5. **Windows**: modes from `ls`'s answer, owner and group names and ids from `id`'s,
   read-only kept, times through `filetime`; symbolic links made when Windows allows it
   (Developer Mode), else GNU's "Cannot create symlink" and status 2; names Windows cannot
   hold reported, not mangled; long paths.
6. The page, `builtins.md`, `help tools`, `CARRIED`, `DELIBERATE_SHADOWS` (System32's
   `tar.exe`), the README's tool list, and an oracle: `tests/oracle/tar_cases.sh` under
   GNU tar 1.35 in WSL, with GNU gzip, bzip2, xz and zstd beside it.

---

## Found along the way

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
  are cash's. They now say the listed cash is that window. **Waits for the user's yes:**
  push the synced notes, a local commit in `C:\Users\thraa\github\scoop-bucket`, to
  `tomcoolpxl/scoop-bucket`.
- **CI's annotations show four `ENOENT ... opendir 'D:\a\cash\cash\target\doc\cash_core\tests\trybuild'`**
  (and `...\tests\target`) errors on every green run, 1.8.0's and 1.8.1's included
  (seen 2026-10-06). The job passes; the cleanup of the Rust Cache step
  (`Swatinem/rust-cache`, `.github/workflows/ci.yml`) walks `target\doc` and trips on
  folders that are gone. Find what makes `target\doc\cash_core\tests` and keep it out of
  the documentation, or exclude `target\doc` from the cache.
- **cash never runs a user's `command_not_found_handle`** (found building the install
  hint, 2026-10-06). Bash calls that function, when defined, with the command and its
  arguments instead of printing `command not found`, and its status becomes the
  command's. cash prints the message regardless; the hint is suppressed when the function
  is defined, as decided, but the function itself does not run. Implement Bash's
  behaviour, with the hint only when no function is defined.
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
