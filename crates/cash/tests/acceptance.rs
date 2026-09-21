//! The acceptance corpus — **D43**, and the executable form of **§4**.
//!
//! The `cash-win32` tests cover the Win32 layer in isolation. These run the real
//! `cash.exe` and assert the behaviour a user actually sees, which is a different thing:
//! every decision here was wired through `cash-core`, and a refactor there could undo it
//! without breaking a single library test.
//!
//! D43 is why these do not diff against bash: §4's divergences are deliberate, so a bash
//! reference would report every one as a failure. The expectations are written out
//! instead.

#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// The binary under test, as built by cargo for this integration test.
const CASH: &str = env!("CARGO_BIN_EXE_cash");

struct Output {
    stdout: String,
    stderr: String,
    code: i32,
}

/// Run a script through `cash -c`.
fn cash(script: &str) -> Output {
    run(&["-c".to_string(), script.to_string()])
}

/// Run `cash` with arbitrary arguments.
fn run(args: &[String]) -> Output {
    let out = Command::new(CASH)
        .args(args)
        .output()
        .expect("failed to run cash");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).trim_end().to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

/// A scratch directory that cleans itself up.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("cash-acceptance-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    /// The directory in cash's canonical spelling, safe to paste into a script.
    fn as_script_path(&self) -> String {
        self.0.to_string_lossy().replace('\\', "/")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ---------------------------------------------------------------------------
// D3 — path model. §4 divergence #1.
// ---------------------------------------------------------------------------

#[test]
fn pwd_renders_canonically() {
    // §9 measured brush printing `C:\Windows`. The whole point of D3 is that a rendered
    // path usually becomes an argument to a native exe, where the spelling is fatal.
    assert_eq!(cash("cd C:/Windows; pwd").stdout, "C:/Windows");
    assert_eq!(cash("cd C:/Windows; echo $PWD").stdout, "C:/Windows");
}

#[test]
fn every_spelling_of_a_directory_is_accepted() {
    for script in [
        "cd C:/Windows; pwd",
        "cd /c/Windows; pwd",
        r#"cd "C:\Windows"; pwd"#,
        r#"cd 'C:\Windows'; pwd"#,
    ] {
        assert_eq!(cash(script).stdout, "C:/Windows", "failed for: {script}");
    }
}

#[test]
fn relative_and_parent_navigation_stay_canonical() {
    assert_eq!(cash("cd C:/Windows; cd System32; pwd").stdout, "C:/Windows/System32");
    assert_eq!(cash("cd C:/Windows/System32; cd ..; pwd").stdout, "C:/Windows");
    // `cd -` echoes the directory it moved to, as bash does, so silence it to assert on
    // `pwd` alone. That echo is itself worth checking: it comes from $OLDPWD, which is a
    // separate rendering site from $PWD.
    assert_eq!(cash("cd C:/Windows; cd C:/Users; cd - >/dev/null; pwd").stdout, "C:/Windows");
    assert_eq!(cash("cd C:/Windows; cd C:/Users; cd -").stdout, "C:/Windows");
}

#[test]
fn tmp_maps_to_the_real_temp_directory() {
    let out = cash("cd /tmp; pwd").stdout;
    assert!(out.contains(':'), "/tmp did not resolve to a real path: {out}");
    assert!(!out.contains('\\'), "/tmp rendered with backslashes: {out}");
}

#[test]
fn home_agrees_with_pwd() {
    // These disagreeing is worse than either spelling: `[ "$PWD" = "$HOME" ]` silently
    // fails when one is C:/Users/me and the other C:\Users\me.
    let out = cash(r#"cd ~; [ "$PWD" = "$HOME" ] && echo agree || echo "$PWD vs $HOME""#);
    assert_eq!(out.stdout, "agree");
    assert!(!cash("echo $HOME").stdout.contains('\\'), "$HOME has backslashes");
}

#[test]
fn paths_derived_from_home_are_canonical_too() {
    // $HISTFILE is built by joining onto the home directory, and a naive join inserts a
    // backslash separator.
    let histfile = cash("echo $HISTFILE").stdout;
    assert!(!histfile.is_empty(), "HISTFILE unset");
    assert!(!histfile.contains('\\'), "HISTFILE has backslashes: {histfile}");
    assert!(histfile.ends_with(".cash_history"), "unexpected HISTFILE: {histfile}");
}

#[test]
fn unix_spellings_work_for_file_operations() {
    // D10's second chokepoint: every path cash resolves itself funnels through
    // absolute_path, so these work without each call site knowing about it.
    assert_eq!(cash("[ -f /c/Windows/win.ini ] && echo yes").stdout, "yes");
    assert_eq!(cash("read -r l < /c/Windows/win.ini; [ -n \"$l\" ] && echo read").stdout, "read");
}

#[test]
fn unc_paths_are_handled() {
    assert_eq!(cash("winpath //mynas/share1").stdout, "//mynas/share1");
    assert_eq!(cash("winpath -w //mynas/share1").stdout, r"\\mynas\share1");
    assert_eq!(cash(r"winpath '\\mynas\share1'").stdout, "//mynas/share1");
    assert_eq!(
        cash("winpath -w //mynas/share1/dir/file").stdout,
        r"\\mynas\share1\dir\file"
    );
}

// ---------------------------------------------------------------------------
// D20 / D41 — line endings and BOM. §4 divergence #4.
// ---------------------------------------------------------------------------

#[test]
fn command_substitution_drops_the_carriage_return() {
    // The insidious one: `[ "$v" = "1.2.0" ]` fails while printing identically, because
    // the \r merely returns the cursor. §9 measured 4 bytes where bash-on-Linux gives 3.
    let scratch = Scratch::new("crlf-subst");
    std::fs::write(scratch.path().join("v.txt"), b"1.2.0\r\n").unwrap();

    let script = format!(
        r#"v=$(cat {dir}/v.txt); printf '%s|%s' "$v" "$(printf %s "$v" | wc -c)""#,
        dir = scratch.as_script_path()
    );
    assert_eq!(cash(&script).stdout, "1.2.0|5");
}

#[test]
fn read_drops_the_carriage_return() {
    let scratch = Scratch::new("crlf-read");
    std::fs::write(scratch.path().join("two.txt"), b"a\r\nb\r\n").unwrap();

    let script = format!(
        r#"n=0; while read -r l; do n=$((n+1)); last="$l"; done < {dir}/two.txt; printf '%s|%s' "$n" "$(printf %s "$last" | wc -c)""#,
        dir = scratch.as_script_path()
    );
    assert_eq!(cash(&script).stdout, "2|1");
}

#[test]
fn an_explicit_delimiter_is_left_alone() {
    // D20 applies only where cash decides a line ends. `-d` means the caller has its own
    // framing, and a lone \r with no \n is data rather than a terminator.
    assert_eq!(cash(r#"printf 'x:y' | { read -r -d ':' a; printf '%s' "$a"; }"#).stdout, "x");
}

#[test]
fn a_script_with_a_bom_runs() {
    // Windows editors write one, and it is invisible while breaking the script outright:
    // the shebang goes unrecognised and the first token carries three phantom bytes.
    let scratch = Scratch::new("bom");
    let script = scratch.path().join("bom.sh");

    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(b"#!/usr/bin/env bash\r\necho bom-ok\r\n");
    std::fs::write(&script, bytes).unwrap();

    assert_eq!(run(&[script.to_string_lossy().into_owned()]).stdout, "bom-ok");
}

// ---------------------------------------------------------------------------
// D15 — exit codes.
// ---------------------------------------------------------------------------

#[test]
fn exit_codes_truncate_like_bash() {
    assert_eq!(cash("cmd.exe /d /s /c exit 3 >/dev/null 2>&1; echo $?").stdout, "3");
    assert_eq!(cash("cmd.exe /d /s /c exit 300 >/dev/null 2>&1; echo $?").stdout, "44");
}

#[test]
fn the_shells_own_exit_status_propagates() {
    // The mapped status has to reach cash's own exit code, not just `$?` inside the
    // script — that is what a caller of cash.exe actually observes.
    assert_eq!(cash("true").code, 0);
    assert_eq!(cash("exit 7").code, 7);
    assert_eq!(cash("cmd.exe /d /s /c exit 3 >/dev/null 2>&1").code, 3);
    assert_eq!(
        cash("cmd.exe /d /s /c exit 3221225477 >/dev/null 2>&1").code,
        139,
        "an access violation should surface as a segfault-equivalent status"
    );
}

#[test]
fn a_crash_can_never_be_reported_as_success() {
    // The reason D15 exists. 0xC0000100 & 0xFF is zero, so naive truncation reports a
    // crashed process as success and it passes an && chain.
    let access_violation = cash("cmd.exe /d /s /c exit 3221225477 >/dev/null 2>&1; echo $?");
    assert_eq!(access_violation.stdout, "139", "should match a Linux segfault");

    let truncates_to_zero = cash("cmd.exe /d /s /c exit 3221225728 >/dev/null 2>&1; echo $?");
    assert_ne!(truncates_to_zero.stdout, "0", "a crash reported as success");
}

// ---------------------------------------------------------------------------
// D31 — environment case. §4 divergence #6.
// ---------------------------------------------------------------------------

#[test]
fn environment_lookup_ignores_case() {
    assert_eq!(cash(r#"[ -n "$PATH" ] && [ -n "$Path" ] && [ -n "$path" ] && echo all"#).stdout, "all");
}

#[test]
fn an_exact_match_still_wins() {
    // The fallback must not collapse the namespace: a script defining its own $x and $X
    // keeps bash semantics.
    assert_eq!(cash("x=lower; X=UPPER; printf '%s|%s' \"$x\" \"$X\"").stdout, "lower|UPPER");
}

// ---------------------------------------------------------------------------
// §9.1 — the parser fix.
// ---------------------------------------------------------------------------

#[test]
fn case_inside_command_substitution_parses() {
    // A case pattern's `)` has no opener, so a plain paren count ends the substitution
    // at `a*)`. Needed fixing in both the tokenizer and the word parser.
    assert_eq!(cash("x=$(case abc in a*) echo m;; esac); echo $x").stdout, "m");
    assert_eq!(cash("x=$(case xyz in a*) echo m;; *) echo other;; esac); echo $x").stdout, "other");
    assert_eq!(cash("x=$(case abc in (a*) echo paren;; esac); echo $x").stdout, "paren");
    assert_eq!(
        cash("x=$(case a in a) case b in b) echo nest;; esac;; esac); echo $x").stdout,
        "nest"
    );
    // A word merely containing `esac` must not terminate anything.
    assert_eq!(cash("x=$(echo esacular); echo $x").stdout, "esacular");
}

// ---------------------------------------------------------------------------
// D45 — cash's own builtins.
// ---------------------------------------------------------------------------

#[test]
fn cash_builtins_exist() {
    for name in ["winpath", "start", "elevate", "detach"] {
        let out = cash(&format!("type {name}"));
        assert!(
            out.stdout.contains("builtin"),
            "{name} is not a builtin: {}{}",
            out.stdout,
            out.stderr
        );
    }
}

#[test]
fn winpath_converts_between_all_three_spellings() {
    assert_eq!(cash("winpath /c/Users/x").stdout, "C:/Users/x");
    assert_eq!(cash("winpath -w /c/Users/x").stdout, r"C:\Users\x");
    assert_eq!(cash("winpath -u C:/Users/x").stdout, "/c/Users/x");
}

#[test]
fn winpath_handles_every_drive_letter() {
    // Nothing special-cases C.
    assert_eq!(cash("winpath /d/data/x").stdout, "D:/data/x");
    assert_eq!(cash("winpath /z/net/y").stdout, "Z:/net/y");
    assert_eq!(cash("winpath -u E:/vm/z").stdout, "/e/vm/z");
}

#[test]
fn winpath_reads_stdin_so_it_composes() {
    assert_eq!(cash(r#"printf 'C:/a\n/c/b\n' | winpath -w"#).stdout, "C:\\a\nC:\\b");
}

// ---------------------------------------------------------------------------
// The /x/ diagnostic — D3 and D4 meeting.
// ---------------------------------------------------------------------------

#[test]
fn a_unix_drive_argument_to_a_command_is_explained() {
    // D3 accepts /c/... for paths cash resolves; D4 forbids rewriting arguments, so a
    // command gets it verbatim and fails with an error that explains nothing.
    let out = cash(r#"enable -n cat 2>/dev/null; cat /c/Windows/win.ini >/dev/null"#);
    assert!(
        out.stderr.contains("winpath") && out.stderr.contains("D4"),
        "no hint given; stderr was: {}",
        out.stderr
    );
}

#[test]
fn the_diagnostic_stays_quiet_when_it_would_not_help() {
    // Only fires when the literal spelling fails AND translating fixes it, which makes
    // false positives essentially impossible.
    let valid = cash(r#"enable -n cat 2>/dev/null; cat C:/Windows/win.ini >/dev/null"#);
    assert!(!valid.stderr.contains("winpath"), "warned on a valid path: {}", valid.stderr);

    // /cash is a directory name, not a drive.
    let not_a_drive = cash(r#"enable -n cat 2>/dev/null; cat /cash/nope 2>/dev/null"#);
    assert!(!not_a_drive.stderr.contains("winpath"), "warned on a non-drive path");

    // Translating would not help: it does not exist either way.
    let missing = cash(r#"enable -n cat 2>/dev/null; cat /d/definitely/not/here 2>/dev/null"#);
    assert!(!missing.stderr.contains("winpath"), "warned when translation would not help");
}

// ---------------------------------------------------------------------------
// D48 — bundled coreutils.
// ---------------------------------------------------------------------------

#[test]
fn coreutils_are_builtins_and_need_no_path() {
    assert!(cash("type cat").stdout.contains("builtin"));
    // The point of bundling: cash carries its own userland for these.
    assert_eq!(cash(r#"PATH=""; echo bundled | cat"#).stdout, "bundled");
    assert_eq!(cash(r#"PATH=""; printf 'b\na\n' | sort"#).stdout, "a\nb");
}

#[test]
fn a_builtin_can_be_disabled() {
    // bash's own escape hatch, and the answer to D48's silent-substitution risk for
    // anyone running GNU coreutils rather than uutils.
    assert!(!cash("enable -n cat; type cat").stdout.contains("builtin"));
}

// ---------------------------------------------------------------------------
// D16 / D7 — globbing and /dev/null.
// ---------------------------------------------------------------------------

#[test]
fn globbing_is_case_insensitive() {
    // §4 divergence #3. On a case-insensitive volume `main.tf` and `main.TF` cannot
    // coexist, so this can only remove false negatives.
    let scratch = Scratch::new("glob");
    std::fs::write(scratch.path().join("Upper.TXT"), b"x").unwrap();

    let script = format!("cd {}; echo *.txt", scratch.as_script_path());
    assert_eq!(cash(&script).stdout, "Upper.TXT");
}

#[test]
fn dev_null_discards() {
    assert_eq!(cash("echo noise > /dev/null; echo done").stdout, "done");
}

// ---------------------------------------------------------------------------
// D6 — the session guarantee is actually installed.
// ---------------------------------------------------------------------------

#[test]
fn the_session_job_is_installed() {
    let out = Command::new(CASH)
        .args(["-c", "true"])
        .env("CASH_DEBUG_SESSION", "1")
        .output()
        .expect("run cash");
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        stderr.contains("session job installed"),
        "D6's containment was not in force: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// D13 / D19 / D21 / D22 — signals.
// ---------------------------------------------------------------------------

#[test]
fn traps_are_accepted_for_signals_cash_can_deliver() {
    // §9 measured `trap INT` rejected outright: Signal was an empty enum on Windows, so
    // nothing signal-shaped worked at all.
    for signal in ["INT", "TERM", "HUP", "QUIT", "SIGINT", "EXIT"] {
        let out = cash(&format!(r#"trap "echo caught" {signal}; echo ok"#));
        assert!(
            out.stdout.contains("ok"),
            "trap {signal} was rejected: {}{}",
            out.stdout,
            out.stderr
        );
    }
}

#[test]
fn the_exit_trap_actually_fires() {
    assert_eq!(cash(r#"trap "echo cleanup" EXIT; echo body"#).stdout, "body\ncleanup");
}

#[test]
fn a_signal_with_no_win32_mechanism_is_refused_not_faked() {
    // Accepting a trap that could never fire is the silent-failure pattern D20 and D26
    // both reject. There is no Win32 mechanism behind SIGUSR1, so say so.
    for signal in ["USR1", "USR2", "PIPE", "ALRM", "CHLD"] {
        let out = cash(&format!(r#"trap "echo x" {signal}"#));
        assert!(
            out.stderr.contains("invalid signal"),
            "trap {signal} should have been refused, got: {}{}",
            out.stdout,
            out.stderr
        );
    }
}

#[test]
fn kill_is_a_builtin_and_lists_real_signal_numbers() {
    // kill was gated to Unix by a single `nix::` reference, so Windows had no kill
    // builtin at all and fell through to whatever kill.exe was on PATH.
    assert!(cash("type kill").stdout.contains("builtin"));

    let listed = cash("kill -l").stdout;
    for expected in ["1) HUP", "2) INT", "9) KILL", "15) TERM"] {
        assert!(listed.contains(expected), "kill -l missing {expected}:\n{listed}");
    }
    // Only what cash can actually deliver: no ILL, PIPE, USR1 that would never fire.
    assert!(!listed.contains("USR1"), "kill -l lists a signal cash cannot deliver");
}

#[test]
fn signal_names_and_numbers_round_trip() {
    // These come from the enum's discriminants, because shared code converts with
    // `s as i32`. Declaration order would make `kill -l KILL` answer 4 rather than 9.
    assert_eq!(cash("kill -l 9").stdout, "KILL");
    assert_eq!(cash("kill -l KILL").stdout, "9");
    assert_eq!(cash("kill -l 15").stdout, "TERM");
    assert_eq!(cash("kill -l TERM").stdout, "15");
    assert_eq!(cash("kill -l 2").stdout, "INT");
}
