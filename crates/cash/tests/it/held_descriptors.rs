//! D26: descriptors above 2 that the shell merely holds must not break native exes.
//!
//! `exec 3>&1 1>log; ...; tool.exe; ... >&3` is the ordinary logging idiom, and every
//! external command after the `exec` used to fail with "fd redirections". A native exe
//! cannot see fd 3 either way, so only a redirection written on the command itself
//! (`tool.exe 3>x`) is the documented error. A redirection on an enclosing compound
//! command or function call does not count as "on the command": `while read -u 3 ...;
//! done 3<file` wraps whatever runs inside it, and the loop body has no say in that.

#![allow(
    clippy::tests_outside_test_module,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly"
)]

use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::common::cash_command;

/// The fd-injection refusal from `sys/stubs/commands.rs`.
const REFUSAL: &str = "fd redirections";

struct Output {
    stdout: String,
    stderr: String,
}

fn cash_in(dir: &Path, script: &str) -> Output {
    let out = cash_command()
        .current_dir(dir)
        .args(["--noprofile", "--norc", "-c", script])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        stderr: String::from_utf8_lossy(&out.stderr).replace("\r\n", "\n"),
    }
}

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cash-heldfd-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn read(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name))
        .unwrap()
        .replace("\r\n", "\n")
}

#[test]
fn an_exec_held_descriptor_does_not_stop_a_native_exe() {
    let dir = fixture("exec");
    let out = cash_in(
        &dir,
        "exec 3>fd3.log; where.exe cmd >/dev/null; echo rc=$?; echo logged >&3",
    );
    assert_eq!(out.stdout, "rc=0\n", "stderr: {}", out.stderr);
    assert!(!out.stderr.contains(REFUSAL), "stderr: {}", out.stderr);
    assert_eq!(read(&dir, "fd3.log"), "logged\n");
}

#[test]
fn the_logging_idiom_keeps_both_streams() {
    let dir = fixture("idiom");
    let out = cash_in(
        &dir,
        "exec 3>&1 1>log.txt; echo inlog; where.exe cmd; echo rc=$?; echo toconsole >&3",
    );
    assert_eq!(out.stdout, "toconsole\n", "stderr: {}", out.stderr);
    let log = read(&dir, "log.txt");
    assert!(log.starts_with("inlog\n"), "log: {log}");
    assert!(log.to_ascii_lowercase().contains("cmd.exe"), "log: {log}");
    assert!(log.ends_with("rc=0\n"), "log: {log}");
}

#[test]
fn a_redirection_above_2_on_the_native_exe_itself_is_still_refused() {
    let dir = fixture("own");
    let out = cash_in(&dir, "where.exe cmd 3>x.log; echo rc=$?");
    assert_eq!(out.stdout, "rc=1\n");
    assert!(out.stderr.contains(REFUSAL), "stderr: {}", out.stderr);

    // Duplicating an already-held descriptor onto the command is still on the command.
    let out = cash_in(&dir, "exec 3>y.log; where.exe cmd 4>&3; echo rc=$?");
    assert_eq!(out.stdout, "rc=1\n");
    assert!(out.stderr.contains(REFUSAL), "stderr: {}", out.stderr);

    // So is a variable-allocated one.
    let out = cash_in(&dir, "where.exe cmd {fd}>z.log; echo rc=$?");
    assert_eq!(out.stdout, "rc=1\n");
    assert!(out.stderr.contains(REFUSAL), "stderr: {}", out.stderr);
}

#[test]
fn a_descriptor_copied_down_into_0_1_or_2_is_a_step_not_a_refusal() {
    // `cat 3< f <&3` and the swap idiom `cmd 3>&1 1>&2 2>&3` (`dialog`, `whiptail`) were
    // refused: fd 3 is set on the command, but only to be copied into 0, 1 or 2, and the
    // program gets what it held there (TODO 4.4). Bash runs both.
    let dir = fixture("step");
    std::fs::write(dir.join("f"), "contents\n").unwrap();
    let out = cash_in(
        &dir,
        "cat 3< f <&3; echo rc=$?; cmd.exe /d /c \"echo swapped\" 3>&1 1>&2 2>&3; echo rc=$?",
    );
    assert_eq!(
        out.stdout, "contents\nrc=0\nrc=0\n",
        "stderr: {}",
        out.stderr
    );
    assert_eq!(out.stderr.trim_end(), "swapped");

    // Set again after the copy, it is on the command once more.
    let out = cash_in(&dir, "cat 3< f <&3 3< f; echo rc=$?");
    assert_eq!(out.stdout, "rc=1\n");
    assert!(out.stderr.contains(REFUSAL), "stderr: {}", out.stderr);
}

#[test]
fn closing_a_held_descriptor_on_the_command_is_not_a_redirection_to_refuse() {
    let dir = fixture("close");
    let out = cash_in(
        &dir,
        "exec 3>f.log; where.exe cmd 3>&- >/dev/null; echo rc=$?",
    );
    assert_eq!(out.stdout, "rc=0\n", "stderr: {}", out.stderr);
}

#[test]
fn an_enclosing_compound_redirection_is_not_on_the_command() {
    let dir = fixture("group");
    let out = cash_in(
        &dir,
        "{ where.exe cmd >/dev/null; echo rc=$?; echo grouped >&3; } 3>g.log",
    );
    assert_eq!(out.stdout, "rc=0\n", "stderr: {}", out.stderr);
    assert_eq!(read(&dir, "g.log"), "grouped\n");

    std::fs::write(dir.join("in.txt"), "a\nb\n").unwrap();
    let out = cash_in(
        &dir,
        "while read -r -u 3 l; do where.exe cmd >/dev/null && echo \"$l\"; done 3<in.txt",
    );
    assert_eq!(out.stdout, "a\nb\n", "stderr: {}", out.stderr);

    let out = cash_in(
        &dir,
        "f() { where.exe cmd >/dev/null; echo rc=$?; }; f 3>func.log",
    );
    assert_eq!(out.stdout, "rc=0\n", "stderr: {}", out.stderr);
}

#[test]
fn held_descriptors_do_not_reach_pipelines_subshells_or_substitutions() {
    let dir = fixture("pipes");
    let out = cash_in(
        &dir,
        "exec 3>p.log; where.exe cmd | cat >/dev/null; echo \"ps=${PIPESTATUS[*]}\"; \
         (where.exe cmd >/dev/null); echo sub=$?; x=$(where.exe cmd); echo cs=$?; \
         echo piped >&3",
    );
    assert_eq!(
        out.stdout, "ps=0 0\nsub=0\ncs=0\n",
        "stderr: {}",
        out.stderr
    );
    assert_eq!(read(&dir, "p.log"), "piped\n");
}

#[test]
fn bundled_coreutils_run_with_a_held_descriptor() {
    // The bundled coreutils re-enter cash.exe as a process, so they took the same
    // failing path as any native exe.
    let dir = fixture("coreutils");
    std::fs::write(dir.join("in.txt"), "hello\n").unwrap();
    let out = cash_in(
        &dir,
        "exec 3>c.log; cat in.txt; wc -l <in.txt; echo done >&3; cat c.log",
    );
    assert!(!out.stderr.contains(REFUSAL), "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "hello\n1\ndone\n", "stderr: {}", out.stderr);
}
