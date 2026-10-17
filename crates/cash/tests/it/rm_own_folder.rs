//! `rm` and the folder it runs in. The bundled `rm` runs as a child in the shell's
//! folder, and Windows will not remove a folder a process sits in, so the cleanup idiom
//! `d=$(mktemp -d); trap 'rm -rf "$d"' EXIT; cd "$d"` left `$d` behind ("The process
//! cannot access the file because it is being used by another process"). cash's own
//! `rm` (`crates/cash-uutils`) moves out of a folder it is to remove first.

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::{cash_command, output_of, run_in};

#[test]
fn a_trap_removes_the_folder_the_shell_is_in() {
    let out = output_of(cash_command().args([
        "-c",
        r#"d=$(mktemp -d); trap 'rm -rf "$d"' EXIT; cd "$d"; mkdir -p sub/deeper; cd sub; : > x; echo "$d""#,
    ]));
    assert!(!out.stdout.is_empty(), "no folder made: {}", out.stderr);
    assert!(
        !std::path::Path::new(&out.stdout).exists(),
        "{} was left: {}",
        out.stdout,
        out.stderr
    );
    assert_eq!((out.code, out.stderr.as_str()), (0, ""));
}

#[test]
fn rm_out_of_its_folder_still_refuses_dot_dot_and_finds_no_relative_name_after() {
    // As on Linux: once the folder rm sits in is gone, a relative name names nothing
    // (`-f` says nothing of it), and it is never looked for from where rm moved to.
    let dir = tempfile::tempdir().unwrap();
    let out = run_in(
        dir.path(),
        "mkdir -p t2/a/b t3 t4/a; \
         (cd t2/a/b && rm -rf ../../../t2 ../../../t3; echo \"status $?\"); \
         (cd t4/a && rm -rf ..; echo \"status $?\"); \
         ls",
    );
    assert_eq!(out.stdout, "status 0\nstatus 1\nt3\nt4", "{}", out.stderr);
    assert!(
        out.stderr
            .contains("refusing to remove '.' or '..' directory: skipping '..'"),
        "{}",
        out.stderr
    );
}
