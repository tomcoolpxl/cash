//! `pwd`'s three options. `-W` is MSYS2 Bash's (the folder in Windows form), which the
//! usage offered and cash refused as invalid; cash's paths are in Windows form already, so
//! it prints what `pwd` prints. `-P` printed the folder as the standard library resolves
//! it, `\\?\C:\…`, where every other path cash prints is `C:/…`.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::{Scratch, run_in};

#[test]
fn pwd_w_prints_the_folder_as_pwd_does() {
    let dir = Scratch::new("pwd-w");
    let out = run_in(
        dir.path(),
        r#"a=$(pwd); b=$(pwd -W); c=$(pwd -LW); [ "$a" = "$b" ] && [ "$a" = "$c" ] && echo same"#,
    );
    assert_eq!(
        (out.stdout.as_str(), out.stderr.as_str(), out.code),
        ("same", "", 0)
    );
}

#[test]
fn pwd_p_prints_the_resolved_folder_in_cashs_spelling() {
    let dir = Scratch::new("pwd-p");
    let out = run_in(
        dir.path(),
        r"mkdir real && cmd /c 'mklink /J jn real' > /dev/null && cd jn
pwd; pwd -P; pwd -WP",
    );
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.len(), 3, "{out:?}", out = out.stdout);
    assert!(lines[0].ends_with("/jn"), "pwd: {}", lines[0]);
    for physical in &lines[1..] {
        assert!(physical.ends_with("/real"), "pwd -P: {physical}");
        assert!(
            !physical.contains(['\\', '?']) && physical.as_bytes().get(1) == Some(&b':'),
            "pwd -P: {physical}"
        );
    }
    assert_eq!((out.stderr.as_str(), out.code), ("", 0));
}
