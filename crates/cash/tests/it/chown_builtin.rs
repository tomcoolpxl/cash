//! `chown` and `chgrp`: Windows sets an owner only with privileges a user lacks and has no
//! POSIX groups, so they change nothing, say so once, and return 0, as `chmod -x` does; a
//! file that is not there is still GNU's error.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::{Scratch, run, run_in};

#[test]
fn chown_and_chgrp_say_once_that_nothing_changed_and_succeed() {
    let dir = Scratch::new("chown-warns");
    std::fs::write(dir.join("a"), "a").expect("write a");
    std::fs::write(dir.join("b"), "b").expect("write b");
    let out = run_in(dir.path(), "chown root:root a b; echo \"st=$?\"");
    assert_eq!(out.stdout, "st=0");
    assert_eq!(
        out.stderr,
        "chown: owners are not changed this way on Windows; nothing changed \
         (sudo icacls FILE /setowner USER sets one)"
    );
    let out = run_in(dir.path(), "chgrp -R staff a; echo \"st=$?\"");
    assert_eq!(out.stdout, "st=0");
    assert_eq!(
        out.stderr,
        "chgrp: Windows has no POSIX groups; nothing changed"
    );
}

#[test]
fn f_is_quiet_and_v_names_each_file() {
    let dir = Scratch::new("chown-fv");
    std::fs::write(dir.join("a"), "a").expect("write a");
    let out = run_in(dir.path(), "chown -f me a; echo \"st=$?\"");
    assert_eq!((out.stdout.as_str(), out.stderr.as_str()), ("st=0", ""));
    let out = run_in(dir.path(), "chown -v --from=x me a");
    assert_eq!(out.stdout, "ownership of 'a' retained");
    assert_eq!(out.code, 0);
}

#[test]
fn a_missing_file_or_operand_is_gnus_error() {
    let dir = Scratch::new("chown-missing");
    let out = run_in(dir.path(), "chown -f me nosuch; echo \"st=$?\"");
    assert_eq!(out.stdout, "st=1");
    assert_eq!(out.stderr, "");
    let out = run_in(dir.path(), "chown me nosuch");
    assert_eq!(out.code, 1);
    assert!(
        out.stderr
            .ends_with("chown: cannot access 'nosuch': No such file or directory"),
        "{}",
        out.stderr
    );

    let out = run("chown");
    assert_eq!(out.code, 1);
    assert_eq!(
        out.stderr,
        "chown: missing operand\nTry 'chown --help' for more information."
    );
    let out = run("chgrp staff");
    assert_eq!(
        out.stderr,
        "chgrp: missing operand after 'staff'\nTry 'chgrp --help' for more information."
    );
}

#[test]
fn reference_takes_the_files_alone_and_must_exist() {
    let dir = Scratch::new("chown-reference");
    std::fs::write(dir.join("a"), "a").expect("write a");
    let out = run_in(dir.path(), "chown -f --reference=a a; echo \"st=$?\"");
    assert_eq!(out.stdout, "st=0");
    let out = run_in(dir.path(), "chown --reference=nosuch a");
    assert_eq!(out.code, 1);
    assert_eq!(
        out.stderr,
        "chown: failed to get attributes of 'nosuch': No such file or directory"
    );
}

#[test]
fn chmod_r_passes_a_link_by_and_names_what_it_changes_as_typed() {
    // chmod -R went into no link before either, but by a walk of its own; on the shared
    // walker its names come from the operand as typed, with no doubled `/` after `d/`, and
    // -v names them so too, where it printed the absolute path.
    let scratch = Scratch::new("chmod-walk");
    let d = scratch.path().join("d");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("f"), "x").unwrap();
    cash_win32::junction::point(&d.join("up"), scratch.path()).unwrap();
    let out = run_in(
        scratch.path(),
        "chmod -Rv u+w d/ | sed \"s/ retained.*//\" | sort",
    );
    assert_eq!(out.stdout, "mode of 'd/'\nmode of 'd/f'", "{}", out.stderr);
}
