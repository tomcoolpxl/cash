# `/` above the drives: design

Found in the phase after 1.40.0, in direnv's stdlib: a loop that climbs with `cd ..` until
`$PWD` is `/` never ends in cash. The user's pick (2026-10-20, TODO.md): a design first,
then a pick list. This page is the design; its open questions went to the user.

**Decided, then reversed** (the user, 2026-10-20, decisions.md D83). 4.1 was picked,
with `usr` and `bin` for the tools added, and built: directories that exist only in cash,
one module saying what each holds. Before it was released the user found what this page
never weighed (section 8): `rm -rf c` at `/` handed `rm` the whole `C:` drive, refused
only by luck; `/usr/bin/sed` was listed as a file and could not be read; the same path
meant different things to cash, MSYS programs and Windows programs. Asked what it was
worth, the answer was: almost nothing beyond the `cd ..` climb, which a spelling ends.
The commit was reverted, unreleased, and the spelling of 8.2 built instead.

## 1. Why

Three idioms climb from a directory to the top. Each ends in Git Bash; in cash, one or
all three never end, and the script hangs when what it looks for is not above it:

| Idiom | Found in | Git Bash | cash | cash, `unixpaths` |
| --- | --- | --- | --- | --- |
| `cd ..` until `$PWD` is `/` | direnv's `find_up`, "find the project root" functions | ends at `/` | hangs at `C:/` | hangs at `/c` |
| `dirname` until `/` | asdf 0.15 | ends at `/` | hangs at `C:` | ends at `/` |
| `${dir%/*}` until empty | rbenv, pyenv, nvm | ends | hangs at `C:` | ends |

The second and third hang in Git Bash too when given `C:/...`: `dirname C:` is `C:`, and
`C:` has no `/` to cut. They are why the `/c/` mode exists (D82,
`research/unix-paths-mode-design.md`). The first is the one that mode left: at a drive's
top, `cd ..` stays where it is, in both spellings.

## 2. What `/` is today

| | Git Bash | cash | cash, `unixpaths` |
| --- | --- | --- | --- |
| `cd /` | `/`, Git's install directory | `C:/`, the current drive's root | `/c` |
| `cd ..` at a drive's top | `/` | stays at `C:/` | stays at `/c` |
| `ls /` | Git's `bin`, `etc`, `usr` | the current drive's root | the same |
| `/Windows` | no such directory | `C:/Windows` (current drive) | the same |
| `/c`, `/tmp` | mounts: drive C, the temp directory | the same, opened by cash | the same |
| a program started at `/` | runs in `C:\Program Files\Git` | `C:\` | `C:\` |

So in cash `/` is Windows' `\`: the root of the current drive, which is the drive of
`$PWD`. Under `unixpaths`, `dirname /c` is already `/` and `/c` already reads as a child
of `/`; only `cd` does not agree.

## 3. What a change can reach

- **The default mode** spells paths `C:/...` (D3). Its string climbs end at `C:`, never at
  `/`, and no change to `cd` alters that. Only the `cd ..` climb could be ended there, and
  only by giving `$PWD` a value, `/`, that is not a `C:/` path: a Windows program handed
  `$PWD` (`explorer "$PWD"`, `cmd /c "cd /d $PWD"`) would get `/`.
- **`unixpaths`** spells paths as Git Bash does. Its string climbs end already; ending the
  `cd ..` climb finishes what the mode is for, and the mode already turns `$PWD` into
  `C:/...` for a Windows program (D82 item 3).

## 4. The design

### 4.1 Under `unixpaths`: `/` is the directory above the drives

- `cd ..` from a drive's top (`/c`, `/d`) gives `/`; `cd ..` at `/` stays at `/`. `cd /`
  gives `/` too, as in Git Bash, not the current drive's root.
- At `/`, `$PWD`, `pwd` and `pwd -P` are `/`. `dirname`, `${PWD%/*}` and the `cd`
  climb then all end where Git Bash's end.
- **What is in `/`:** the drives (`c`, `d`, `e`: each a directory), and cash's names
  `tmp` and `dev`. `ls /`, `echo /*`, `[[ -d /c ]]`, `cd c` and a glob at `/` see those.
  This is the mode's model made whole: `/c` is drive C because `/` holds the drives.
- **A path under `/` that is not a drive or one of cash's names** (`/Windows`,
  `/Users/x`) keeps today's meaning, the root of the drive cash was on before `/`, so a
  script that names `/Windows` keeps working. It opens, but `ls /` does not list it.
- **A program started at `/`** runs in the root of the drive cash came from (Windows
  needs a real directory); its `PWD` is that root in the Windows form, as D82 item 3 hands
  any directory.
- `cd -` and `$OLDPWD` work across `/`; `dirs` shows `/`.

### 4.2 The default mode: unchanged

`C:/..` stays `C:/`, as in `cmd` and PowerShell. The `cd ..` climb still hangs there, as
the other two climbs do; the paths topic says so, and points to `unixpaths` for scripts
written for Unix, which is what that mode is for.

### 4.3 Turned down

- **`/` above the drives in the default mode too.** It ends only one of the three climbs
  there, and puts a non-Windows path, `/`, in `$PWD` of a mode whose point is that every
  path it prints works in a Windows program. `cd /` and `ls /` would change meaning for
  every user, not only those who chose the mode.
- **`/` as Git's install directory, as Git Bash has it.** cash does not require Git for
  Windows, and a mount table is outside D82 (its 4.3).
- **`cd ..` failing at a drive's top.** It ends a loop that checks (`cd .. || break`), not
  direnv's, and stops a script under `set -e` that never needed to check.

## 5. Side effects

- Under `unixpaths`, `cd /` no longer lands in the drive's root, and `ls /` lists drives,
  not that root's files. A user who typed `cd /` to reach `C:\` types `cd /c`.
- A directory named like a drive at a drive's root (`C:\c`, which exists on some disks) is
  hidden under `/c`, as it is already.
- `/` is a directory of the shell's alone: Windows has no such directory, so a program
  given `/` as an argument by a script fails as it does today.
- No effect in the default mode, or on MSYS programs, which get `/` and read it as theirs.

## 6. Size

Small to medium: `cd`'s parent of a drive's top and `cd /` (one place, the shell's own
directory, D10); the open of `/` itself for `ls`, globs and `test` (one listing of drives
and cash's names); the working directory of a program started at `/`; tests for each, and
direnv's `find_up` miss back in its unixpaths case.

## 7. Open questions, for the pick list

1. Build 4.1 (`/` above the drives under `unixpaths`, the default mode unchanged:
   recommended), also do it in the default mode, or leave both as they are and document
   the hang.
2. What `ls /` lists under `unixpaths`: the drives and `tmp` and `dev` (recommended), or
   the drives beside the current drive's root's files, as one merged listing.
3. When: this phase, before 1.41.0 (recommended, as it ends a hang), or the next.

## 8. Reconsidered: what it buys, what it costs

### 8.1 The weighing this page left out

cash already had names that translate where they are used, each to something real: `/dev/null`,
`/dev/stdin`, `/dev/fd/N` and `/dev/tcp` are opened as devices and descriptors (D7); `/tmp` and
`/c/x` are directories on disk; `/usr/bin/NAME` and `/bin/NAME` run as cash's command when
run. None needs a directory to be listed or entered; each lives at one point, an open, a
path, an exec, and they carry nearly all the value.

Directories that exist only in cash add little: the `cd ..` climb ends, and `ls /usr/bin`
answers something, which a script looking for a tool asks `command -v` for. They cost a
defined answer for every operation (list, test, read, write, create, delete, rename, enter,
run, link, walk, pass along) and every kind of program: cash, its tools run as processes of
their own, MSYS programs, whose `/` is Git's, and Windows programs, which have none. Built,
they gave destructive commands every drive (`rm -rf c` at `/` handed `rm` `C:/`, which its
root guard refused only because the operand ended in `/`; `rm -rf *` emptied TEMP; `find /
-delete` walked every drive), listed `/usr/bin/NAME` as a file nothing could read, could hang
on a disconnected network drive, and printed paths no other program understands, against
D3. High cost and risk for a gain the next section gives.

### 8.2 The spelling (built)

Under `unixpaths`, `cd /` and a `cd ..` past a drive's top go to the root of the shell's
drive, as before, and `$PWD` spells it `/`; `cd ..` at `/` stays. Nothing else: `/` stays
what it was when cash opens a path, the root of the drive the shell is on, so `rm -rf c` at
`/` is the directory `C:\c`, and no name stands for a drive. `pwd -W` names the root, a
program started there runs in it and gets it as `PWD`, and a cash keeps `/`. `$OLDPWD`,
`pushd`'s stack and the history keep the root itself, as `/` is the root of whichever drive
the shell is on. The oddity: a drive's root has two spellings, `/c` and `/`, after which
way it was reached. The cost: a flag on the shell's directory, the function that spells
`$PWD`, and the `PWD` a program is handed.
