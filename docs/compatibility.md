# Bash compatibility

cash reports Bash 5.3.15 through `$BASH_VERSION`: every Bash 5.3 change is implemented,
checked against Git Bash 5.3.15, or documented as a deliberate difference
([audit](../research/bash-reference/bash-5.3-audit.md)). Among the 5.3 additions:
`${ command; }` runs a substitution in the current shell, preserving changes to shell
variables, while `${| command; }` expands a temporary `REPLY` and leaves command output
on stdout. `compgen -V name` stores candidates in the indexed array `name` instead of
printing them; no matches clear the array and return status 1. `source -p PATH file` and
`. -p PATH file` use an explicit source search path. These additions do not imply full
Bash 5.3 compatibility. The [Bash source comparison](../research/bash-reference/README.md)
records the behavioral probes and how to rerun them.

Known gaps in the advertised Bash 5.2 interface are tracked in the
[Bash 5.2 gap audit](../research/bash-reference/bash-5.2-gaps.md). The
[roadmap](ROADMAP.md) keeps the feature order explicit: Bash 5.2, native `awk` and `sed`,
and the bundled userland (`stat`, `tty`, `install`, `pathchk`, `nohup`, `who`, `users`,
`pinky`, `logname`, `hostid`) are done; Bash 5.3 is in progress, with every
script-observable item probed against a real Bash 5.3 in the
[Bash 5.3 audit](../research/bash-reference/bash-5.3-audit.md). The selected source
candidates and release gates are recorded in the
[`posixutils-rs` AWK evaluation](../research/posixutils-rs-evaluation.md) and
[`uutils/sed` evaluation](../research/uutils-sed-evaluation.md).

cash's deliberate divergences from bash are documented in [spec.md](spec.md) section 4,
and `cash help differences` prints them. That list is meant to stay short.

## History and job control

History and job-control scripts can use `history -n`/`-r`/`-p`, `fc` editor mode,
and `jobs -n`. `BASH_COMMAND` preserves the command text before expansion, assigning
`SECONDS` resets its signed live counter, and symbolic `umask` forms round-trip even on
Windows, where the mask is remembered rather than applied to ACL-based file creation.
Windows does not load Bash C-ABI builtin modules: `enable -f` and `enable -d` report
"dynamic loading not available" with Bash-compatible status 2.

Interactive commands are persisted in `~/.cash_history`. Removing that file does not
erase entries already loaded by a running shell; use `history -c` to clear memory and
`history -w` to replace the file with the cleared history. cash's test harness gives
interactive cases an isolated temporary home, so test commands never enter user history.

## `read -e`

Interactive `read -e` and `read -E` support editable `-i` text, cursor movement, insertion,
Backspace/Delete, Home/End, and the common Ctrl-A/E/U/K/W controls. `read -s` and
timeouts continue to apply while editing. `-E` is accepted with the same editor but does
not yet add Bash completion. This is a compact built-in editor; GNU Readline history,
completion, and custom `bind` keymaps are outside that surface. The remaining `read`
details and source-level comparisons are in the
[Bash 5.2 gap audit](../research/bash-reference/bash-5.2-gaps.md). A source-level
[Rustyline evaluation](../research/rustyline-evaluation.md) records why cash uses it as a
source reference rather than adding another prompt backend or replacing the
descriptor-aware `read` editor.

## The tools

Missing and deliberately limited command-line options are tracked in the
[builtin option audit](../research/builtin-option-gaps.md), including cash's native
Windows `ps`, `pgrep`, `tree`, `top`, `find`, and `xargs` implementations. The console,
clipboard and small tools (`tput`, `stty`, `iconv`, `column`, `xxd`, `hexdump`,
`uuidgen`, `xdg-open`, `pbcopy`/`pbpaste`, `watch`, `free`, `nice`/`renice`, `flock` and
`nc`) are each checked against their Linux original where one exists.

`top` follows procps: the load average follows Linux's definition, tasks running or
waiting for a processor, exponentially averaged over 1, 5, and 15 minutes. Windows does
not preserve that history before `top` starts, and the value does not include Linux's
uninterruptible sleep state. Swap, nice values, and the Linux buffer/cache split are
omitted. `Commit` shows Windows committed memory against its limit and peak. `PRI` is the
Windows base scheduling priority; on supported Windows releases, `SHR` is the shared part
of the resident working set. The `S` column means no CPU time accrued in the latest
sample; `R` means some did. Sampling is local and cheap: one snapshot of every process and
thread, each process's CPU and memory, and machine CPU, uptime and memory; a process's
account is looked up once.

`xargs` prefers enabled cash builtins, so `xargs echo` uses cash's `echo` regardless of
which `echo.exe` is on `PATH`. Explicit executable paths bypass builtins. Which `PATH`
cash sees depends on which shell launched it, because it inherits that shell's `PATH`.
