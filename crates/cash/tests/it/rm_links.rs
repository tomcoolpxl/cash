//! `rm -r` and folder links: a junction inside the folder being removed is taken away
//! as the link it is, never followed, whether its target is still there or not. uutils'
//! `rm` 0.12.0 removed it as a file, which Windows refuses ("Permission denied"); cash's
//! own `rm` (`crates/cash-uutils`) does not. Junctions need no privilege,
//! so `mklink /J` makes them on any machine.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction"
)]

use std::process::Stdio;

use crate::common::{Scratch, cash_command};

/// Runs `script` in `dir`: standard output and error together, and the status.
fn run_in(dir: &Scratch, script: &str) -> (String, i32) {
    let out = cash_command()
        .args(["--norc", "--noprofile", "-c", script])
        .current_dir(dir.path())
        .stdin(Stdio::null())
        .output()
        .expect("run cash");
    (
        String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n")
            + &String::from_utf8_lossy(&out.stderr).replace("\r\n", "\n"),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn rm_r_removes_a_junction_inside_a_folder_and_keeps_its_target() {
    let dir = Scratch::new("rm-junction");
    let (out, code) = run_in(
        &dir,
        r"mkdir -p outside t/sub && echo keep > outside/f && echo sub > t/sub/f
cmd /c 'mklink /J t\out outside' > /dev/null && cmd /c 'mklink /J t\in t\sub' > /dev/null
[ -L t/out ] && [ -L t/in ] && echo linked
rm -rf t; echo rc=$?
ls -A; cat outside/f",
    );
    assert_eq!((out.as_str(), code), ("linked\nrc=0\noutside\nkeep\n", 0));
}

#[test]
fn rm_r_removes_a_junction_whose_target_went_first() {
    let dir = Scratch::new("rm-dangling");
    let (out, code) = run_in(
        &dir,
        r"mkdir -p u/target && cmd /c 'mklink /J u\jn u\target' > /dev/null
rm -rf u/target && [ ! -e u/jn ] && echo dangling
rm -rf u; echo rc=$?
ls -A | wc -l",
    );
    assert_eq!((out.as_str(), code), ("dangling\nrc=0\n0\n", 0));
}
