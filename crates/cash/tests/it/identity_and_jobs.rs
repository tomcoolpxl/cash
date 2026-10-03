//! Who the shell thinks it is, and what it says about its jobs — **D22**, **§4 #20**.
//!
//! `$UID` and `$EUID` reported a hardcoded 1000 while the `id` builtin reported the
//! account's real RID, so the shell disagreed with its own builtin about who was running
//! it. `[ "$UID" = "$(id -u)" ]` — a normal way to check for a privilege change — saw two
//! different answers and concluded something had happened.
//!
//! `jobs -l` refused outright with "not yet implemented", although the pid it wanted was
//! already tracked for `$!` and `kill %1`.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::needless_raw_string_hashes,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly rather than be \
              threaded back through a Result. Shell snippets are spelled with hashes \
              throughout, including where they are not strictly needed, because \
              alternating the two forms by accident of content reads worse."
)]

use std::process::Command;

use crate::common::run as cash;

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

#[test]
fn uid_agrees_with_the_id_builtin() {
    // The shell and its own builtin must not disagree about who is running.
    let out = cash(r#"[ "$UID" = "$(id -u)" ] && echo agree || echo "differ: $UID vs $(id -u)""#);
    assert_eq!(out.stdout, "agree", "stderr: {}", out.stderr);
}

#[test]
fn euid_agrees_too() {
    let out = cash(r#"[ "$EUID" = "$(id -u)" ] && echo agree || echo "differ: $EUID""#);
    assert_eq!(out.stdout, "agree", "stderr: {}", out.stderr);
}

#[test]
fn uid_is_the_accounts_rid_not_a_placeholder() {
    // 1000 was the hardcoded sentinel. A real account RID starts at 500 (Administrator)
    // or 1001 for the first ordinary user, so the sentinel is distinguishable.
    let out = cash(r#"echo "$UID""#);
    let uid: u64 = out
        .stdout
        .parse()
        .unwrap_or_else(|_| panic!("not numeric: {}", out.stdout));
    // Elevated shells report 0, the root convention (spec §4 row 20); GitHub's runners
    // are elevated. `net session` succeeds only in an elevated process, so it tells the
    // two cases apart without asking cash.
    let elevated = Command::new("net")
        .arg("session")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if elevated {
        assert_eq!(uid, 0, "an elevated shell reports uid 0");
        let id = cash(r#"id -u; id -g; echo "$UID $EUID""#);
        assert_eq!(
            id.stdout, "0\n0\n0 0",
            "id and $UID/$EUID agree: {}",
            id.stderr
        );
    } else {
        assert!(
            uid > 0,
            "uid should be the account RID in a non-elevated shell"
        );
    }
}

#[test]
fn uid_is_stable_across_invocations() {
    // Derived from the account's SID, so it cannot wander between runs.
    let first = cash(r#"echo "$UID""#).stdout;
    let second = cash(r#"echo "$UID""#).stdout;
    assert_eq!(first, second, "uid changed between runs");
}

#[test]
fn whoami_and_id_name_the_same_account() {
    let out = cash(
        r#"w=$(whoami); i=$(id -un); case "$i" in *"$w") echo agree ;; *) echo "differ: [$w] [$i]" ;; esac"#,
    );
    assert_eq!(out.stdout, "agree", "stderr: {}", out.stderr);
}

// ---------------------------------------------------------------------------
// `jobs -l`
// ---------------------------------------------------------------------------

#[test]
fn jobs_l_is_implemented() {
    let out = cash(r#"ping.exe -n 10 127.0.0.1 > /dev/null & jobs -l; kill -KILL $! 2>/dev/null"#);
    assert!(
        !out.stderr.contains("not yet implemented"),
        "jobs -l still refuses: {}",
        out.stderr
    );
    assert!(!out.stdout.is_empty(), "jobs -l printed nothing");
}

#[test]
fn jobs_l_shows_a_pid_that_matches_the_one_the_shell_reports() {
    let out = cash(
        r#"
        ping.exe -n 10 127.0.0.1 > /dev/null &
        pid=$!
        listed=$(jobs -l | tr -s ' ' | cut -d' ' -f2)
        [ "$pid" = "$listed" ] && echo agree || echo "differ: [$pid] [$listed]"
        kill -KILL "$pid" 2>/dev/null
        "#,
    );
    assert_eq!(out.stdout, "agree", "stderr: {}", out.stderr);
}

#[test]
fn jobs_l_puts_the_pid_after_the_job_marker() {
    // bash's layout: `[1]+ 12345 Running   sleep 30 &`. Splitting on the first space
    // would land inside the command, because the status and command are tab-separated.
    let out = cash(r#"ping.exe -n 10 127.0.0.1 > /dev/null & jobs -l; kill -KILL $! 2>/dev/null"#);
    let line = out.stdout.lines().next().unwrap_or_default();
    assert!(line.starts_with("[1]"), "no job marker: {line:?}");

    let second = line.split_whitespace().nth(1).unwrap_or_default();
    assert!(
        second.parse::<u32>().is_ok(),
        "the pid is not the second field: {line:?}"
    );
}

#[test]
fn plain_jobs_does_not_show_a_pid() {
    let out = cash(r#"ping.exe -n 10 127.0.0.1 > /dev/null & jobs; kill -KILL $! 2>/dev/null"#);
    let line = out.stdout.lines().next().unwrap_or_default();
    let second = line.split_whitespace().nth(1).unwrap_or_default();
    assert!(
        second.parse::<u32>().is_err(),
        "a bare `jobs` leaked a pid: {line:?}"
    );
}

#[test]
fn jobs_p_still_prints_only_pids() {
    let out = cash(r#"ping.exe -n 10 127.0.0.1 > /dev/null & jobs -p; kill -KILL $! 2>/dev/null"#);
    let line = out.stdout.lines().next().unwrap_or_default();
    assert!(
        line.trim().parse::<u32>().is_ok(),
        "jobs -p printed more than a pid: {line:?}"
    );
}

#[test]
fn a_job_spec_selects_one_job() {
    // `jobs %1` used to refuse with "not yet implemented", although the resolver already
    // existed for `kill %1` and `wait %1`.
    let out = cash(r#"ping.exe -n 10 127.0.0.1 > /dev/null & jobs %1; kill -KILL $! 2>/dev/null"#);
    assert!(
        out.stdout.starts_with("[1]"),
        "jobs %1 did not list the job: {} {}",
        out.stdout,
        out.stderr
    );
}

#[test]
fn a_job_spec_works_with_the_other_options() {
    let listed =
        cash(r#"ping.exe -n 10 127.0.0.1 > /dev/null & jobs -l %1; kill -KILL $! 2>/dev/null"#);
    let second = listed
        .stdout
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_string();
    assert!(
        second.parse::<u32>().is_ok(),
        "jobs -l %1 lost the pid: {}",
        listed.stdout
    );

    let pids =
        cash(r#"ping.exe -n 10 127.0.0.1 > /dev/null & jobs -p %1; kill -KILL $! 2>/dev/null"#);
    assert!(
        pids.stdout.trim().parse::<u32>().is_ok(),
        "jobs -p %1 printed more than a pid: {}",
        pids.stdout
    );
}

#[test]
fn the_current_job_spec_resolves() {
    let out = cash(r#"ping.exe -n 10 127.0.0.1 > /dev/null & jobs %+; kill -KILL $! 2>/dev/null"#);
    assert!(
        out.stdout.starts_with("[1]"),
        "%+ did not resolve: {}",
        out.stdout
    );
}

#[test]
fn an_unknown_job_spec_is_reported() {
    let out = cash(r#"jobs %9; echo "rc=$?""#);
    assert!(out.stdout.contains("rc=1"), "no failure: {}", out.stdout);
    assert!(
        out.stderr.contains("no such job"),
        "no diagnostic: {}",
        out.stderr
    );
}

#[test]
fn jobs_l_with_no_jobs_prints_nothing() {
    let out = cash("jobs -l; echo done");
    assert_eq!(out.stdout, "done", "jobs -l invented a job: {}", out.stdout);
}

// ---------------------------------------------------------------------------
// Bundled userland: tty, logname, hostid, users, who, pinky, stat, pathchk
// ---------------------------------------------------------------------------

#[test]
fn tty_detects_pipe_as_not_a_tty() {
    let out = cash("echo hello | tty");
    assert_eq!(out.stdout, "not a tty");

    let silent = cash("echo hello | tty -s; echo rc=$?");
    assert_eq!(silent.stdout, "rc=1");
}

#[test]
fn logname_matches_current_user() {
    let out = cash("logname");
    assert!(!out.stdout.is_empty(), "logname produced no output");
    let user = std::env::var("USERNAME").unwrap_or_default();
    if !user.is_empty() {
        assert_eq!(out.stdout, user);
    }
}

#[test]
fn hostid_prints_eight_hex_digits() {
    let out = cash("hostid");
    assert_eq!(
        out.stdout.len(),
        8,
        "hostid should be 8 hex characters: {}",
        out.stdout
    );
    assert!(
        out.stdout.chars().all(|c| c.is_ascii_hexdigit()),
        "hostid not hex: {}",
        out.stdout
    );
}

#[test]
fn users_lists_logged_in_users() {
    let out = cash("users");
    assert!(!out.stdout.is_empty(), "users produced no output");
    let user = std::env::var("USERNAME").unwrap_or_default();
    if !user.is_empty() {
        assert!(
            out.stdout.contains(&user),
            "users missing current user: {}",
            out.stdout
        );
    }
}

#[test]
fn who_displays_session_info() {
    let out = cash("who");
    assert!(!out.stdout.is_empty(), "who produced no output");

    let count = cash("who -q");
    assert!(
        count.stdout.contains("# users="),
        "who -q missing count: {}",
        count.stdout
    );

    let whoami = cash("who am i");
    assert!(!whoami.stdout.is_empty(), "who am i produced no output");
}

#[test]
fn pinky_displays_user_info() {
    let out = cash("pinky");
    assert!(
        out.stdout.contains("Login"),
        "pinky header missing: {}",
        out.stdout
    );
}

#[test]
fn stat_reports_file_attributes() {
    let size = cash("stat -c %s Cargo.toml");
    let size_num: u64 = size.stdout.parse().expect("stat %s should be numeric");
    assert!(size_num > 0, "stat reported size 0 for Cargo.toml");

    let ftype = cash("stat -c %F Cargo.toml");
    assert_eq!(ftype.stdout, "regular file");

    let octal = cash("stat -c %a Cargo.toml");
    assert_eq!(octal.stdout, "644");
}

#[test]
fn stat_finds_a_relative_path_in_the_shells_folder() {
    // `stat` asked the process's working folder, which `cd` does not change: after
    // `cd tests`, `stat it/main.rs` found nothing (2026-10-02, TODO 4.7).
    let out = cash("cd tests && stat -c %F it/main.rs");
    assert_eq!(out.stdout, "regular file", "{}", out.stderr);
}

#[test]
fn pathchk_validates_path_portability() {
    let out = cash("pathchk Cargo.toml; echo rc=$?");
    assert_eq!(out.stdout, "rc=0");
}

#[test]
fn install_creates_directories_and_copies_files() {
    let out = cash(
        "tmp_dir=$(mktemp -d); \
         install -d \"$tmp_dir/sub/dir\"; \
         echo 'sample content' > \"$tmp_dir/sample.txt\"; \
         install -m 644 \"$tmp_dir/sample.txt\" \"$tmp_dir/sub/dir/copied.txt\"; \
         cat \"$tmp_dir/sub/dir/copied.txt\"; \
         rm -rf \"$tmp_dir\"",
    );
    assert_eq!(out.stdout, "sample content");
}
