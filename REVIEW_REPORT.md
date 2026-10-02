# Code review of cash, 2026-10-02

Reviewed: `main` at `c367166a` (v1.3.7). The whole workspace was in scope: the
repository, the build and release, the architecture, the algorithms and the code. This
report replaces the one of 2026-09-22, which is in git history (`git show
c367166a:REVIEW_REPORT.md`); §9 says which of its findings still hold.

The findings to be worked are in `TODO.md`, phase 8, in the order to work them.

---

## 1. How the review was done

**The checklist** came from three sources:

- Google's engineering practices on [what a reviewer looks
  for](https://google.github.io/eng-practices/review/reviewer/looking-for.html):
  design, functionality, complexity, tests, naming, comments, style, consistency,
  documentation and context, in that order.
- The [Rust API Guidelines checklist](https://rust-lang.github.io/api-guidelines/checklist.html),
  for the API and dependability items that make sense in an application workspace.
- Unsafe and FFI review practice: a SAFETY comment that is actually true, handle
  ownership, buffer sizes, GetLastError order, and no panic unwinding across an FFI
  boundary. Sources: [Effective Rust item 34](https://effective-rust.com/ffi.html), the
  [Rust FAQ on soundness](https://www.rustfaq.org/en/what-are-the-rules-for-unsafe-code-soundness/)
  and [Microsoft's Rust training, unsafe and FFI](https://microsoft.github.io/RustTraining/c-cpp-book/ch14-unsafe-rust-and-ffi.html).

**The reviewers.** Nine of them worked in parallel, one per layer: the repository,
build, CI and release; `cash-win32`; the execution half of `cash-core`; the language
half of `cash-core`; parser, interactive and `vendor/`; the builtins; the binary,
`cash-shell` and the tests; awk, sed and bc; and a cross-cutting pass for duplication
and smells. Each was read-only and built nothing. The machine is shared, and CI was
green on 1.3.7: fmt, clippy pedantic+nursery with `unwrap`/`expect`/`panic` denied,
nextest and doc tests. So the review looked for what those tools miss.

**Decisions first.** Before anything was called a defect, it was checked against:

- `spec.md` §3 (D1–D69) and §4 (the 39 deliberate divergences)
- the "Decided" list in `TODO.md`
- `open-issues.md`
- each `vendor/*/CASH-PATCHES.md`
- `research/`

Something decided is not a finding. It is reported only where the code does not do
what the decision says, or the decision has a cost nobody recorded. §8 lists what was
checked and found to be decided.

**Evidence.** Bugs were shown by running the installed cash (Scoop, 1.3.4) against
Git Bash 5.3.15 (GNU sed 4.9, gawk 5.4, GNU bc 1.07.1 under WSL). For every such bug,
the code behind it was checked to be unchanged between 1.3.4 and HEAD. Every finding
carries one mark:

- ✅ reproduced a second time by the lead reviewer in this session;
- 🔁 reproduced by the reviewer who found it (the probe is quoted);
- 📖 from reading the code.

**Severity.**

- **Critical:** a wrong result with no error, in something scripts rely on.
- **High:** a wrong result, a crash or a security hole a user can hit in ordinary use.
- **Medium:** wrong in a narrower case, a resource leak, or a decision not implemented.
- **Low:** an edge case, robustness, or consistency.
- **Info:** worth knowing.

---

## 2. Verdict

cash is a well-made codebase with a clear design record: almost every module cites the
decision it implements, and that made this review possible. The crate graph is sound.
`cash-win32` is a real bottom layer that holds nearly all of the unsafe code, and the
unsafe code there is careful (alignment, bounded retry loops, `cb` fields, pid-reuse
safety). CI, local runs and the release gate all go through one entry point. The test
suite is large (about 3,000 `#[test]`s) and now drives real ConPTY sessions against Git
Bash's screens.

Six things stand out.

1. **The shell's state and the process's state get mixed up.** cash never changes its
   own working directory, and `export` never touches the process environment. That is
   right, but any code that reaches for `std::process::Command`, `std::env::var` or a
   relative `Path` gets the *process's* cwd and environment. It finds the wrong file,
   misses an exported variable, or runs the wrong program. This one cause produces
   about a dozen findings, from `./tool.exe` after `cd` (which runs the copy in the
   start folder) to `xargs` and `find -exec` losing `export`, and PATHEXT changes being
   ignored (§4.1).
2. **Some crashes bypass the panic recovery.** A recursion only 200 deep, or about 20
   deep inside a pipeline, overflows a 2 MiB tokio worker stack and kills the shell. A
   stack overflow is an abort, not a panic, so the documented recovery never runs.
   Five other inputs panic outright (§4.2).
3. **Errors and blocking inside pipelines and background jobs.** An error in a
   pipeline stage takes down the whole pipeline, or the whole shell. Background lists
   of builtins hold tokio worker threads. Coprocesses deadlock (§4.3).
4. **awk prints wrong numbers.** `%g`, OFMT and CONVFMT drop digits: `print 100000.4`
   prints `1`, an average of 150000 prints `15`, and integers stop at 2^63. These bugs
   come from posixutils-rs, and awk, bc and sed are the one part of the workspace that
   opts out of every lint (§4.6).
5. **Quoting at the edges.** These are the places where a string becomes a command:
   - batch arguments get stray carets;
   - `start` hands an unquoted URL to `cmd.exe`, so `&` runs a second command;
   - two spellings of command substitution slip past the §4 #37 subscript hardening;
   - `declare -f` and `export -f` print functions with here-documents that cannot be
     read back (§4.5).
6. **Privacy.** `HISTCONTROL`, `HISTIGNORE`, `HISTSIZE` and `HISTFILESIZE` are not
   read anywhere. A command typed with a leading space, the usual way to keep a secret
   out of history, goes into `~/.cash_history` at once. That is despite the starter
   `.bashrc` (D69) setting `HISTCONTROL=ignoreboth` (§4.7).

Around those sit supply-chain gaps in CI, spec text that has drifted from the code,
brush leftovers that users can see, and duplicated helpers that already disagree with
each other.

| Area | Assessment |
| --- | --- |
| Architecture and layering | Good. One structural weakness: the shell-vs-process state boundary (§4.1). |
| Win32 layer and unsafe code | Good. Sound where checked; edge bugs are in D17 pipes, D19 stop and D32 escaping. |
| Language semantics | Good on the common path (field splitting, arithmetic, IFS all match bash); arrays, extglob and history have real bugs. |
| Execution engine | Mixed. Process creation is right; error containment and async blocking are not. |
| Builtins | Mixed. Strong ones resolve through the shell; the ones that spawn do not. |
| awk / sed / bc | bc is excellent, sed good, awk has Critical number-formatting bugs. |
| Tests | Strong and broad. Gaps: language conformance, release-only paths, no fuzzing, CI retries hide flakes. |
| Build, CI, release | Good gates. Missing supply-chain checks; token scope and action pinning are loose. |
| Docs | The decision record is excellent, but parts of `spec.md` now describe old code. |

---

## 3. Architecture

**The layers.** Dependencies point one way:

- `cash-win32` has no internal dependencies and sits at the bottom.
- `cash-parser` is next.
- `cash-core` depends on parser and win32.
- `cash-builtins` and `cash-interactive` depend on core.
- `cash-shell` depends on all of them.
- `cash` (the binary) sits on top.
- The absorbed tools (`cash-awk`, `cash-bc`, `cash-sed`, the uutils) are leaves, which
  `cash-shell`'s bundled dispatcher re-enters as `cash.exe --invoke-bundled`.

There are no cycles. `windows_sys` is used only inside `cash-win32`. `unsafe` outside
it is limited to:

- awk's VM (59 lines);
- `stat.rs` and `wellknownvars.rs` (private FFI, ARCH-06);
- one `set_var` before the runtime starts.

**Where the brush legacy shows:**

- **The platform seam.** brush's `sys` layer is still there for one platform, behind
  `#![allow(unused)]`. That is the `sys::platform` alias, stubs for `commands`, `fd`,
  `terminal` and `resource`, an empty `PlatformError`, and no-op `arg0`,
  `process_group` and `take_foreground`.
- **Features and metadata named as in brush.** The `experimental-bundled-coreutils`
  feature is on and shipped (D48). There is also `binstall` metadata for tags that
  never exist, `msrv-policy.md` references to a missing file, and dev-dependency sets
  copied from brush-shell.
- **User-visible strings:** `brush:` messages, `brush$ ` as the default prompt,
  `$BRUSH_VERSION`, `BRUSH_PS_ALT`.
- **Broken LICENSE symlinks** in seven crates. They point at `crates/LICENSE`, which
  does not exist.

**The biggest structural risks:**

1. **State ownership.** Nothing stops new code from using process state. A rule plus a
   check would: "a builtin gets cwd, environment, PATH and PATHEXT only from `Shell`,
   and spawns only through one cash-core helper" (§4.1, R1).
2. **Execution depth.** Every nesting level is a set of large boxed async state
   machines on a 2 MiB worker stack (§4.2).
3. **Quality is uneven across crates.** About 34k lines of absorbed tool code sit
   outside the lint regime the rest of the workspace relies on (ARCH-01).
4. **Command resolution has two homes.** It lives in `cash-core` and in
   `cash-win32::resolve`, with three PATHEXT parsers that disagree (ARCH-04).
5. **Spec drift.** `spec.md` is the decision record, so a stale row costs more there
   than elsewhere (ARCH-13, W32-09).

---

## 4. Themes

### 4.1 Shell state and process state

cash keeps its working directory and its exported variables in `Shell`; it never calls
`set_current_dir` and never writes to the process environment. That is the right
design (D10), but every site below uses the process's copy instead:

| ID | Site | What happens | Mark |
| --- | --- | --- | --- |
| EXE-01 | `cash-core/src/commands.rs:420-425`, `:1000` | `cd sub && ./tool.exe` runs `tool.exe` from the folder cash started in. With none there, it is "command not found". `Command::new(relative)` resolves against the process cwd although `candidate` was already joined to the shell's. | ✅ |
| XC-1, BI-01 | `xargs.rs:206-230`, `find.rs:692-731` | `xargs` and `find -exec` lose `export`ed variables and PATH changes. They do not find `npm` (a `.cmd`), cannot run a shebang script, and print `find -exec` output past a redirection. xargs' child also reads cash's stdin. | 🔁 |
| BI-02 | `nohup.rs` | `nohup sh -c …` is refused (`-c`). The child runs in the process cwd, inherits process stdout past `> file`, and `nohup.out` lands in the wrong folder. | 🔁 |
| BI-08 | `win.rs:245-256` | `detach` passes a null cwd and environment: `cd proj; detach code .` opens the wrong folder. | 🔁 |
| BI-06 | `win.rs:120-131` | `start report.pdf` after `cd` resolves against the process cwd (see also §4.5). | 📖 |
| BI-07 | `install.rs` | Every operand is relative to the process cwd. `-m` knows four literal modes. | 🔁 |
| BI-12 | `find.rs:488-493` | `-newer FILE` is relative to the process cwd. | 🔁 |
| ARCH-04, XC-10 | `sys/windows/fs.rs:19-33`, `pathindex.rs:232`, `msys.rs:89` | PATHEXT for lookup and `test -x` comes from the process environment. `export PATHEXT=.EXE` changes children but not cash. There are three parsers: lowercase and untrimmed, uppercase and trimmed, and one that ignores PATHEXT. The doc comment says the opposite. | 🔁 |
| XC-11 | `commands.rs:257`, `:334` | `COMSPEC` is read from the process environment. | 📖 |
| EXE-05 | `shell/fs.rs:298-320` | Assigning `PATH` does not clear the command hash: `PATH=a:$PATH; foo; PATH=b:$PATH; foo` runs `a/foo` twice. Bash runs `b/foo` the second time. | ✅ |
| XC-7 | `umask.rs:170` | The mask is a process-wide static, so `( umask 077 )` changes the parent's umask. | ✅ |

**Fix (R1).**

- Absolutize the command path once, in `SimpleCommand::execute`.
- Give builtins one helper, say `ExecutionContext::external_command(program, args)`,
  built on the existing `compose_std_command` and `exported_environment`, with the
  context's fds and `jobreg::contain`.
- Give `Shell` a `pathext()`.
- Add an `xtask check` that fails on `std::process::Command::new`, `std::env::var` and
  `File::open(<relative>)` in `cash-builtins` and `cash-core`, outside an allow-list.

### 4.2 Crashes that the recovery does not catch

README "Shell robustness" promises that a panic in an interactive command is reported
and the prompt comes back. These inputs end the shell instead:

| ID | Input | Result | Mark |
| --- | --- | --- | --- |
| BIN-01 | `f(){ (( $1 > 0 )) && f $(( $1 - 1 )); }; f 200`; depth 45 inside `$()`; depth 20 in a pipeline stage | `thread 'main' / 'tokio-rt-worker' has overflowed its stack`. Bash runs depth 500. | ✅ |
| PI-06, XC-2 | `echo {1..3..99999999999999999999}` | "cash had a problem and crashed" (`word.rs:961`, `unwrap` inside `peg!`, which clippy cannot see; the crate has `#![allow(clippy::unwrap_used)]`). | ✅ |
| LANG-08 | `declare -c c; c=éa` | Panics in `variables.rs:524` (`replace_range(0..1)` on a 2-byte character). | ✅ |
| LANG-07 | `PS1='\D{%Q} '` | Panics on the first prompt (`prompt.rs:236`). `\!` and `\#` give "not yet implemented". | 🔁 |
| PI-22 | about 10,000 nested `$(` pasted at the prompt | Stack overflow. Bash fails at 3,000 too, but cash loses the session. | 🔁 |
| BIN-02 | a settings.json or fragment containing a lone `/` | `terminal.rs:655-680` loops forever, allocating. Reached by `ls --icons`, which the starter rc aliases, and by `--terminal-profile`. | 📖 (read and confirmed) |

**Why BIN-01 happens.**

- The runtime is built without `thread_stack_size` (`cash-shell/src/entry.rs:189`).
  `/STACK:8388608` in `build.rs` covers only the main thread.
- The 500-deep function guard (`callstack.rs:126`) never fires before the stack runs
  out.
- Each level is several boxed `async_trait` futures plus very large inline state
  machines: `expand_parameter_expr` is one async fn of about 600 lines,
  `execute_in_pipeline` about 240 (EXE-18, LANG-26).

**Fix (R2).**

- Give the runtime big worker stacks (`Builder::thread_stack_size`, tens of MiB) and
  run `block_on` on a big-stack thread.
- Set the depth guard below the measured capacity.
- Split the largest async fns into boxed helpers.
- Remove the parser's crate-wide `unwrap` allow.
- Add a cargo-fuzz target over `tokenize_str`, `parse_program`, `word::parse` and the
  JSONC tokenizer. The parser already derives `Arbitrary`; nothing uses it (PI-14).

### 4.3 Pipelines, background jobs and async

| ID | Severity | Finding | Mark |
| --- | --- | --- | --- |
| EXE-02 | High | **An error inside a pipeline stage escapes the stage.** `read -u 99 x \| cat; echo after` prints only the error, with no `after` and an empty PIPESTATUS. Bash prints `after 0 1 0`. `true \| echo ${u:?boom}; echo after` exits the whole shell. The waiter's `?` (`results.rs:292`, `interp.rs:773`, `:1743`, `commands.rs:836`) aborts the pipeline. | ✅ |
| EXE-03 | High | **Background lists run on tokio workers.** `interp.rs:444`, and command substitutions at `commands.rs:1432`, use `tokio::spawn`. Builtins block that worker (`read` on a pipe, `while read`). With as many such jobs as cores, a foreground `$(…)` waits for them; with `tail -f` it hangs for ever. Pipeline stages already use `spawn_blocking` (`interp.rs:960`). | 🔁 |
| EXE-04 | High | **Coprocesses.** The coproc holds the write end of its own stdin (the fds are added before `shell.clone()`), so `coproc cat` never sees EOF and `wait` hangs. `COPROC_PID` is the job number. `kill %1` fails. The fds are 3 and 4, not 63 and 60, so `exec 3>log` clobbers them. | 🔁 |
| EXE-08 | Medium | **A background job of builtins only.** `{ while ((1)); do x=1; done; } &` hangs the `&`: `pid_ready` never fires. `while :; do :; done & kill %1` cannot be killed. | 🔁 |
| EXE-06 | Medium | **Process substitution drops its own runtime when the list returns.** `cat <( { sleep 1; echo late; } & echo early )` loses `late` (`interp.rs:2758`). | 🔁 |
| EXE-09 | Medium | **An undocumented 128-slot cap** (`CASH_MAX_SUBSHELLS`) is shared by `&` jobs and by compound/function pipeline stages. While 128 jobs run, `{ …; } \| x` fails with status 1. | 🔁 |
| EXE-07 | Medium | **A script file is parsed whole before it runs.** A syntax error on line 2 stops line 1, and a self-extracting payload after `exit` must parse (`shell/execution.rs:199`). | 🔁 |
| EXE-15 | Low | **A panicked background task** is polled again after completion (`jobs.rs:266`). | 📖 |
| EXE-14, XC-9, W32-09 | Low | **Resume after `CREATE_SUSPENDED` is unchecked.** It uses `let _ = resume_process(…)` (undocumented `NtResumeProcess`, NTSTATUS mapped as a Win32 error). If it fails, the child stays suspended and the shell waits for ever. | 📖 |

**Fix (R3):**

- Handle failure per stage: display the error and turn it into the stage's status.
- Run background lists as `spawn_blocking` plus `Handle::block_on`.
- Clone the coproc's shell before adding the fds.
- Give internal jobs a cancel token checked at command boundaries.
- Run process substitutions on the shell's runtime.
- Check the resume result.

### 4.4 Process substitution pipes (D17)

| ID | Severity | Finding | Mark |
| --- | --- | --- | --- |
| W32-02 | High | **`<(…)` sometimes delivers nothing to a consumer that opens it twice.** `cmd /c type <(echo x)` failed 11 times in 80 with "The pipe has been ended". Likely cause: a race between the replay check (`pipe.rs:134`) and `reader_eof` (`:155`). | 🔁 |
| W32-03 | Medium | **Each unopened `<(…)` leaks two threads blocked in `ConnectNamedPipe`.** 30 substitutions took the shell from 26 to 86 threads. A read substitution has no `SubstitutionEnd`. | 🔁 |
| W32-04, EXE-10 | Medium | **The replay buffer keeps every byte for the life of the pump.** `<(tail -f log)` grows without bound. | 📖 |
| W32-07 | Medium | **The pipes have default security and accept remote clients.** They are created with a null `SECURITY_ATTRIBUTES` and without `PIPE_REJECT_REMOTE_CLIENTS`, at a predictable name. | 📖 |

**Fix (R5):**

- One pump thread owns the reader; instances only replay.
- Trim the replay buffer once both clients connect.
- Release unclaimed instances when the command ends.
- Use `PIPE_REJECT_REMOTE_CLIENTS`, a current-user DACL and a random name part.

### 4.5 Quoting where a string becomes a command

| ID | Severity | Finding | Mark |
| --- | --- | --- | --- |
| BI-06 | High | **Command injection through `start`.** `start` runs `cmd.exe /d /s /c start "" <target>` through std, which quotes only arguments with spaces. A URL with `&` (`https://x/?a=1&b=2`) ends `start` and runs the rest as a command; `%VAR%` expands. Fix: call `ShellExecuteExW` with the absolute path. | 📖 |
| W32-01 | High | **Batch arguments get stray carets.** `escape_for_cmd` (`cmd.rs:81-92`) quotes, then caret-escapes metacharacters inside those quotes. A `.bat`/`.cmd` receives `Q^&A notes.txt` and `a ^\| b`. Probed: `%` and `(x86)` came through clean. `az.cmd --query "… \| …"` is the realistic case. Fix: track cmd's quote state and add a round-trip test through a real `.bat`. | ✅ |
| LANG-04 | High | **The §4 #37 injection hardening is incomplete.** `runs_a_command` (`expansion.rs:781`) checks text, not syntax. `unset "a[$k]"` with `k='$((touch x) )'` or `'${ touch x; }'` creates `x`. The plain `$(…)` form is refused, as promised. Fix: decide on the parsed word. | ✅ |
| PI-01 | High | **`declare -f` and `export -f` print a here-doc that cannot be parsed.** The printer indents the body and the end tag (`ast.rs:1762`), so `eval "$(declare -f g)"` fails and `export -f g; bash -c g` is "not found". | ✅ |
| PI-02 | High | **A `)` inside a here-doc ends `$(…)` early.** `word.rs:1453` has its own `$(` scanner that knows nothing of here-docs, while the tokenizer's is correct. `a=$(cat <<'Z' … ) … Z )` gives the wrong text. | ✅ |
| BI-19 | Low | **`elevate` builds PowerShell `-ArgumentList` by hand.** An argument with a space is split. | 📖 |

### 4.6 awk, sed and bc

bc matched GNU bc on every non-decided probe and is faster on big base conversion.
sed matched GNU on about 50 probes. awk has the worst defects in the review, almost all
inherited from posixutils-rs 0.9.0 (96bd8a3):

| ID | Severity | Finding | Mark |
| --- | --- | --- | --- |
| TXT-01 | **Critical** | **`%g`, OFMT and CONVFMT drop digits.** `print 100000.4` → `1`; the mean of 100000 and 200001 → `15`; `printf "%g",100000` → `1`; `print 0.00001234` → `0.000012`; `a[100000.4]` is key `1`. Four defects in `format.rs`: zeros stripped with no decimal point, `trunc` for `floor`, negative exponents, and padding. | ✅ |
| TXT-02 | High | **Integral values print through `i64`.** `print 2^64` → `9223372036854775807`; `printf "%x",-1` is fatal. | ✅ |
| TXT-03 | High | **A `for (k in a)` left by `break` or `return` locks `a` for the rest of the run.** The standard dedup function dies ("active iterator"). `for(k in a) delete a` panics. | ✅ |
| TXT-05 | High | **Unknown string escapes (`"\."`, `"\&"`) are parse errors.** gawk, mawk and BWK accept them. | 🔁 |
| TXT-06 | High | **Plain `getline` reads only the current file**, and nothing in BEGIN. | 🔁 |
| TXT-04 | High | **sed `/x/,+1` and `/x/,3` do not re-check `addr1` after a range ends.** `/x/,+1p` misses the second range. | ✅ |
| TXT-07 | Medium | **sed `2,3c T` never prints `T`.** `2,4!c` misses lines. | 🔁 |
| TXT-08, TXT-14 | Medium | **awk writes with `print!`.** `awk '{print}' \| head -1` panics. Every record is one `WriteFile` (28× slower than gawk). There is no flush before `system()`. | ✅ |
| TXT-09 | Medium | **sed `-z` is accepted and ignored.** | 🔁 |
| TXT-10 | Medium | **sed's literal fast path turns `\$` and `\^` into anchors.** | 🔁 |
| TXT-11, TXT-12 | Medium | **NUL bytes and regex-RS boundaries make awk fail outright.** A NUL in a record is fatal ("invalid string"). A UTF-8 character across the 8 KiB boundary with a regex RS is fatal. | 🔁 |
| TXT-13 | Medium | **Stale array-element references.** `a["x"] = split(…, a)` is wrong; deleting in the right-hand side panics. | 🔁 |
| TXT-15 | Medium | **sed `-i` deletes the original before `persist`.** It is not atomic, a failed persist loses both files, and the read-only attribute is lost. | 📖 |
| TXT-16 | Medium | **awk reads one byte per syscall and has no dynamic-regex cache.** It is 10–28× slower than gawk. | 🔁 |
| TXT-17 | Medium | **sed without `LANG` is in byte mode while awk is UTF-8.** `LANG=en_US` is a hard error. | 🔁 |
| TXT-18 | Medium | **An untyped variable passed to a function and used there as an array stays empty.** | 🔁 |
| TXT-19–25 | Low | `print $1==$2` is a parse error; bc ignores `BC_LINE_LENGTH`; sed backrefs need UTF-8; leftmost-longest is approximated in both crates; sed `\U` is silently literal; awk's error reporter can panic; awk and sed duplicate the shell-command spawner and the regex helpers. | 🔁 |

**The awk VM's unsafe code is sound as far as it was examined.** The stack is fixed,
fields are boxed and never freed before exit, and array references are indices, so a
staleness bug panics instead of dangling. One off-by-one is latent: `stack.rs:230` has
`>=` where it should have `>`.

**Fix (R6):**

- Fix TXT-01/02/03/04 with differential cases added to `tests/awk-differential.sh` and
  the sed corpus.
- Make those differentials frozen goldens that run in `it` (BIN-05).
- Put the three crates under `[lints] workspace = true`, with a local allow-list for
  style lints and the unsafe lints kept on (ARCH-01).
- Report the inherited bugs upstream.

### 4.7 History

| ID | Severity | Finding | Mark |
| --- | --- | --- | --- |
| PI-03, LANG-05 | High | **The history variables are ignored.** `HISTCONTROL`, `HISTIGNORE`, `HISTSIZE` and `HISTFILESIZE` are read nowhere, and `set +o history` does not stop recording. ` export TOKEN=…` (leading space) is in `~/.cash_history` the moment Enter is pressed (D44). The file grows without bound, and each flush walks all of it. | ✅ |
| LANG-06 | Medium | **History expansion fires inside `${!arr[@]}`, `$!` and `[!a]`.** These give "event not found" at the prompt. | 🔁 |
| PI-11 | Low | **A history entry is written in two or three `WriteFile` calls**, so two tabs can interleave. | 📖 |
| LANG-22 | Low | **Multi-line entries come back split.** Also, `:q` does not escape `'`, and searches collect the whole history. | 🔁 |

---

## 5. Other findings, by area

These are not covered by §4. Every reviewer's full list was read; what is left out here
is a duplicate of a row above.

### 5.1 Language (`cash-core`, `cash-parser`)

| ID | Severity | Finding | Mark |
| --- | --- | --- | --- |
| LANG-01 | High | **extglob `!(a\|ab)` matches `ab`.** `echo !(*.tar\|*.tar.gz)` lists `x.tar.gz`, so `rm !(*.tar\|*.tar.gz)` deletes what was excluded (`pattern.rs:152`, an atomic group that commits too early). | ✅ |
| LANG-02 | High | **Subscripts in `c=([2+1]=y z [i]=w)` are not arithmetic.** Everything lands on 0 and 1. `a+=([-1]=X)` writes `[0]`. | ✅ |
| LANG-03 | High | **Negative indices count from the element count, not the highest index + 1.** After `unset 'a[1]'`, `${a[-1]}` reads, writes and unsets the wrong element. | ✅ |
| LANG-09 | Medium | **`${#x}` counts bytes and `${x: -1}` is empty for `café`.** Bash in UTF-8 gives 4 and `é`. | ✅ |
| LANG-10 | Medium | **`$(( $empty ))` is a parse error that aborts the script.** Bash gives 0. | 🔁 |
| LANG-11 | Medium | **`declare -i` through `read`, `printf -v`, array literals and `for` treats names as 0.** `x=1/0` is silently 0. Plain `x=expr` was fixed. | 🔁 |
| LANG-12 | Medium | **`declare -n r='a[1]'; r=Z` creates a variable named `a[1]`.** | 🔁 |
| LANG-13 | Medium | **Inside backquotes, `\$` and `\"` are not unescaped.** | 🔁 |
| LANG-14 | Medium | **`${x/b/"$r"}` with `r='&&'` substitutes the match.** Quoting should protect it (patsub_replacement). | 🔁 |
| LANG-15 | Medium | **The D31 case-insensitive fallback scans every variable on each miss.** 20k new variables take 2.2 s against bash's 0.23 s, and with `Foo` and `FOO` both set the result depends on HashMap order. | 🔁 |
| PI-04 | Medium | **Backslash-newline is kept in an unquoted here-doc.** | 🔁 |
| PI-05 | Medium | **`$(( $((1)) << 2 ))` is read as a here-doc.** The arithmetic flag is a bool, not a depth. | 🔁 |
| PI-07 | Medium | **Nested `case` without a final `;;` parses in exponential time.** Depth 21 takes 10.5 s (`peg.rs:327`). | 🔁 |
| LANG-16 | Low | **Completion turns typed glob characters into a glob.** `[draft] ` completes nothing. | 🔁 |
| LANG-17 | Low | **`[[ ab =~ a\|ab ]]` gives `a`.** POSIX leftmost-longest gives `ab`. | 🔁 |
| LANG-18 | Low | **`${x:=1+2}` with `-i` returns `1+2`, not 3.** | 🔁 |
| LANG-19 | Low | **`${x@u}` capitalises every word.** Bash capitalises only the first character. | 🔁 |
| LANG-20 | Low | **`${a[@]@K}` gives values only, not key/value pairs.** | 🔁 |
| LANG-21 | Low | **An assignment through a circular nameref silently succeeds.** | 🔁 |
| LANG-23 | Low | **`RANDOM=42` does not seed.** | 🔁 |
| LANG-24 | Low | **`printf %s x=~` does not expand the tilde.** | 🔁 |
| PI-12 | Low | **`$$'…'` and `\$'…'` open ANSI-C quoting.** | 🔁 |
| PI-13 | Low | **The winnow parser is an `unimplemented!()` stub behind a feature.** | 📖 |
| PI-20, LANG-25 | Info | **Duplicated tables and conversions.** Char vs byte offsets are rebuilt in three consumers; two metacharacter tables disagree. | 📖 |

### 5.2 Execution, builtins and binary

| ID | Severity | Finding | Mark |
| --- | --- | --- | --- |
| BI-03 | High | **`kill $a $b` refuses the second operand.** `kill $(jobs -p)` kills nothing. | ✅ |
| BI-04 | High | **`find d -delete` runs pre-order and cannot delete directories.** GNU implies `-depth`. | ✅ |
| BI-05 | High | **`chmod go-w f` makes `f` read-only for its owner.** The who-part is discarded, and `u+rw,go-w` is "invalid mode". D23/D34 decide "read-only attribute only", not this. | ✅ |
| XC-3, ARCH-06 | Medium | **`[ a -ef b ]` is "operation not supported".** `stat` on a directory gives inode 0 and 1 link, because a private FFI copy opens without `FILE_FLAG_BACKUP_SEMANTICS`. `cash_win32::fs::same_file` already exists. | ✅ |
| XC-4 | Medium | **`mapfile -t` keeps the `\r`.** D20 names mapfile. `text::split_lines` is documented for mapfile but used by nobody; pager and xargs carry copies. | ✅ |
| XC-5, EXE-11 | Medium | **`time` and `times` always show 0 user and sys.** `cash_win32::process::cpu_time` already exists. | 🔁 |
| XC-6 | Medium | **Two D31 name tables (24 vs 8 names); the short one is used.** `declare -p` shows `Lang=`, `ComSpec=`. | 🔁 |
| XC-8, BIN-04 | Medium | **Error output is ANSI-coloured into pipes and files, and `NO_COLOR` is ignored.** There are three prefix styles (`error:`, `cash:`, `name:`) and three private `paint` helpers. | ✅ |
| BI-09 | Medium | **`printf '%d' abc` exits 0 with a `cash.exe:` prefix.** `%(…)T` is an error with a raw Fluent key. | 🔁 |
| BI-10 | Medium | **`\c` in `%b` does not stop format reuse.** | 🔁 |
| BI-11 | Medium | **`find -exec` problems.** `{}` inside a word is not substituted. `-exec … +` has no 32 KiB batching, and spawn errors are silent. | 🔁 |
| BI-13 | Medium | **No loop detection on links.** `find -L` has no visited set, and `chmod -R` follows junctions. | 📖 |
| W32-05 | Medium | **Two `kill -STOP` need two `kill -CONT`.** Each STOP adds to the suspend count. | 🔁 |
| W32-06 | Medium | **`kill -9 <pid>` kills the pid's whole job tree.** D22 says a bare pid is one process. | 🔁 |
| W32-08 | Medium | **`#!/usr/bin/env -S bash -e` takes `-S` as the command.** | 🔁 |
| BIN-03 | Medium | **`--remove-terminal-profile` cuts a whole `newTabMenu` folder that contains the cash entry.** | 📖 |
| EXE-13, W32-10 | Medium | **D36's pooled prompt job does not exist.** Every Starship spawn sweeps the registry and creates a job. D6's exception table still lists it. | 📖 |
| PI-08 | Medium | **`READLINE_LINE` replacement clears only the first line of a multi-line buffer** (Ctrl-X Ctrl-E, `bind -x`). | 📖 |
| PI-10 | Medium | **Highlighting checks a slash-containing command synchronously on every key.** An offline UNC path stalls typing. There is a tension inside D59 here. | 📖 |
| EXE-12 | Low | **`exec -a name` does nothing.** `arg0` is a no-op stub, and the process-group and terminal plumbing is dead. | 🔁 |
| BI-14 | Low | **`ls \| head -1` reports "pipe is being closed", exit 2.** `ls -R` hides errors in subdirectories. | 🔁 |
| BI-15 | Low | **Smaller `kill` gaps.** `kill -s 0`, `kill -l 137` and the message for `kill abc` differ from bash. `kill -l` lacks a final newline. | 🔁 |
| BI-16 | Low | **`--help` and usage errors are not uniform.** Stdout vs stderr, exit 0 vs 2, and bare clap errors. | 🔁 |
| BI-17 | Low | **`chmod +x` warns "not represented", against D23.** D23 says it is a silent no-op. Every install script prints it. | 🔁 |
| BI-18 | Low | **xargs budget counts unquoted bytes.** It also reads all of stdin before starting, and accepts an unmatched quote. | 📖 |
| BIN-06 | Low | **`cash doctor` always says "inside another job object".** It asks after cash made its own. | 🔁 |
| BIN-07 | Low | **A path-rendering bundled tool runs twice if reading its output fails.** A second `mktemp` file is created. | 📖 |
| BIN-09 | Low | **`CASH_LINKED_TOOL_EXE` is inherited by every descendant.** | 📖 |
| BIN-10, BIN-11, BIN-12 | Low | **Settings and links can be left half-done on error.** A symlinked settings.json becomes a file; a failed folder delete leaves the menu entry; links made before an error are not recorded. | 📖 |
| BIN-13 | Low | **`--enable-highlighting` cannot override `false` in config.toml**, and its test asserts the default. | 📖 |
| BIN-14 | Low | **A recovered panic still prints "cash had a problem and crashed"** and writes a crash report to `%TEMP%`. | 📖 |
| W32-11, W32-12 | Low | **Unused code in `cash-win32`.** `spawn::spawn`, `build_cmd_command_line`, `path::to_extended` and `lexically_normalize` (which keeps `C:/..`) are not used in production. `conpty` and `vtscreen` are test-only but public. | 📖 |
| W32-13, W32-14, XC-13 | Low | **Handle and wide-string helpers.** There are five RAII handle types and 15+ manual `CloseHandle`s; three leak on error in `pipe.rs`. There are about 12 copies of the UTF-16 encoder, some via `to_string_lossy`. | 📖 |
| W32-15 | Low | **Win32 error codes get misreported.** `GetLastError` is read after other calls, and HRESULT and NTSTATUS go through `from_raw_os_error`. | 📖 |
| W32-16 | Low | **Avoidable per-call scans.** Suspend takes one system-wide thread snapshot per tree member, and `real_case` reads a whole directory. | 📖 |
| W32-18 | Low | **Test hooks are live in production.** `CASH_EXE` and `CARGO_BIN_EXE_cash` redirect every `#!/bin/sh` script. | 📖 |
| W32-19, W32-20 | Low | **Two small loop and decoding bugs.** A registry enumeration spins on persistent errors, and a lone surrogate drops the next unit. | 📖 |
| XC-12, XC-14, XC-16 | Low | **Parallel copies of shared mechanisms.** There are three process-creation paths and two environment-block builders. Paths are shown via `replace('\\','/')` past the D10 chokepoint. Case folding is a mix of ASCII and Unicode. | 📖 |
| XC-18 | Low | **`TerminalInfo` has 48 bools; two are read.** D39 promises OSC 133 and 9;9, but only 633 is emitted. | 📖 |
| XC-19 | Low | **Runtimes and `read -e` tab.** Each process substitution builds a tokio runtime. `block_in_place` in `read -e` tab would panic on a current-thread runtime. | 📖 |

### 5.3 Repository, build, CI and release

| ID | Severity | Finding |
| --- | --- | --- |
| ARCH-01 | Medium | **awk, bc and sed opt out of every workspace lint, rustc warnings included.** That covers about 34k lines, 678 `unwrap`/`expect`/`panic` and 59 unsafe lines without SAFETY comments. No decision records it; vendor/'s exemption is recorded. |
| ARCH-03 | Medium | **CI and release supply-chain gaps.** No `cargo deny`/`audit` and no Dependabot; `rust-toolchain.toml` cites a `dependabot.yml` that does not exist. `contents: write` covers the whole release workflow, including the test job. Third-party actions are pinned by tag. rust-cache runs in the release job. `cargo build --profile dist` lacks `--locked`. `ci.yml` has no `permissions`. |
| ARCH-12 | Low | **release.yml: dispatch, pre-release and injection.** A `workflow_dispatch` with a `tag` builds the dispatched ref, not the tag (`checkout` has no `ref`). `"${{ inputs.tag }}"` is interpolated into PowerShell. A `-rc` tag pushed publishes as latest. No provenance attestation. |
| ARCH-05 | Low | **The MSRV is incoherent.** The workspace says 1.88; cash-shell and cash-interactive say 1.95, citing a missing `docs/reference/msrv-policy.md`. No MSRV job runs. |
| ARCH-02 | Low | **The wrong repository URL in user-facing text.** `--help`, the panic report and "not yet implemented" point at `github.com/thraa/cash`; the repository is `tomcoolpxl/cash`. `CARGO_PKG_HOMEPAGE` is empty. |
| ARCH-08 | Low | **Unused dependencies.** cash-sed's runtime deps `predicates`, `textwrap` and `phf` are unused; about 20 dev-deps in cash-shell and about 15 in `cash` are copied from brush. |
| ARCH-09, ARCH-10, BIN-15, XC-21 | Low | **brush names users can see.** `brush:` messages; `help cat` says "executes via `brush --invoke-bundled`"; the default prompt is `brush$ `; `$BRUSH_VERSION`; `BRUSH_PS_ALT`; `experimental-bundled-coreutils` is a shipped default; dead `binstall` and `about.toml` metadata. |
| ARCH-11 | Low | **Lint allows hide too much.** Crate-wide `allow(unused)` (sys), `allow(dead_code)` (cash-shell) and `allow(unwrap_used)` (parser). `allow` outnumbers `expect`, so stale allows never surface. |
| ARCH-15 | Low | **Licence files.** NOTICE omits posixutils-rs and uutils sed. Seven crates' `LICENSE` is a symlink to a missing `crates/LICENSE`. |
| ARCH-17, ARCH-16 | Info | **Dependency versions.** External versions are repeated per crate and drift (regex, serde_json, fancy-regex). Actionable duplicates: `check_elevation` pulls `windows` 0.51 (replace it with a TokenElevation call in cash-win32, ARCH-07); `human-panic` pulls `sysinfo` and `windows` 0.62; cash-awk uses rand 0.8. |
| ARCH-07 | Low | **The user identity is found by running `whoami.exe /user /fo csv` and scraping its output.** Done twice, at every start, while cash-win32 already reads the token SID. |
| ARCH-13, EXE-17, W32-09, BI-22 | Low | **The spec describes old code.** D1 says Bash 5.2.37 (it is 5.3.15). §1 and §4 row 19 say `stat` is not carried (it is). §5 still draws brush crates. D6 and §6 say the spawn race is open (it was closed by `CREATE_SUSPENDED`). The D9 table names cash crates as their own upstream. |
| ARCH-14 | Low | **README and RELEASING are out of date.** The README Layout omits five crates and three folders. The RELEASING crate list omits awk, bc and sed. The toolchain version is hard-coded in five places. |
| BIN-21, ARCH-19 | Info | **CI hides flaky tests.** CI runs nextest with `retries = 3`, while the local rule is `--retries 0`, so flakes pass silently there. A `ci` profile that fails or reports on FLAKY would show them. |

### 5.4 Tests

| ID | Severity | Finding |
| --- | --- | --- |
| BIN-05 | Medium | **The language-conformance basis D43 rests on is nearly empty.** `cases/brush` holds 46 cases and none of them test the language. The 209-case `tests/corpus` and the awk/sed/git-prompt differential scripts run only by hand (`results.json` is from 2026-09-27). Only 3 of 8 GNU `.tests` files are used. |
| PI-09 | Medium | **The vendored reedline and crossterm tests, including those for cash's own patches, never run in CI.** |
| BIN-18 | Low | **Test helpers are duplicated across `it/`.** 60 of 62 modules declare their own `CASH`; 43 their own run helper; 36 their own output struct. An `it/common.rs` would hold them. |
| BIN-19 | Low | **About 19 tests use fixed-name `%TEMP%` folders and `remove_dir_all` them first.** Two runs delete each other's. That is shared-state flakiness, not timing. |
| BIN-20 | Low | **Some tests test the wrong thing or depend on the machine.** One test skips unless an external awk exists, though cash runs its own. One needs the author's `kali-linux` WSL. 14 tests pass silently when skipped. `it` tests inherit the developer's config.toml and `BASH_ENV`. |
| BIN-17, PI-14 | Low | **No fuzzing or property testing.** The parser's `Arbitrary` derives are unused, and the absorbed harness carries unreachable oracle machinery. |

**Counts of `#[test]`:**

| Crate | Tests | Notes |
| --- | --- | --- |
| cash | 1,078 | 1,002 in `it`, 29 ConPTY |
| cash-sed | 510 | |
| cash-awk | 422 | |
| cash-parser | 239 | |
| cash-bc | 222 | |
| cash-win32 | 221 | |
| cash-core | 121 | |
| cash-builtins | 89 | |
| cash-interactive | 67 | |
| cash-shell | 30 | |
| cash-coreutils-builtins | 3 | |

Not in the table: the 51-screen `pty_oracle` comparison against Git Bash, and the 46
YAML cases. Unit tests are strong in the tool crates. The binary is covered through
`it`. The thinnest coverage is in `cash-core` (121 tests for 32k lines) and in release-only behaviour:
CI tests debug builds, and the human_panic path exists only in release.

---

## 6. Metrics

| Crate | Lines | unsafe | `#[allow` | `#[expect` | `let _ =` | TODO | `.unwrap()` in src |
| --- | --- | --- | --- | --- | --- | --- | --- |
| cash | 27,358 | 1 | 66 | 0 | 131 | 13 | 12 |
| cash-awk | 14,812 | 59 | 4 | 0 | 6 | 0 | 92 |
| cash-bc | 6,625 | 0 | 0 | 0 | 4 | 0 | 106 |
| cash-builtins | 28,049 | 3 | 32 | 41 | 40 | 17 | 42 |
| cash-core | 32,579 | 3 | 43 | 42 | 64 | 60 | 52 |
| cash-coreutils-builtins | 509 | 0 | 4 | 0 | 1 | 0 | 10 |
| cash-interactive | 6,951 | 0 | 17 | 7 | 26 | 15 | 46 |
| cash-parser | 11,566 | 0 | 22 | 9 | 1 | 26 | 17 |
| cash-sed | 12,560 | 1 | 3 | 0 | 2 | 2 | 317 |
| cash-shell | 2,946 | 0 | 13 | 1 | 13 | 4 | 10 |
| cash-test-harness | 2,519 | 0 | 3 | 4 | 0 | 0 | 7 |
| cash-win32 | 16,106 | 451 | 23 | 2 | 47 | 0 | 92 |

`unwrap` counts in clippy-linted crates are mostly in tests or justified. Those in awk,
bc and sed are not linted at all (ARCH-01).

**The longest functions:**

| Lines | Function | Location |
| --- | --- | --- |
| 588 | `expand_parameter_expr` | `cash-core/src/expansion.rs:1498` |
| 517 | `init_well_known_vars` | `cash-core/src/wellknownvars.rs:59` |
| 502 | `next_token_until` | `cash-parser/src/tokenizer.rs:768` (nesting 9) |
| 462 | `expand_history` | `cash-core/src/history/expansion.rs:326` (nesting 10) |

21 functions are over 200 lines. The biggest files:

| Lines | File |
| --- | --- |
| 4,571 | awk `compiler.rs` |
| 3,657 | sed `compiler.rs` |
| 2,906 | `interp.rs` |
| 2,834 | `expansion.rs` |

Startup: `cash -c true` takes about 147 ms through the Scoop shim, and every `cash -c`
builds a multi-thread runtime with one worker per core.

---

## 7. Recommendations, in order

The order is: what a user can be hurt by, then what makes the next bug cheaper. Each
one is a group of items in `TODO.md` phase 8.

1. **R1. One owner for cwd, environment, PATH and PATHEXT** (§4.1). Absolutize the
   command path. Add one spawn helper for builtins, `Shell::pathext()`, a PATH setter
   that clears the hash, and `umask` in `Shell` state. Then add an `xtask check` that
   forbids `std::process::Command::new` and `std::env::var` in core and builtins
   outside an allow-list. This one guard prevents the most common bug class found.
2. **R2. Crashes** (§4.2). Big worker stacks and a depth guard that fires first. Fix
   the five panics and the JSONC loop. Remove the parser's crate-wide `unwrap` allow.
   Add cargo-fuzz targets for the tokenizer, parser, word parser and JSONC.
3. **R3. awk numbers and the text-tool bugs** (§4.6). TXT-01 to TXT-06 first, each
   with a differential case. Then bring awk, bc and sed under the workspace lints.
4. **R4. Security and privacy at the edges** (§4.5, §4.7). `start` via
   `ShellExecuteExW`. A quote-aware `escape_for_cmd` with a real `.bat` round-trip
   test. The #37 check on the parsed word. `HISTCONTROL`, `HISTIGNORE`, `HISTSIZE`
   and `HISTFILESIZE`. Pipe DACL and remote-client rejection.
5. **R5. Pipeline and job semantics** (§4.3). Contain each stage's errors. Background
   lists on `spawn_blocking`. Coproc. Cancellable internal jobs. Check the resume
   result.
6. **R6. D17 pipes** (§4.4). One pump, a bounded replay buffer, release of unclaimed
   instances.
7. **R7. Language correctness** (§5.1). Arrays (LANG-02, LANG-03, LANG-12), extglob
   (LANG-01; a backtracking matcher rather than more regex), here-docs (PI-01, PI-02,
   PI-04, PI-05), UTF-8 lengths, `$(( ))`, `declare -i`, backquote escapes,
   patsub_replacement.
8. **R8. Builtins** (§5.2). `kill` with several operands, `find -delete`, `chmod`
   who-sets, `-ef` and `stat` via one `cash_win32::fs::file_info`, `mapfile` CR,
   `times`, colour only on a terminal with `NO_COLOR`, uniform `--help` and usage
   statuses.
9. **R9. CI and release** (§5.3). `permissions: contents: read` by default; SHA-pinned
   actions; `cargo deny check advisories`; `dependabot.yml`; `--locked`; checkout of
   the tag on dispatch; tag input through `env:`; pre-release from the tag; a nextest
   `ci` profile that shows flakes; the vendored crates' tests in `xtask ci full`.
10. **R10. Tests** (§5.4). Freeze the corpus and differential outputs as goldens in
    `it`, add `it/common.rs`, unique temp folders, and isolate the user's config in
    every `it` test.
11. **R11. Records and leftovers.** Bring `spec.md` (D1, §1, §4 row 19, §5, D6, D9,
    D23, D36) and README/RELEASING up to date. Remove brush strings, dead code
    (`spawn::spawn`, winnow, `sys` stubs, the harness's oracle mode), the stale
    `TODO(bundled)` comments and the unused dependencies. Fix the LICENSE links and
    NOTICE. Replace `whoami.exe` scraping and `check_elevation` with token calls in
    cash-win32.

**Three decisions are yours and are marked as such in `TODO.md`:**

- D36: implement the pooled prompt job, or record that it was dropped.
- D23: `chmod +x` silent as written, or amend D23.
- Whether `time` and `times` should report CPU time (spec §6.1 called `resource` "not
  needed" in the M0 survey; that was not a decision about `time`).

---

## 8. Checked and found to be decided

Reviewers found these, checked them against the record, and left them out as findings:

- **Paths and naming:** `pwd` prints `C:/…` (D3); `$SHELL` names cash (§4 #21); `sh`,
  `bash` and `cash` are cash (§4 #17).
- **Arguments and tools:** an argument reaches a command as written (D4); `which ls`
  prints a virtual path (D58); `ls` colours on a terminal (§4 #24).
- **Matching:** globbing and `find -name` are case-insensitive (D16); case-insensitive
  environment lookup (D31).
- **CRLF:** trimmed in `$(…)` and `read` (D20).
- **Descriptors:** redirections above fd 2 are refused for programs (D26); `/dev/stdout`
  shares the descriptor (§4 #39).
- **Process substitution:** `>(…)` is waited for at exit and `<(…)` is not (D17); temp
  files are used for `diff` and `cmp` (D17).
- **Jobs and processes:** `$!` is empty for a builtin-only job (§4 #14); CHLD is not
  raised for `$(…)` or bundled tools (D64); finished jobs are released, not reaped (D6);
  GUI apps outlive cash (D6, D45).
- **Signals:** TERM terminates console programs at once (D21); `kill 0` and `kill -1`
  (§4 #15, #16); a thread created during the suspend sweep is missed (D19).
- **Tools:** a bundled producer prints "Broken pipe" (§4 #22); bc's GNU extensions are
  errors and bc exits 1 after one (D56); sed and awk keep CRLF (D49); `chmod -x` warns
  and returns 0 (D34).
- **Build and release:** release builds use `panic = "unwind"` (README); binaries are
  unsigned (research/packaging-evaluation.md); there is no Linux CI (D43 amended);
  `multiple_crate_versions` is allowed; vendor/ is outside the lints (CASH-PATCHES.md).
- **Tests:** timing tests run on an idle machine and are never lengthened (TODO
  "Decided"); one `it` test executable (README).
- **Interactive:** Tab inserts the shared part, then opens the grid (TODO "Decided",
  D40); history appends at once (D44, as to *when*).

**Also checked and found sound:**

- **Handle inheritance.** std serialises inheritable stdio, and cash creates no
  inheritable handles, so the old report's §4.2 risk does not apply to production.
- **Unsafe code details:**
  - variable-length Win32 buffers are aligned or read with `read_unaligned`;
  - all size-retry loops are bounded;
  - the console control handlers cannot unwind;
  - the IAT patching in bundled-tool processes is bounds-checked.
- **Script behaviour matches Bash 5.3 exactly** for errexit, pipefail, the ERR trap,
  PIPESTATUS, lastpipe and `|&`.
- **Expansion matches bash** in more than 30 field-splitting and IFS cases and in
  arithmetic wrapping.
- **Pattern matching:** the regex translation has no catastrophic backtracking (bash
  itself does).
- **bc matches GNU bc.**
- **`tidy.ps1`'s deletion logic is safe.**

---

## 9. The report of 2026-09-22

| Item | Status now |
| --- | --- |
| §1/§2.1 Bypassed spawner, post-spawn race | **Race fixed** (`tokio_process.rs` spawns `CREATE_SUSPENDED`, contains, resumes). `cash_win32::spawn::spawn` remains, unused. The resume is unchecked (EXE-14). |
| §2.2/§5.6 PATHEXT in a `LazyLock` | The cache is gone; the value is still read from the process environment (ARCH-04). |
| §3.1 `panic = "abort"` | No longer applies: release is `unwind` by decision. |
| §3.2 Clippy covers all crates | No longer true: awk, bc and sed opted out afterwards (ARCH-01). |
| §3.3 Version coupling | Fixed: lockstep via `[workspace.dependencies]`. |
| §4.2 Handle-inheritance leak | Only in dead code; not a production risk. |
| §4.3/§5.5 Uncalled `build_cmd_command_line` | Batch escaping is used now, but it is wrong inside quotes (W32-01); the named function is still unused. |
| §4.5 `session.rs` enables VT input | Wrong for current code (D68 removes VT input before each prompt). |
| §5.1 `declare -t` filter | Fixed. |
| §5.2 `declare -i` parsing | Partly fixed (LANG-11). |
| §5.3/§5.4 Bundled pipelines serialised | Fixed (`seq 1 2000000 \| head -n2` takes 0.17 s); the `TODO(bundled)` comments are stale. |
| §5.7 Stale `chmod` in doctor | Fixed. |
| §5.8 `history`/`bind` need interactive state | Was wrong. |
| §5.9 Rebranding | Partly done. The URL it recommended (`thraa/cash`) is itself wrong (ARCH-02); `brush:` strings remain. |
| §6.2 Interactive tests Linux-only | Fixed: ConPTY tests, `pty_oracle`, `read_console`, `ctrl_c`. |
| §6.3 Differential suites stubbed | Partly fixed, partly wrong (BIN-05). |
| §6.4 ConPTY harness blueprint | Implemented (`cash_win32::conpty`). |
| §7.2 Subshell clone cost | Partly fixed (`rpds` history, `Arc` path index); env, functions, builtins and aliases are still deep-cloned per stage. |
