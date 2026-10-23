# Interpreter dispatch: one future per nesting level, not five per command

The question (the user, 2026-10-23): the benchmark after 2.0.0 showed cash's interpreter
slower than at 1.23.0. Simple commands were fixed (`perf(core)`, e9529401); what was left,
the user chose to re-engineer rather than patch (D86).

## What a profile showed

samply, `i=0; while ((i<2000000)); do ((i++)); done`, the `profiling` build (21,391
samples on the shell's thread, after e9529401):

| where | share | what it is |
|---|---|---|
| `memcpy` | 19% self | futures moved into the boxes `#[async_trait]` makes |
| `mi_malloc_aligned` and the allocator | 14% inclusive | those boxes, one per level per command |
| `eval_expr_impl` | 20% inclusive | the arithmetic itself: assignment 10%, lookup 8% |
| `ExecutionParameters::clone` | 3% | three copies per command (cheap: the file table is shared) |

Every command in a loop's body runs through `CompoundList::execute`,
`AndOrList::execute`, `Pipeline::execute`, `Command::execute_in_pipeline` and the
node's own `execute`. Each is an `#[async_trait]` method, which builds its future on the
stack and moves it into a new `Box`: five allocations and five copies per command, each
copy as large as everything that level can await, inlined. `Pipeline`'s future holds the
pipe set-up and the wait for its stages even for one command. As features came in
between 1.39 and 1.49 (traps where Bash runs them, POSIX mode, held redirections), the
futures grew, and so did every copy: the slowdown.

## The design

1. **The execution traits return `impl Future + Send`** (return-position `impl Trait`
   in traits, stable Rust), written as `async move` blocks. A caller's future then holds
   its callee's in place: no box, no copy between levels.
2. **A box only where the syntax tree recurses**, which a future type cannot do without
   one: where a compound command (`{ }`, `( )`, `if`, `while`, `until`, `for`, `case`,
   `select`) runs its body, and where a function's body runs. A loop's turn then makes
   one allocation for its body, not five for each command in it.
3. **A one-command pipeline takes a path of its own** in `Pipeline::execute`: no pipes,
   no queue of results, the command run in this shell and its status kept as the general
   path keeps it. The general path stays for two commands and more, and for `time`.

Not changed: what a command does, its errors, traps, `set -e`, `PIPESTATUS`, job control.
Each step is built, run through the whole suite (both lanes and the corpus) and timed
against 1.39 and 2.0.1 (`dist` builds, Git Bash as the control) before the next.

Turned down: a synchronous interpreter for commands that never wait (the largest gain,
but a second interpreter beside the first, every feature written twice); shrinking each
future by boxing its slow paths one by one (the cheaper fix, offered and not picked).
