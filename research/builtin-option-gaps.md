# Builtin command option gaps

Audit date: 2026-09-24. This covers cash's Bash-compatible builtins and the native
Windows commands cash implements itself. It does not enumerate every obscure upstream
flag, and it does not treat the uutils commands cash embeds unchanged as cash-owned
implementations. The purpose is to stop useful options from being discovered by accident.

References used for the comparison:

- GNU Bash source at `C:/Users/thraa/github/bash-reference`, commit `9c465866` (the
  advertised interface is Bash 5.2)
- [Bash builtin reference](https://www.gnu.org/software/bash/manual/html_node/Bash-Builtins.html)
- [procps `ps`](https://man7.org/linux/man-pages/man1/ps.1.html) and
  [`pgrep`](https://man7.org/linux/man-pages/man1/pgrep.1.html)
- [GNU `xargs` options](https://www.gnu.org/software/findutils/manual/html_node/find_html/xargs-options.html)

## Correctness and discoverability issues

These deserve attention before adding breadth because cash accepts the spelling or
implements the behavior incompletely.

| Command | Gap | Cost | Status |
|---|---|---:|---|
| `ps` | `-j` and therefore `-efj` worked but `ps --help` did not advertise it. | Small | Fixed in this audit: `-j` is a declared option and help explains its Windows base-priority view. |
| `xargs -I` | Cash split replacement input on spaces. GNU/POSIX replacement mode consumes one logical line per item, so `a b` must remain one item. | Small | Fixed in this audit, including CRLF input, leading blanks, blank lines, and preserved trailing blanks. |
| `xargs -P N` | The option is accepted but execution is serial. This obeys the numeric upper bound while missing the reason users specify `-P`. | Medium | Open. Implement bounded concurrent execution or reject values above 1 clearly. |
| `xargs -I{}` | The common attached-value spelling is treated as the command name; only `-I {}` works. | Small | Open; argument parsing needs a regression before changing the trailing command parser. |
| `declare -t` | Function tracing is accepted but not recorded. | Medium | Open; silently inert flags are worse than explicit refusal. |
| `bind -m vi-*` | Mutating bindings under a vi keymap is silently ignored. | Large | Open; either implement vi keymaps or report the limitation. |
| `complete -o bashdefault`, `complete -A service` | Both are accepted but generate no candidates. | Medium | Open; currently visible only in debug tracing. |
| `ls --help` | Prints only a two-line synopsis even though the builtin has a useful option set. | Small | Open; help should enumerate the implemented flags. |

## Useful, cheap native-command gaps

These use data the implementation already has or require only local parsing/sorting.
They are good candidates for the next compatibility pass.

| Command | Missing useful surface |
|---|---|
| `ps` | `-p`/`--pid`, `--ppid`, `--sort`, `--no-headers`; a small supported-field form of `-o` is feasible even though arbitrary Linux fields are not. |
| `pgrep` | `-c`/`--count`, `-d`/`--delimiter`, `-i`/`--ignore-case`, `-n`/`--newest`, `-o`/`--oldest`, and `-p`/`--pid`. Process creation times are already available through the detail query used by `ps` and `top`. |
| `pstree` | `-s`/`--show-parents`; `-n` can be accepted as an explicit spelling for the PID ordering cash already uses. |
| `top` | `-p PID` selection and `-i` idle-process suppression. Both filter rows already sampled. |
| `tree` | `-I PATTERN`, `-P PATTERN`, `--prune`, and multiple root directories. These only affect the existing walk. |
| `xargs` | `-d`/`--delimiter`, `-L`/`--max-lines`, `-E`/`--eof`, and `-x`/`--exit`. `-d` is particularly common for data that is line-like but cannot use NUL delimiters. |
| `ls` | `-i`, `-U`, `-X`, `-I`/`--ignore`, `--sort`, and `--group-directories-first`. These are formatting or ordering changes over data already collected. |
| `more` | `-s` squeeze blank lines, `+NUMBER`, and `+/PATTERN`. The shared pager already has line movement and search. |

`which`, `hostname`, and `coolfetch` have adequate option surfaces for their stated cash
roles. `chmod` is constrained mainly by Windows permission semantics rather than parsing;
adding GNU flags would not make POSIX mode bits real.

## Larger or platform-sensitive native-command gaps

| Command | Gap and reason |
|---|---|
| `ps` | Full arbitrary `-o` formats, process groups, sessions, TTY selection, and Linux state fields need concepts Windows does not expose machine-wide. Full command lines require permission-sensitive PEB/process queries. |
| `pgrep` | `-f` and `-a` require full command lines. User/group selection requires opening every process token. Session, process-group, terminal, namespace, cgroup, and run-state selectors have no direct Windows equivalent. |
| `pstree` | `-a` needs full command lines; thread trees require a second snapshot and significantly more output. |
| `top` | `-H` thread mode and full command lines add per-process work. Linux load, nice, swap, and buffer/cache fields should remain absent instead of being fabricated. |
| `find` | Common missing predicates/actions include `-regex`, `-perm`, `-user`, `-group`, `-printf`, `-ls`, `-depth`, `-xdev`, `-execdir`, and `-ok`. Cash rejects unknown predicates explicitly, so none are silently ignored. |
| `less` | Cash is a compact pager rather than a full less clone. Common absent flags include `-i`, `-M`/`-m`, `-g`/`-G`, `-p`, and `-x`; reproducing less's complete interactive language is a separate project. |

## Bash builtin option gaps

The detailed language-level audit remains in
[`bash-5.2-gaps.md`](bash-reference/bash-5.2-gaps.md). The option-specific gaps confirmed directly in
the current source are:

The audit also completed `history -n`/`-r`/`-p`, `fc` editor mode, status-change
tracking for `jobs -n`, and symbolic `umask` input. `enable -f`/`-d` now report the
same unavailable dynamic-loading capability as Git for Windows Bash, with status 2.

| Builtin | Missing or incomplete option |
|---|---|
| `read` | `-e`, `-E`, and `-i` provide core terminal editing and preserve redirected-input behavior. `-E` currently uses the same compact editor as `-e`; GNU Readline completion, history navigation, and custom `bind` keymaps remain absent. Source-audited edge semantics for `-n`/`-N`, `TMOUT`, control bytes, array targets, and escaped delimiters are implemented. |
| `jobs` | Bash's `-x command` form is absent. |
| `bind` | `-f` is unimplemented; vi-keymap mutations are silently ignored. |
| `cd` | `-e` in physical mode is unimplemented; `-@` is an extended-attribute feature without a useful Windows mapping. |
| `declare` | `-t` is accepted but function tracing is not tracked. |
| `complete`/`compgen` | `bashdefault`, service completion, and a completion-restart path are incomplete as described above. |

This list is an implementation audit, not a declaration of full Bash 5.2 conformance.
Syntax, expansion, traps, variables, and `shopt` behaviors that are not command options
belong in the separate Bash 5.2 gaps report.
