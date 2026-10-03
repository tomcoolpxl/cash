//! What a script sees of its environment — **D3**, **D5**, and the here-document deadlock.
//!
//! D5 is the decision cash had written down and not implemented: `$PATH` is rendered
//! Unix-style to scripts, colon-separated with `/c/...`, and converted back to the
//! semicolon-separated Windows form only in the environment block handed to a child. It
//! rendered the raw Windows value instead, so both idioms D5 names as the reason —
//! `IFS=: read -ra dirs <<< "$PATH"` and `PATH=/foo:$PATH` — produced nonsense.
//!
//! Testing the first of those is what found the deadlock. A here-string is written into
//! a pipe before the command that reads it starts, so above the pipe buffer the write
//! blocks forever. On Windows that buffer is 4096 bytes; `$PATH` here is 4425. Measured:
//! 4090 bytes worked, 4096 hung the shell outright. Linux escapes it with
//! `F_SETPIPE_SZ`, which has no Windows equivalent.

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

use crate::common::{cash_command, run as cash};

// ---------------------------------------------------------------------------
// Here-documents above the pipe buffer (the deadlock)
// ---------------------------------------------------------------------------

#[test]
fn a_here_string_larger_than_the_pipe_buffer_does_not_hang() {
    // 4096 is the exact size that hung. If this regresses the test does not fail with a
    // message — it never returns, which is why the sizes are named rather than random.
    for size in [4095_usize, 4096, 8192, 65_536] {
        let out = cash(&format!(
            r#"v=$(printf '%{size}s' '' | tr ' ' 'x'); read -r line <<< "$v"; echo "${{#line}}""#
        ));
        assert_eq!(
            out.stdout,
            size.to_string(),
            "a here-string of {size} bytes did not round-trip"
        );
    }
}

#[test]
fn a_here_document_larger_than_the_pipe_buffer_does_not_hang() {
    let out = cash(
        r#"
        d=$(mktemp -d)
        i=0
        while [ "$i" -lt 2000 ]; do echo "line $i"; i=$((i+1)); done > "$d/src"
        cat <<EOF | wc -l | tr -d ' '
$(cat "$d/src")
EOF
        cd /; rm -rf "$d"
        "#,
    );
    assert_eq!(out.stdout, "2000", "stderr: {}", out.stderr);
}

#[test]
fn a_large_here_string_survives_its_exact_bytes() {
    // Not just the length: a temp-file path must not lose or reorder content.
    let out = cash(
        r#"v=$(printf 'ab%.0s' $(seq 1 3000)); read -r line <<< "$v"; echo "${#line} ${line:0:4} ${line: -4}""#,
    );
    assert_eq!(out.stdout, "6000 abab abab", "stderr: {}", out.stderr);
}

#[test]
fn here_documents_do_not_leak_temp_files() {
    // cash writes a here-document's file under TMP/TEMP, and so does every other test
    // here that uses one. nextest runs them all at once, so counting the shared temp
    // directory raced with them (`left: 2, right: 1`). A private one is this test's own.
    let tmp = std::env::temp_dir().join(format!("cash-leak-check-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let run = |script: &str| {
        let out = cash_command()
            .env("TMP", &tmp)
            .env("TEMP", &tmp)
            .args(["-c", script])
            .output()
            .expect("failed to run cash");
        String::from_utf8_lossy(&out.stdout).trim_end().to_string()
    };
    let left = || -> Vec<String> {
        std::fs::read_dir(&tmp)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("cash-here-"))
            .collect()
    };

    // The file is listed while the command reading it runs. Without this, a cash that
    // stopped honouring TMP would pass by leaking its files somewhere else.
    let listed = run(r#"v=$(printf '%9000s' '' | tr ' ' 'x'); ls "$TEMP" <<< "$v""#);
    assert!(
        listed.contains("cash-here-"),
        "the here-document was not written to TMP: {listed:?}"
    );

    for _ in 0..5 {
        let out = run(r#"v=$(printf '%9000s' '' | tr ' ' 'x'); read -r l <<< "$v"; echo "${#l}""#);
        assert_eq!(out, "9000");
    }
    assert_eq!(
        left(),
        Vec::<String>::new(),
        "a here-document temp file was left behind"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn a_here_string_still_works_for_small_input() {
    // The fix must not break the ordinary case it was never about.
    let out = cash(r#"read -r a b <<< "one two"; echo "[$a][$b]""#);
    assert_eq!(out.stdout, "[one][two]");
}

// ---------------------------------------------------------------------------
// `$PATH` is Unix-style to scripts (D5)
// ---------------------------------------------------------------------------

#[test]
fn path_is_colon_separated_and_unix_spelled() {
    let out = cash(r#"echo "$PATH""#);
    assert!(
        out.stdout.contains(':'),
        "PATH is not colon-separated: {}",
        out.stdout
    );
    assert!(
        !out.stdout.contains(';'),
        "PATH still has semicolons: {}",
        out.stdout
    );
    assert!(
        !out.stdout.contains('\\'),
        "PATH still has backslashes: {}",
        out.stdout
    );
    assert!(
        out.stdout.starts_with('/'),
        "PATH is not Unix-spelled: {}",
        out.stdout
    );
}

#[test]
fn d5s_first_worked_example_runs() {
    // `IFS=: read -ra dirs <<< "$PATH"` is the reason D5 gives for the Unix form.
    let out = cash(r#"IFS=: read -ra dirs <<< "$PATH"; echo "${#dirs[@]}"; echo "${dirs[0]}""#);
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "unexpected output: {} {}",
        out.stdout,
        out.stderr
    );
    assert!(
        lines[0].parse::<usize>().is_ok_and(|n| n > 1),
        "no entries parsed: {}",
        lines[0]
    );
    assert!(
        lines[1].starts_with('/'),
        "first entry is not Unix-spelled: {}",
        lines[1]
    );
}

#[test]
fn d5s_second_worked_example_runs() {
    // `PATH=/foo:$PATH` has to compose, and commands must still resolve afterwards.
    let out =
        cash(r#"PATH=/c/nonexistent:$PATH; echo "${PATH%%:*}"; cmd.exe /d /s /c "echo child-ran""#);
    assert_eq!(
        out.stdout, "/c/nonexistent\nchild-ran",
        "prepending to PATH broke resolution: {}",
        out.stderr
    );
}

#[test]
fn a_child_process_receives_the_windows_form() {
    // D5's third clause: the conversion back happens at the process boundary, because
    // `git.exe` and `terraform.exe` cannot read the Unix form.
    let out = cash(r#"cmd.exe /d /s /c "echo %PATH%""#);
    assert!(
        out.stdout.contains(';'),
        "the child got no semicolons: {}",
        out.stdout
    );
    assert!(
        out.stdout.contains(":\\") || out.stdout.contains(":/"),
        "the child got no drive-letter paths: {}",
        out.stdout
    );
}

#[test]
fn a_path_entry_containing_spaces_survives_the_round_trip() {
    // `C:\Program Files\...` is on every Windows PATH, and a naive split would break it.
    let out = cash(r#"echo "$PATH" | tr ':' '\n' | grep -c ' '"#);
    let with_spaces: usize = out.stdout.trim().parse().unwrap_or(0);
    assert!(
        with_spaces > 0,
        "no PATH entry with a space survived: {}",
        out.stdout
    );
}

#[test]
fn a_windows_spelled_prepend_still_works() {
    // Somebody will write `PATH="C:/tools;$PATH"`, and splitting must cope with both.
    let out = cash(r#"PATH="C:/tools:$PATH"; cmd.exe /d /s /c "echo still-ran""#);
    assert_eq!(out.stdout, "still-ran", "stderr: {}", out.stderr);
}

#[test]
fn path_lookup_is_unaffected_by_the_rendering() {
    let out = cash("command -v cmd.exe > /dev/null && echo found");
    assert_eq!(out.stdout, "found", "stderr: {}", out.stderr);
}

// ---------------------------------------------------------------------------
// `$SHELL` and `$0` name cash (D3)
// ---------------------------------------------------------------------------

#[test]
fn shell_names_cash_rather_than_whatever_launched_it() {
    // Inherited, it pointed at Git Bash. `make`, `npm run` and an editor's integrated
    // terminal all read it to decide what to launch.
    let out = cash(r#"echo "$SHELL""#);
    assert!(
        out.stdout.to_lowercase().contains("cash.exe"),
        "$SHELL does not name cash: {}",
        out.stdout
    );
    assert!(
        !out.stdout.contains('\\'),
        "D3: backslashes in $SHELL: {}",
        out.stdout
    );
}

#[test]
fn argv_zero_is_rendered_canonically() {
    let out = cash(r#"echo "$0""#);
    assert!(
        !out.stdout.contains('\\'),
        "D3: backslashes in $0: {}",
        out.stdout
    );
}

#[test]
fn shell_and_argv_zero_agree() {
    let out = cash(r#"[ "$SHELL" = "$0" ] && echo agree || echo "differ: [$SHELL] [$0]""#);
    assert_eq!(out.stdout, "agree");
}

// ---------------------------------------------------------------------------
// `id` and `groups` report Windows identity (§4 #19)
// ---------------------------------------------------------------------------

#[test]
fn id_and_groups_are_builtins() {
    for name in ["id", "groups"] {
        let out = cash(&format!("type {name}"));
        assert!(
            out.stdout.contains("shell builtin"),
            "{name} is not the builtin: {}",
            out.stdout
        );
    }
}

#[test]
fn id_reports_a_real_windows_sid() {
    // MSYS reported uid 197609 — a number Windows does not have and `groups` could not
    // map back to a name.
    let out = cash("id");
    assert!(
        out.stdout.contains("sid=S-1-"),
        "no SID reported: {}",
        out.stdout
    );
    assert!(out.stdout.contains("uid="), "no uid field: {}", out.stdout);
    assert_eq!(out.code, 0);
}

#[test]
fn id_u_is_numeric_and_id_un_is_a_name() {
    let numeric = cash("id -u");
    assert!(
        numeric.stdout.parse::<u64>().is_ok(),
        "id -u is not a number: {}",
        numeric.stdout
    );

    let named = cash("id -un");
    assert!(!named.stdout.is_empty(), "id -un printed nothing");
    assert!(
        named.stdout.parse::<u64>().is_err(),
        "id -un printed a number: {}",
        named.stdout
    );
}

#[test]
fn groups_names_an_account_rather_than_failing() {
    // The MSYS one printed `cannot find name for group ID 197609` and exited non-zero.
    let out = cash("groups");
    assert_eq!(out.code, 0, "groups failed: {}", out.stderr);
    assert!(!out.stdout.is_empty(), "groups printed nothing");
    assert!(
        !out.stdout.contains("cannot find name"),
        "groups could not resolve its own answer: {}",
        out.stdout
    );
}

#[test]
fn hostname_uname_and_computername_all_agree() {
    let out = cash(
        r#"
        echo "hn=$(hostname)"
        echo "un=$(uname -n)"
        echo "cn=$COMPUTERNAME"
        echo "env_hn=$HOSTNAME"
    "#,
    );
    let hn = out
        .stdout
        .lines()
        .find_map(|l| l.strip_prefix("hn="))
        .unwrap_or_default();
    let un = out
        .stdout
        .lines()
        .find_map(|l| l.strip_prefix("un="))
        .unwrap_or_default();
    let cn = out
        .stdout
        .lines()
        .find_map(|l| l.strip_prefix("cn="))
        .unwrap_or_default();
    let env_hn = out
        .stdout
        .lines()
        .find_map(|l| l.strip_prefix("env_hn="))
        .unwrap_or_default();

    assert!(!cn.is_empty(), "COMPUTERNAME is empty");
    assert_eq!(hn, cn, "hostname [{hn}] != COMPUTERNAME [{cn}]");
    assert_eq!(un, cn, "uname -n [{un}] != COMPUTERNAME [{cn}]");
    assert_eq!(env_hn, cn, "$HOSTNAME [{env_hn}] != COMPUTERNAME [{cn}]");
}

#[test]
fn logout_builtin_requires_login_shell() {
    let non_login = cash("logout");
    assert_eq!(non_login.code, 1);
    assert!(
        non_login.stderr.contains("not login shell"),
        "unexpected stderr: {}",
        non_login.stderr
    );

    // A login shell, without the runner's own ~/.profile, whose last status logout would
    // otherwise exit with.
    let login_out = cash_command()
        .args(["--login", "--noprofile", "-c", "logout"])
        .output()
        .expect("run cash");
    assert_eq!(login_out.status.code(), Some(0));
}

#[test]
fn type_and_which_do_not_produce_mixed_slashes() {
    let out = cash("type -a which; type -p which");
    for line in out.stdout.lines() {
        if line.contains(':') && line.contains('/') {
            assert!(
                !line.contains('\\'),
                "found mixed slash in type output: [{line}]"
            );
        }
    }
}
