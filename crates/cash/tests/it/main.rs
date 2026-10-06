//! The tests that drive `cash.exe`, built as one test executable.
//!
//! Cargo makes every file directly under `tests/` an executable of its own, and each
//! linked the whole shell and carried its own debug info: 55 of them made a test build
//! slow to link and gigabytes large. As modules of this one executable they link once
//! (the Cargo book's advice, and matklad's "Delete Cargo Integration Tests").
//!
//! nextest, which CI and `cargo xtask` use, still runs every test in a process of its
//! own; plain `cargo test` runs them on threads of one process, as it always ran the
//! tests of one file. To run one file's tests:
//!
//! ```text
//! cargo nextest run -p cash -E 'binary(it) and test(/^acceptance::/)'
//! cargo test -p cash --test it acceptance::
//! ```
//!
//! Beside this executable: `integration_tests.rs`, which has a harness of its own, and
//! the ConPTY suites `conpty_interactive_tests.rs`, `pty_oracle.rs` and `pty_latency.rs`.

mod abbr;
// cash's own acceptance corpus — §4's divergences and every decision wired into the
// shell, asserted against the real cash.exe.
mod acceptance;
// The awkward corners: spaces in paths, empty files, nesting, and the boundaries where
// two decisions meet.
mod acceptance_edge_cases;
// Indexed arrays as Bash has them: negative indices, `[expr]=` in a list, namerefs.
mod arrays;
// D11/D22: `$!` and background job identity.
mod background_pid;
// EXE-03, EXE-06, EXE-08: background jobs of builtins start, run beside the foreground, and end.
mod background_builtins;
mod bash_gaps;
mod batch_relative_path;
mod bc;
mod bc_cash;
// ARCH-02, ARCH-09, ARCH-10: what a user sees names cash, not brush.
mod brush_names;
// Comprehensive parameter matrix across builtins.
mod builtin_parameters;
mod bundled_paths;
mod cd_errors;
// The binary, a run of it isolated from the user's settings, and a scratch folder.
mod common;
// D40: `docker completion bash` and friends, which need bash-completion's helpers.
mod completion_scripts;
// D40: completion is the one part of the shell with no command-line surface, so these
// drive a `Shell` directly.
mod completion_windows;
// coolfetch system banner.
mod coolfetch;
// EXE-04: `coproc` ends when its input does, and is a job with a pid.
mod coprocesses;
// Inputs that used to take the whole shell down: deep recursion, and panics.
mod crash_safety;
// M2: real scripts, written the way they are written on Linux.
mod corpus;
// BIN-05: every `tests/corpus` case against the oracle's frozen output.
mod corpus_goldens;
mod crlf_scripts;
// D13: what the keyboard's Ctrl-C does to a script, outside `read`.
mod ctrl_c;
// D7: `/dev/stdin`, `/dev/stdout`, `/dev/stderr` and `/dev/fd/N` as the shell's descriptors.
mod dev_descriptors;
mod directory_stack;
mod disown;
mod doctor_busybox;
// D22: a job's pid names the process cash started, or none, after the job has ended.
mod ended_children;
// D3/D5: what a script sees of its environment, and the here-document deadlock.
mod environment_contract;
// Errors that abandon the whole top-level command.
mod error_jumps;
// 13.4: errors as Bash words them, named by file and line, coloured only on a terminal.
mod error_messages;
// Expansions as Bash performs them.
mod expansions;
// Extended patterns, `!(…)` above all.
mod extensionless_lookup;
mod extglob;
// find and xargs parity.
mod find_xargs;
mod finished_command_orphans;
mod folder_history;
mod fuser_lsof;
// Git for Windows' git-prompt.sh (`__git_ps1`), sourced from the Git install.
mod git_prompt;
// BIN-05: Git's prompt scripts against Git Bash's frozen output.
mod git_prompt_goldens;
mod gui_apps_outlive;
mod held_descriptors;
// `help` and `cash help`: the catalogue covers every builtin, and its pages, topics and
// search.
mod help_catalogue;
// Here-documents as Bash reads and writes them.
mod here_documents;
// D22/§4 #20: $UID agreeing with `id`, and `jobs -l`.
mod identity_and_jobs;
// D69: `cash --init-rc`, the starter ~/.bashrc Scoop writes for a user with none.
mod init_rc;
// Text that must never become a command: `start`'s target, a twice-expanded subscript.
mod injection;
mod job_groups;
mod kill_family;
// A job a signal ended is told of by the signal, in `jobs` and in a script's notice.
mod killed_jobs;
// D21/D22: what a kill target means, including the `kill 0` that used to signal the
// whole console.
mod kill_targets;
mod kill_term;
mod line_ending_tools;
mod link_tools;
// Programs started from a folder too long for Windows to start one in.
mod long_folders;
mod ls_builtin;
mod msys_args;
mod namerefs;
// D48: one pager behind `less` and `more`.
mod pager;
mod ping;
// D11/D26: pipelines that actually overlap, and `read -t`.
mod pipeline_concurrency;
// EXE-02: an error, `exit` or `break` in a pipeline stage ends only the stage.
mod pipeline_errors;
// How a test asks whether a process is gone, when its pid may be another's by now.
mod process_identity;
// A `<(…)` inside a word, as in `--file=<(cmd)`, is part of the word.
mod process_substitution_words;
// D48: `ps`, which uutils does not carry.
mod ps_builtin;
mod pure_bash_corpus;
// `read -t`, `-d`, `-n` and `-s` at a console, which collects a line unless told not to.
mod read_console;
mod real_world_tests;
// D3/D7/D8/D34/D35: what cash says it will run vs what it runs.
mod resolution_honesty;
// EXE-07: a script that does not parse runs a complete command at a time, up to the error.
mod run_by_command;
// `select`, a bash construct cash could not parse at all.
mod select_clause;
mod shebang_dispatch;
// D10: the working directory, environment and PATH are the shell's, not the process's.
// EXE-12: the name a cash is started by, as bash, sh or exec -a.
mod shell_names;
mod shell_state;
// Assignments to the variables the shell keeps itself.
mod small_tools;
mod special_variables;
mod ss;
mod stat_builtin;
// $BASH_SUBSHELL and the `set -x` prefix count what Bash counts.
mod subshell_levels;
// `sudo`, `su` and `sudoedit`, as far as they go without a UAC prompt or a password.
mod sudo;
mod terminal_profile;
// D48/D49/D56: the bundled awk, sed and bc in a pipeline.
mod text_tools;
// top builtin batch mode and monitoring.
mod top_builtin;
mod tree_builtin;
mod where_command;
mod winpaths;

// The upstream bc suite (see bc.rs) reaches its test library as `crate::plib`.
use bc::plib;
// `stty`: GNU's words on the console's modes, and `stty -echo` hiding what `read` reads.
mod stty_builtin;
// `tput` against ncurses 6.6's frozen output for xterm-256color, and the console's size.
mod tput_builtin;
