//! What cash says it will run, and what it actually runs — **D3**, **D7**, **D8**, **D34**.
//!
//! An audit of command resolution found three tools reporting someone else's answer.
//! None of them was missing; each was confidently wrong, which is the harder failure to
//! notice:
//!
//! - `which cat` said `/usr/bin/cat` while cash ran its own builtin.
//! - `cash doctor` reported `PATH` findings for commands cash never resolves through
//!   `PATH`, and on a machine without Git for Windows told the user to install coreutils
//!   they already had.
//! - `bash` resolved to `C:\WINDOWS\system32\bash.exe` — the WSL launcher — so
//!   `bash helper.sh` could silently continue under Linux.
//!
//! The same audit found `pwd` printing backslashes before the first `cd`, which is §4's
//! very first divergence row promising the opposite.

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

const CASH: &str = env!("CARGO_BIN_EXE_cash");

/// A `PATH` with nothing but Windows on it, which is what a machine without Git for
/// Windows looks like.
const BARE_PATH: &str = r"C:\WINDOWS\system32;C:\WINDOWS";

struct Output {
    stdout: String,
    stderr: String,
    code: i32,
}

fn cash(script: &str) -> Output {
    run(Command::new(CASH).args(["-c", script]))
}

/// The same, on a machine that has only Windows installed.
fn cash_bare(script: &str) -> Output {
    run(Command::new(CASH)
        .args(["-c", script])
        .env("PATH", BARE_PATH))
}

fn run(command: &mut Command) -> Output {
    let out = command.output().expect("failed to run cash");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).trim_end().to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

// ---------------------------------------------------------------------------
// `which` answers about cash (D8)
// ---------------------------------------------------------------------------

#[test]
fn which_reports_the_builtin_the_shell_would_run() {
    // Not `/usr/bin/cat`. The question is what *this shell* runs. A command cash carries
    // gets the path that runs it — cash's own executable with the name appended — and
    // bash's own builtins, which are no program anywhere, say so.
    let own = CASH.replace('\\', "/");
    for name in ["cat", "ps", "less", "which"] {
        let out = cash(&format!("which {name}"));
        assert!(
            out.stdout.eq_ignore_ascii_case(&format!("{own}/{name}")),
            "which {name} did not report cash's own: {}",
            out.stdout
        );
        assert_eq!(out.code, 0);
    }
    let out = cash("which kill");
    assert_eq!(out.stdout, "kill: shell builtin");
}

#[test]
fn the_path_which_prints_can_be_run() {
    // `LS=$(which ls); "$LS" -la` is how scripts capture a tool; the path is no file, so
    // cash runs the command it names, by name and through any process-spawning route.
    let out = cash(
        r#"LS=$(which ls); [ -x "$LS" ] && echo executable
           "$LS" -d /c/Windows
           ls() { echo 'a function'; }; "$LS" -d /c/Windows
           (exec "$(which rev)" <<< olleh)
           echo /c/abc | xargs "$(which basename)"
           find /c/Windows -maxdepth 0 -exec "$(which basename)" {} \;"#,
    );
    assert_eq!(
        out.stdout, "executable\n/c/Windows\n/c/Windows\nhello\nabc\nWindows",
        "{}",
        out.stderr
    );
}

#[test]
fn which_agrees_with_type() {
    // Two tools answering the same question must not disagree. This is the invariant the
    // MSYS `which` broke.
    for name in ["cat", "ps", "git", "cmd"] {
        let which = cash(&format!("which {name} > /dev/null 2>&1; echo $?"));
        let typed = cash(&format!("type {name} > /dev/null 2>&1; echo $?"));
        assert_eq!(
            which.stdout, typed.stdout,
            "which and type disagree on whether {name} exists"
        );
    }
}

#[test]
fn which_prints_a_bare_path_for_an_external() {
    // `p=$(which git)` has to keep working, so an external gets a path and nothing else.
    let out = cash("which cmd");
    assert!(
        out.stdout.to_lowercase().ends_with(".exe"),
        "not a bare path: {}",
        out.stdout
    );
    assert!(
        !out.stdout.contains(' ') || out.stdout.contains(":/"),
        "not a path: {}",
        out.stdout
    );
    assert!(
        !out.stdout.contains('\\'),
        "D3: backslashes in a rendered path: {}",
        out.stdout
    );
}

#[test]
fn type_renders_a_path_the_same_way_which_does() {
    // `type` printed the path as the resolver had assembled it, so an already-rendered
    // PATH directory was joined to the file name with a backslash:
    //
    //     ls is C:/Program Files/Git/usr/bin\ls.exe
    //
    // Two commands answering about the same file must not spell it two ways (D3).
    let typed = cash("type cmd").stdout;
    assert!(
        !typed.contains('\\'),
        "D3: backslashes in a rendered path: {typed}"
    );

    let from_which = cash("which cmd").stdout;
    assert!(
        typed.ends_with(&from_which),
        "type and which disagree: [{typed}] [{from_which}]"
    );
}

#[test]
fn type_a_renders_every_candidate() {
    // `-a` is where the mixed separator showed up, because it is the spelling that prints
    // the PATH hits rather than stopping at the builtin.
    let out = cash("type -a cmd");
    assert!(
        !out.stdout.contains('\\'),
        "D3: backslashes in a rendered path: {}",
        out.stdout
    );
}

#[test]
fn which_p_restricts_the_search_to_path() {
    // The escape hatch for a script that genuinely wants a file, not an answer.
    let out = cash("which -p cat");
    assert!(
        out.stdout.is_empty() || out.stdout.contains('/'),
        "-p returned something that is not a path: {}",
        out.stdout
    );
    assert!(
        !out.stdout.contains("builtin"),
        "-p reported a builtin: {}",
        out.stdout
    );
}

#[test]
fn which_reports_functions_and_missing_names() {
    let function = cash("f() { :; }; which f");
    assert_eq!(function.stdout, "f: shell function");

    let missing = cash("which definitely-not-a-command; echo rc=$?");
    assert!(
        missing.stdout.contains("rc=1"),
        "missing name did not fail: {}",
        missing.stdout
    );
    assert!(
        missing.stderr.contains("not found"),
        "no diagnostic: {}",
        missing.stderr
    );
}

#[test]
fn which_s_is_silent() {
    let out = cash("which -s cat; echo rc=$?");
    assert_eq!(out.stdout, "rc=0");
}

// ---------------------------------------------------------------------------
// `sh` and `bash` are cash (D7)
// ---------------------------------------------------------------------------

#[test]
fn sh_and_bash_resolve_to_cash() {
    // On a bare Windows PATH the alternative for `bash` is the WSL launcher, and for
    // `sh` there is no alternative at all.
    for name in ["sh", "bash"] {
        let out = cash_bare(&format!("which {name}"));
        assert!(
            out.stdout.to_lowercase().contains("cash.exe"),
            "{name} did not resolve to cash: {}",
            out.stdout
        );
        assert!(
            !out.stdout.to_lowercase().contains("system32"),
            "{name} resolved into System32 — that is the WSL launcher: {}",
            out.stdout
        );
    }
}

#[test]
fn cash_resolves_to_the_running_cash() {
    // A terminal profile starts cash by full path, so its folder is usually not on PATH;
    // `cash doctor` must work anyway.
    let out = cash_bare("which cash; cash -c 'echo nested-cash-ran'");
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert!(
        lines
            .first()
            .is_some_and(|line| line.to_lowercase().contains("cash.exe")),
        "cash did not resolve to itself: {} {}",
        out.stdout,
        out.stderr
    );
    assert_eq!(lines.get(1), Some(&"nested-cash-ran"), "{}", out.stderr);
}

#[test]
fn a_nested_shell_is_cash_and_keeps_cash_semantics() {
    // The point of the rule: the path guarantees do not stop at the `sh -c` boundary.
    let out = cash_bare(r#"sh -c 'pwd; echo $BASH_VERSION'"#);
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert!(
        !lines.is_empty(),
        "nested shell produced nothing: {}",
        out.stderr
    );
    assert!(
        !lines[0].contains('\\'),
        "D3: a nested shell printed backslashes: {}",
        lines[0]
    );
    assert!(
        lines.get(1).is_some_and(|v| v.starts_with('5')),
        "nested shell was not cash: {lines:?}"
    );
}

#[test]
fn a_bin_sh_shebang_has_an_interpreter() {
    let dir = std::env::temp_dir().join("cash-shebang-sh");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create dir");
    let script = dir.join("s.sh");
    std::fs::write(&script, b"#!/bin/sh\necho shebang-ran\n").expect("write");

    let out = run(Command::new(CASH)
        .arg(script.to_string_lossy().replace('\\', "/"))
        .env("PATH", BARE_PATH));

    assert_eq!(out.stdout, "shebang-ran", "stderr: {}", out.stderr);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn another_shell_is_not_impersonated() {
    // `sh` and `bash` only. cash does not pretend to be zsh, dash or pwsh.
    let out = cash_bare("which zsh; echo rc=$?");
    assert!(
        out.stdout.contains("rc=1"),
        "cash claimed to be zsh: {}",
        out.stdout
    );
}

// ---------------------------------------------------------------------------
// `pwd` is canonical from the first line (D3)
// ---------------------------------------------------------------------------

#[test]
fn pwd_is_canonical_before_any_cd() {
    // The starting directory is inherited from the OS, which reports backslashes; `cd`
    // canonicalised, so the bug only showed on the very first `pwd` of a session.
    let out = cash("pwd");
    assert!(
        !out.stdout.contains('\\'),
        "pwd printed backslashes: {}",
        out.stdout
    );
    assert!(
        out.stdout.contains(":/"),
        "not a rendered drive path: {}",
        out.stdout
    );
}

#[test]
fn the_pwd_variable_agrees_with_the_builtin() {
    let out = cash(r#"[ "$PWD" = "$(pwd)" ] && echo agree || echo "differ: [$PWD] [$(pwd)]""#);
    assert_eq!(out.stdout, "agree");
}

#[test]
fn a_path_composed_from_pwd_is_usable() {
    // D3's own worked example: `terraform -chdir="$(pwd)/modules"`.
    let out = cash(r#"echo "-chdir=$(pwd)/modules""#);
    assert!(
        !out.stdout.contains('\\'),
        "composed argument has backslashes: {}",
        out.stdout
    );
    assert!(
        out.stdout.starts_with("-chdir="),
        "unexpected: {}",
        out.stdout
    );
}

#[test]
fn a_resolved_relative_path_has_one_kind_of_separator() {
    // `working_dir().join(name)` inserts a backslash, which leaked into every diagnostic
    // naming a resolved path.
    let out =
        cash(r#"d=$(mktemp -d); cd "$d"; cat no-such-file 2>&1 | head -1; cd /; rm -rf "$d""#);
    assert!(
        !out.stdout.contains('\\'),
        "a diagnostic mixed separators: {}",
        out.stdout
    );
}

// ---------------------------------------------------------------------------
// `chmod` does what Windows can, and says what it cannot (D23, D34)
// ---------------------------------------------------------------------------

#[test]
fn chmod_is_a_builtin() {
    let out = cash("type chmod");
    assert!(
        out.stdout.contains("shell builtin"),
        "chmod is not the builtin: {}",
        out.stdout
    );
}

#[test]
fn chmod_minus_w_really_makes_a_file_read_only() {
    // The half Windows genuinely has. MSYS's chmod wrote a mode bit nothing outside MSYS
    // could see; this one sets the attribute every Windows program honours.
    let out = cash(
        r#"d=$(mktemp -d); cd "$d"; : > f; chmod -w f; (echo x > f) 2>/dev/null && echo wrote || echo refused; chmod +w f; cd /; rm -rf "$d""#,
    );
    assert_eq!(
        out.stdout, "refused",
        "read-only was not enforced: {}",
        out.stderr
    );
}

#[test]
fn chmod_plus_w_restores_writability() {
    let out = cash(
        r#"d=$(mktemp -d); cd "$d"; : > f; chmod -w f; chmod +w f; echo x > f && echo wrote; cd /; rm -rf "$d""#,
    );
    assert_eq!(out.stdout, "wrote", "stderr: {}", out.stderr);
}

#[test]
fn chmod_plus_x_is_silent() {
    // D23: executability comes from the extension or a shebang, so there is nothing to
    // set and nothing lost; it said "execute: not represented" (the user, 2026-10-02).
    let out = cash(
        r#"d=$(mktemp -d); cd "$d"; : > f; chmod +x f; chmod 755 f; echo "rc=$?"; cd /; rm -rf "$d""#,
    );
    assert_eq!(out.stdout, "rc=0");
    assert!(out.stderr.is_empty(), "{}", out.stderr);
}

#[test]
fn chmod_minus_x_warns_rather_than_lying() {
    // D34: revoking execute needs a Deny ACE, so it is not done, and said.
    let out =
        cash(r#"d=$(mktemp -d); cd "$d"; : > f; chmod -x f; echo "rc=$?"; cd /; rm -rf "$d""#);
    assert_eq!(out.stdout, "rc=0", "D34 says return 0");
    assert!(
        out.stderr.contains("execute") && out.stderr.contains("Windows"),
        "no honest diagnostic: {}",
        out.stderr
    );
}

#[test]
fn group_and_other_bits_leave_the_owner_alone() {
    // `chmod go-w f` made `f` read-only for its owner, and `u+rw,go-w` was an invalid
    // mode (BI-05). Group and other have no per-file bits on Windows: silent (the user,
    // 2026-10-03).
    let out = cash(
        r#"d=$(mktemp -d); cd "$d"; : > f; chmod go-w f; echo x > f && echo wrote; chmod u-w,o+r f; (echo x > f) 2>/dev/null || echo refused; chmod u+rw,go-w f; echo x > f && echo wrote-again; cd /; rm -rf "$d""#,
    );
    assert_eq!(
        out.stdout,
        "wrote
refused
wrote-again"
    );
    assert!(out.stderr.is_empty(), "{}", out.stderr);
}

#[test]
fn chmod_is_quiet_under_dash_f() {
    let out =
        cash(r#"d=$(mktemp -d); cd "$d"; : > f; chmod -f -x f; echo "rc=$?"; cd /; rm -rf "$d""#);
    assert_eq!(out.stdout, "rc=0");
    assert!(out.stderr.is_empty(), "-f was not silent: {}", out.stderr);
}

#[test]
fn a_numeric_mode_applies_its_write_bit() {
    let out = cash(
        r#"d=$(mktemp -d); cd "$d"; : > f; chmod 444 f; (echo x > f) 2>/dev/null && echo wrote || echo refused; chmod 644 f; echo x > f && echo wrote-after; cd /; rm -rf "$d""#,
    );
    assert_eq!(out.stdout, "refused\nwrote-after", "stderr: {}", out.stderr);
}

#[test]
fn chmod_reports_a_missing_file_and_an_invalid_mode() {
    let missing = cash("chmod +w /definitely/not/here; echo rc=$?");
    assert!(
        missing.stdout.contains("rc=1"),
        "missing file did not fail: {}",
        missing.stdout
    );

    let invalid = cash("chmod zzz /tmp; echo rc=$?");
    assert!(
        invalid.stdout.contains("rc=1"),
        "invalid mode did not fail: {}",
        invalid.stdout
    );
    assert!(
        invalid.stderr.contains("invalid mode"),
        "no diagnostic: {}",
        invalid.stderr
    );

    let none = cash("chmod; echo rc=$?");
    assert!(
        !none.stdout.contains("rc=0"),
        "no operands was accepted: {}",
        none.stdout
    );
}

#[test]
fn chmod_r_reaches_into_a_directory() {
    let out = cash(
        r#"d=$(mktemp -d); mkdir -p "$d/sub"; : > "$d/sub/f"; chmod -R -w "$d"; (echo x > "$d/sub/f") 2>/dev/null && echo wrote || echo refused; chmod -R +w "$d"; cd /; rm -rf "$d""#,
    );
    assert_eq!(out.stdout, "refused", "stderr: {}", out.stderr);
}

// ---------------------------------------------------------------------------
// `cash doctor` reports cash's answer (D35)
// ---------------------------------------------------------------------------

#[test]
fn doctor_does_not_advise_installing_a_builtin() {
    // The bug: on a bare PATH it said `WARN cat not found -> winget install ...` for a
    // working builtin.
    let out = run(Command::new(CASH).arg("doctor").env("PATH", BARE_PATH));
    for builtin in [
        "cat", "mktemp", "cut", "tr", "head", "tail", "wc", "awk", "sed", "stat", "tty", "nohup",
        "who", "users", "pinky", "logname", "hostid", "pathchk", "install", "dos2unix", "unix2dos",
        "fuser", "lsof", "ss",
    ] {
        assert!(
            !out.stdout.contains(&format!("WARN  {builtin}")),
            "doctor warned about the builtin {builtin}:\n{}",
            out.stdout
        );
    }
}

#[test]
fn doctor_confirms_the_shells_resolve_to_cash() {
    let out = run(Command::new(CASH).arg("doctor").env("PATH", BARE_PATH));
    assert!(
        out.stdout.contains("resolves to cash"),
        "doctor did not report the shell rule:\n{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("WSL launcher"),
        "doctor did not name what the rule takes precedence over:\n{}",
        out.stdout
    );
}

#[test]
fn doctor_still_names_what_is_genuinely_missing() {
    // The fix must not turn the diagnostic into a rubber stamp.
    let out = run(Command::new(CASH).arg("doctor").env("PATH", BARE_PATH));
    for absent in ["grep", "diff"] {
        assert!(
            out.stdout.contains(&format!("WARN  {absent}")),
            "doctor did not report {absent} as missing:\n{}",
            out.stdout
        );
    }
    assert_ne!(
        out.code, 0,
        "doctor reported success on a machine with no userland"
    );
}

#[test]
fn doctor_does_not_warn_about_a_dos_tool_a_builtin_shadows() {
    // System32's `more.com` cannot shadow cash's `more`: PATH is never consulted for a
    // builtin. Warning about it was exactly backwards.
    let out = run(Command::new(CASH).arg("doctor"));
    assert!(
        !out.stdout.contains("more (shadowing)"),
        "doctor warned that a builtin was shadowed:\n{}",
        out.stdout
    );
}

#[test]
fn doctor_is_clean_on_a_fully_equipped_machine() {
    let out = run(Command::new(CASH).arg("doctor"));
    if out.stdout.contains("WARN") {
        // Not a failure: this machine may genuinely be missing something. But the
        // warnings must be about real externals, never about builtins.
        for line in out.stdout.lines().filter(|l| l.contains("WARN")) {
            assert!(
                ![
                    "cat", "cut", "tr", "head", "tail", "wc", "mktemp", "ps", "less"
                ]
                .iter()
                .any(|b| line.contains(&format!("WARN  {b} "))),
                "a builtin was reported as a problem: {line}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// The old name is gone (D9)
// ---------------------------------------------------------------------------

#[test]
fn the_control_builtin_carries_cashs_name() {
    let out = cash("type cashctl; type cashinfo");
    assert_eq!(
        out.stdout.lines().count(),
        2,
        "cashctl/cashinfo missing: {} {}",
        out.stdout,
        out.stderr
    );

    let old = cash("type brushctl 2>/dev/null; echo rc=$?");
    assert!(
        old.stdout.contains("rc=1"),
        "the brush name is still registered: {}",
        old.stdout
    );
}

#[test]
fn command_v_renders_a_path_found_on_path_with_forward_slashes() {
    // `type` already rendered; `command -v` printed `C:/WINDOWS/system32\netstat.exe`.
    let out = cash("command -v netstat; type -P netstat");
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.len(), 2, "{} {}", out.stdout, out.stderr);
    assert!(!lines[0].contains('\\'), "{}", lines[0]);
    assert!(
        lines[0].to_ascii_lowercase().ends_with("/netstat.exe"),
        "{}",
        lines[0]
    );
    assert_eq!(lines[0], lines[1]);
}

#[test]
fn hash_t_renders_a_path_the_same_way_command_v_does() {
    // `hash -t` printed the cached path as the resolver had joined it:
    //
    //     C:/Program Files/Git/usr/bin\ls.exe
    let out = cash("hash netstat; hash -t netstat; command -v netstat");
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.len(), 2, "{} {}", out.stdout, out.stderr);
    assert!(
        !lines[0].contains('\\'),
        "D3: backslashes in a rendered path: {}",
        lines[0]
    );
    assert_eq!(lines[0], lines[1], "hash -t and command -v disagree");
}

#[test]
fn hash_t_renders_every_name_it_lists() {
    // With several names each line is `name<TAB>path`.
    let out = cash("hash netstat cmd; hash -t netstat cmd");
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.len(), 2, "{} {}", out.stdout, out.stderr);
    for (line, name) in lines.iter().zip(["netstat", "cmd"]) {
        assert!(line.starts_with(&format!("{name}\t")), "{line}");
        assert!(!line.contains('\\'), "D3: backslashes: {line}");
    }
}

#[test]
fn hash_lt_renders_but_does_not_quote() {
    // Bash prints `-lt` verbatim; only the whole-table `-l` listing is quoted.
    let out = cash("hash netstat; hash -lt netstat");
    assert!(
        out.stdout.starts_with("builtin hash -p ") && out.stdout.ends_with(" netstat"),
        "{} {}",
        out.stdout,
        out.stderr
    );
    assert!(
        !out.stdout.contains('\\'),
        "D3: backslashes: {}",
        out.stdout
    );
    assert!(!out.stdout.contains('\''), "quoted: {}", out.stdout);
}

#[test]
fn hash_l_lists_the_table_as_input_that_round_trips() {
    // "Usable for input" means feeding it back reproduces the table, space in the path and
    // all.
    let out = cash(
        r#"hash -p "C:\Program Files\x.exe" x; hash netstat; l=$(hash -l); echo "$l"; hash -r; eval "$l"; hash -t x"#,
    );
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.len(), 3, "{} {}", out.stdout, out.stderr);
    assert!(
        lines[..2].contains(&"builtin hash -p 'C:/Program Files/x.exe' x"),
        "{lines:?}"
    );
    assert!(
        lines[..2].iter().all(|l| !l.contains('\\')),
        "D3: backslashes: {lines:?}"
    );
    assert_eq!(lines[2], "C:/Program Files/x.exe", "did not round-trip");
}

#[test]
fn type_a_does_not_find_a_name_that_is_only_hashed() {
    // `-a` skips the hash table, as in bash, so a hashed entry for a file that has gone is
    // not found -- rather than silently succeeding with no output.
    let out = cash(
        r#"hash -p 'C:\no\such.exe' gone; type -a gone; echo "rc=$?"; type -a -t gone; echo "rc=$?"; type -a -P gone; echo "rc=$?""#,
    );
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(
        lines,
        ["rc=1", "rc=1", "C:/no/such.exe", "rc=1"],
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("type: gone: not found"),
        "no diagnostic: {}",
        out.stderr
    );
}

#[test]
fn type_takes_the_last_of_t_and_p_but_keeps_the_forced_search() {
    // As in bash: `-Pt` prints the type and `-tP` the path, but `-Pt` still searches PATH
    // only, so a function is not found under it.
    let out = cash(
        r#"f() { :; }; type -Pt cmd; type -tP cmd; type -pt cmd; type -tp f; echo "rc=$?"; type -Pt f; echo "rc=$?"; type -Pp f; echo "rc=$?"; type -t -p -t cmd; type -p -t -p f; echo "rc=$?""#,
    );
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.len(), 8, "{} {}", out.stdout, out.stderr);
    assert_eq!(lines[6..], ["file", "rc=0"], "a repeated flag was refused");
    assert_eq!(lines[0], "file");
    assert!(
        lines[1].to_ascii_lowercase().ends_with("/cmd.exe") && !lines[1].contains('\\'),
        "{}",
        lines[1]
    );
    assert_eq!(lines[2..6], ["file", "rc=0", "rc=1", "rc=1"]);
    assert!(out.stderr.is_empty(), "{}", out.stderr);
}

#[test]
fn hash_lists_hits_with_rendered_paths() {
    // Hashing counts nothing; each run and each `hash -t` lookup counts one, as in bash.
    let out = cash(
        "hash; hash netstat; hash; netstat -? > /dev/null 2>&1; hash -t netstat > /dev/null; hash",
    );
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.len(), 5, "{} {}", out.stdout, out.stderr);
    assert_eq!(lines[0], "hash: hash table empty");
    assert_eq!(lines[1], "hits\tcommand");
    assert!(lines[2].starts_with("   0\t"), "{}", lines[2]);
    assert_eq!(lines[3], "hits\tcommand");
    assert!(lines[4].starts_with("   2\t"), "{}", lines[4]);
    assert!(
        lines[4].to_ascii_lowercase().ends_with("/netstat.exe") && !lines[4].contains('\\'),
        "D3: {}",
        lines[4]
    );
}
