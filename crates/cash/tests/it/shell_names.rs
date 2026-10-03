//! The name a cash is started by, when a cash starts it as `bash` or `sh` (D7) or with
//! `exec -a` (EXE-12).
//!
//! Bash takes `$0`, and POSIX mode for `sh`, from `argv[0]`. Windows gives a program none
//! of its own, and `bash -c 'echo $0'` printed cash's path; `sh` was not POSIX. A cash
//! now tells the cash it starts the name, through `CASH_ARGV0`, which goes no further.
//! And `exec bash` ran Git's bash, found on `PATH`, rather than cash.

#![allow(
    clippy::tests_outside_test_module,
    clippy::literal_string_with_formatting_args,
    reason = "an integration test is outside a test module by construction, and its \
              scripts' `${…}` are the shell's, not format arguments"
)]

use crate::common::{Scratch, run, run_in};

#[test]
fn a_sh_shebang_is_posix_and_a_bash_one_is_not() {
    let scratch = Scratch::new("shell-names");
    for (name, line) in [
        ("s.sh", "#!/bin/sh"),
        ("e.sh", "#!/usr/bin/env sh"),
        ("b.sh", "#!/usr/bin/env bash"),
    ] {
        std::fs::write(
            scratch.join(name),
            format!("{line}\nshopt -oq posix && echo posix; echo \"$0\"\n"),
        )
        .unwrap();
    }
    let out = run_in(scratch.path(), "./s.sh; ./e.sh; ./b.sh");
    assert_eq!(
        out.stdout, "posix\n./s.sh\nposix\n./e.sh\n./b.sh",
        "{}",
        out.stderr
    );
}

#[test]
fn bash_and_sh_are_their_own_names() {
    let out = run(r#"bash -c 'echo "$0"'; sh -c 'echo "$0"'; bash -c 'bash -c "echo \$0"'"#);
    assert_eq!(out.stdout, "bash\nsh\nbash", "{}", out.stderr);
}

#[test]
fn env_and_timeout_pass_the_name_on_too() {
    let out = run(
        r#"env bash -c 'echo "$0"'; timeout 5 sh -c 'echo "$0"; shopt -oq posix && echo posix'"#,
    );
    assert_eq!(out.stdout, "bash\nsh\nposix", "{}", out.stderr);
}

#[test]
fn sh_is_posix_with_every_builtin_and_bash_is_not() {
    let out = run(r"sh -c 'shopt -oq posix && echo posix; type -t shopt'
          bash -c 'shopt -oq posix || echo not-posix'");
    assert_eq!(out.stdout, "posix\nbuiltin\nnot-posix", "{}", out.stderr);
}

#[test]
fn exec_runs_cash_for_bash_and_sh_and_passes_its_a_name() {
    let out = run(r#"(exec bash -c 'echo "${CASH_VERSION:+cash} $0"')
           bash -c 'exec -a foo bash -c "echo \$0"'
           exec sh -c 'echo "${CASH_VERSION:+cash} $0"'"#);
    assert_eq!(out.stdout, "cash bash\nfoo\ncash sh", "{}", out.stderr);
}

#[test]
fn the_name_reaches_no_other_program() {
    let out = run(
        r#"bash -c 'echo "${CASH_ARGV0-unset}"; cmd /c "set CASH_ARGV0" >nul 2>&1 || echo gone'"#,
    );
    assert_eq!(out.stdout, "unset\ngone", "{}", out.stderr);
}
