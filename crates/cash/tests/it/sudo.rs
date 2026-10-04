//! `sudo`, `su` and `sudoedit` as far as a test can take them without a UAC prompt or a
//! password: their help and refusals, `sudo -l`, `sudoedit` on a file it leaves alone,
//! and the elevated side's wrapper, which needs no elevation to run. In a shell already
//! elevated (CI's), what runs in place runs. `scripts/try-sudo.md` lists the rest, for a
//! person at a real UAC prompt.

#![allow(
    clippy::tests_outside_test_module,
    clippy::unwrap_used,
    reason = "Integration tests test the compiled binary and assert loudly on failure."
)]

use crate::common::{CASH, Scratch, isolate, output_of, run, run_in};

#[test]
fn sudo_su_and_sudoedit_print_their_usage() {
    let out = run("sudo --help; echo \"rc $?\"; su -h; sudoedit --help");
    for option in [
        "--login",
        "--preserve-env",
        "--non-interactive",
        "--edit",
        "-K",
    ] {
        assert!(out.stdout.contains(option), "{option}: {}", out.stdout);
    }
    assert!(out.stdout.contains("rc 0"), "{}", out.stdout);
    assert!(
        out.stdout.contains("--preserve-environment"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("SUDO_EDITOR"), "{}", out.stdout);
    assert!(!out.stderr.contains("not supported"), "{}", out.stderr);
}

/// What is refused is refused before anything elevates or asks a password: an unknown
/// account, `sudo -n` with nothing to approve it, `-K` with a command.
#[test]
fn sudo_refuses_before_asking() {
    let out = run(r#"sudo -u no-such-user-xyz true; echo "rc $?"
                     su no-such-user-xyz; echo "rc $?"
                     sudo -K true; echo "rc $?"
                     sudo -n true; echo "rc $?""#);
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(
        lines.get(..3),
        Some(&["rc 1", "rc 1", "rc 1"][..]),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("sudo: unknown user no-such-user-xyz"),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("su: unknown user no-such-user-xyz"),
        "{}",
        out.stderr
    );
    assert!(out.stderr.contains("-K takes no command"), "{}", out.stderr);
    // Elevated, or with gsudo's cache open, it runs; else it says so at once.
    assert!(
        lines.get(3) == Some(&"rc 0") || out.stderr.contains("sudo: a password is required"),
        "{}\n{}",
        out.stdout,
        out.stderr
    );
}

#[test]
fn sudo_l_says_who_and_how() {
    let out = run("sudo -l");
    assert_eq!(out.code, 0, "{}", out.stderr);
    for label in ["User:", "Administrator:", "Elevates with:", "cash.exe:"] {
        assert!(out.stdout.contains(label), "{label}: {}", out.stdout);
    }
}

/// An editor that changes nothing writes nothing back, so nothing elevates, and leaves no
/// copy behind. Elevated, a change is written into the file, and a new file is created.
#[test]
fn sudoedit_writes_back_only_what_changed() {
    let scratch = Scratch::new("sudoedit");
    let out = run_in(
        scratch.path(),
        r#"mkdir tmp; printf 'one\n' > f.txt
           TEMP=$PWD/tmp EDITOR=true sudoedit f.txt; echo "rc $?"
           cat f.txt; ls tmp | wc -l
           if [[ $EUID == 0 ]]; then
             TEMP=$PWD/tmp EDITOR="sh -c 'for f; do echo two >> \"\$f\"; done' ed" sudoedit f.txt new.txt
             echo "rc $?"; cat f.txt new.txt; ls tmp | wc -l
           fi"#,
    );
    let elevated_tail = "\nrc 0\none\ntwo\ntwo\n0";
    let expected = "rc 0\none\n0";
    assert!(
        out.stdout == expected || out.stdout == format!("{expected}{elevated_tail}"),
        "{}\n{}",
        out.stdout,
        out.stderr
    );
    assert!(out.stderr.contains("f.txt unchanged"), "{}", out.stderr);
}

/// The elevated side of `sudo`, run directly: it passes the status on, makes the user who
/// asked the owner of what it creates (in an elevated test, rather than Administrators),
/// and says so when it runs as another account than the one that asked.
#[test]
fn the_elevated_side_keeps_files_the_users() {
    let scratch = Scratch::new("sudo-owner");
    let sid = cash_win32::account::current_user().unwrap().text();
    let wrapped = |sid: &str| {
        let mut command = std::process::Command::new(CASH);
        isolate(&mut command)
            .args([
                "--invoke-bundled",
                "--sudo-owner",
                sid,
                CASH,
                "--no-config",
                "-c",
            ])
            .arg("touch made; stat -c %U made; whoami; rm made; exit 3")
            .current_dir(scratch.path());
        output_of(&mut command)
    };

    let out = wrapped(&sid);
    assert_eq!(out.code, 3, "{}", out.stderr);
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.len(), 2, "{}", out.stdout);
    assert!(lines[0].eq_ignore_ascii_case(lines[1]), "{}", out.stdout);
    assert!(!out.stderr.contains("not you"), "{}", out.stderr);

    // SYSTEM's SID, which no test runs as: another account asked.
    let out = wrapped("S-1-5-18");
    assert_eq!(out.code, 3, "{}", out.stderr);
    assert!(
        out.stderr.contains("not you; files and ~ are theirs"),
        "{}",
        out.stderr
    );
}
