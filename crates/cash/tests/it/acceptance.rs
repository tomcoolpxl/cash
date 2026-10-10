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

use std::io::Write as _;
use std::path::Path;
use std::process::Stdio;

use crate::common::{CASH, Output, Scratch, cash_command, output_of, run as cash};
use crate::process_identity::OwnPing;

/// Run `cash` with arbitrary arguments.
fn run(args: &[String]) -> Output {
    output_of(cash_command().args(args))
}

/// Run a script through `cash -c` with `input` waiting on its standard input, a pipe.
fn cash_reading(script: &str, input: &str) -> Output {
    run_reading(&["-c", script], input)
}

/// Run `cash` with arbitrary arguments and `input` waiting on its standard input.
fn run_reading(args: &[&str], input: &str) -> Output {
    let mut child = cash_command()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run cash");
    // A script that fails before it reads may be gone already, which is not the
    // failure under test.
    let _ = child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.as_bytes());
    let out = child.wait_with_output().expect("wait for cash");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).trim_end().to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

/// Run a script through `cash -c` in `dir`, which is its temp directory as well; one
/// still running after 30 seconds is ended, and the test fails.
fn cash_in_scratch(dir: &Path, script: &str) -> Output {
    let mut child = cash_command()
        .args(["-c", script])
        .current_dir(dir)
        .env("TEMP", dir)
        .env("TMP", dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run cash");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while child.try_wait().expect("poll cash").is_none() {
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            panic!("cash was still running after 30 s: {script}");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let out = child.wait_with_output().expect("wait for cash");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).trim_end().to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_string(),
        code: out.status.code().unwrap_or(-1),
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
    assert_eq!(
        cash("cd C:/Windows; cd System32; pwd").stdout,
        "C:/Windows/System32"
    );
    assert_eq!(
        cash("cd C:/Windows/System32; cd ..; pwd").stdout,
        "C:/Windows"
    );
    // `cd -` echoes the directory it moved to, as bash does, so silence it to assert on
    // `pwd` alone. That echo is itself worth checking: it comes from $OLDPWD, which is a
    // separate rendering site from $PWD.
    assert_eq!(
        cash("cd C:/Windows; cd C:/Users; cd - >/dev/null; pwd").stdout,
        "C:/Windows"
    );
    assert_eq!(
        cash("cd C:/Windows; cd C:/Users; cd -").stdout,
        "C:/Windows"
    );
}

#[test]
fn tmp_maps_to_the_real_temp_directory() {
    let out = cash("cd /tmp; pwd").stdout;
    assert!(
        out.contains(':'),
        "/tmp did not resolve to a real path: {out}"
    );
    assert!(!out.contains('\\'), "/tmp rendered with backslashes: {out}");
}

#[test]
fn home_agrees_with_pwd() {
    // These disagreeing is worse than either spelling: `[ "$PWD" = "$HOME" ]` silently
    // fails when one is C:/Users/me and the other C:\Users\me.
    let out = cash(r#"cd ~; [ "$PWD" = "$HOME" ] && echo agree || echo "$PWD vs $HOME""#);
    assert_eq!(out.stdout, "agree");
    assert!(
        !cash("echo $HOME").stdout.contains('\\'),
        "$HOME has backslashes"
    );
}

#[test]
fn paths_derived_from_home_are_canonical_too() {
    // $HISTFILE is built by joining onto the home directory, and a naive join inserts a
    // backslash separator.
    let histfile = cash("echo $HISTFILE").stdout;
    assert!(!histfile.is_empty(), "HISTFILE unset");
    assert!(
        !histfile.contains('\\'),
        "HISTFILE has backslashes: {histfile}"
    );
    assert!(
        histfile.ends_with(".cash_history"),
        "unexpected HISTFILE: {histfile}"
    );
}

#[test]
fn unix_spellings_work_for_file_operations() {
    // D10's second chokepoint: every path cash resolves itself funnels through
    // absolute_path, so these work without each call site knowing about it.
    assert_eq!(cash("[ -f /c/Windows/win.ini ] && echo yes").stdout, "yes");
    assert_eq!(
        cash("read -r l < /c/Windows/win.ini; [ -n \"$l\" ] && echo read").stdout,
        "read"
    );
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
    assert_eq!(
        cash(r#"printf 'x:y' | { read -r -d ':' a; printf '%s' "$a"; }"#).stdout,
        "x"
    );
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

    assert_eq!(
        run(&[script.to_string_lossy().into_owned()]).stdout,
        "bom-ok"
    );
}

// ---------------------------------------------------------------------------
// D15 — exit codes.
// ---------------------------------------------------------------------------

#[test]
fn exit_codes_truncate_like_bash() {
    assert_eq!(
        cash("cmd.exe /d /s /c exit 3 >/dev/null 2>&1; echo $?").stdout,
        "3"
    );
    assert_eq!(
        cash("cmd.exe /d /s /c exit 300 >/dev/null 2>&1; echo $?").stdout,
        "44"
    );
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
    assert_eq!(
        access_violation.stdout, "139",
        "should match a Linux segfault"
    );

    let truncates_to_zero = cash("cmd.exe /d /s /c exit 3221225728 >/dev/null 2>&1; echo $?");
    assert_ne!(truncates_to_zero.stdout, "0", "a crash reported as success");
}

// ---------------------------------------------------------------------------
// D31 — environment case. §4 divergence #6.
// ---------------------------------------------------------------------------

#[test]
fn environment_lookup_ignores_case() {
    assert_eq!(
        cash(r#"[ -n "$PATH" ] && [ -n "$Path" ] && [ -n "$path" ] && echo all"#).stdout,
        "all"
    );
}

#[test]
fn an_exact_match_still_wins() {
    // The fallback must not collapse the namespace: a script defining its own $x and $X
    // keeps bash semantics.
    assert_eq!(
        cash("x=lower; X=UPPER; printf '%s|%s' \"$x\" \"$X\"").stdout,
        "lower|UPPER"
    );
}

// ---------------------------------------------------------------------------
// §9.1 — the parser fix.
// ---------------------------------------------------------------------------

#[test]
fn case_inside_command_substitution_parses() {
    // A case pattern's `)` has no opener, so a plain paren count ends the substitution
    // at `a*)`. Needed fixing in both the tokenizer and the word parser.
    assert_eq!(
        cash("x=$(case abc in a*) echo m;; esac); echo $x").stdout,
        "m"
    );
    assert_eq!(
        cash("x=$(case xyz in a*) echo m;; *) echo other;; esac); echo $x").stdout,
        "other"
    );
    assert_eq!(
        cash("x=$(case abc in (a*) echo paren;; esac); echo $x").stdout,
        "paren"
    );
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
    assert_eq!(
        cash(r#"printf 'C:/a\n/c/b\n' | winpath -w"#).stdout,
        "C:\\a\nC:\\b"
    );
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
        out.stderr.contains("winpath") && !out.stderr.contains("(D4)"),
        "no hint given; stderr was: {}",
        out.stderr
    );
}

#[test]
fn the_diagnostic_stays_quiet_when_it_would_not_help() {
    // Only fires when the literal spelling fails AND translating fixes it, which makes
    // false positives essentially impossible.
    let valid = cash(r#"enable -n cat 2>/dev/null; cat C:/Windows/win.ini >/dev/null"#);
    assert!(
        !valid.stderr.contains("winpath"),
        "warned on a valid path: {}",
        valid.stderr
    );

    // /cash is a directory name, not a drive.
    let not_a_drive = cash(r#"enable -n cat 2>/dev/null; cat /cash/nope 2>/dev/null"#);
    assert!(
        !not_a_drive.stderr.contains("winpath"),
        "warned on a non-drive path"
    );

    // Translating would not help: it does not exist either way.
    let missing = cash(r#"enable -n cat 2>/dev/null; cat /d/definitely/not/here 2>/dev/null"#);
    assert!(
        !missing.stderr.contains("winpath"),
        "warned when translation would not help"
    );
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

    // Every way a script reaches it, and the spelling `"$dir/null"` makes of it.
    for script in [
        "echo noise >> /dev/null",
        "echo noise &> /dev/null",
        "{ echo noise; echo more >&2; } > /dev/null 2>&1",
        "set -C; echo noise > /dev/null",
        "exec 3> /dev/null; echo noise >&3",
        r#"d=/dev/; echo noise > "$d/null""#,
        "cat < /dev/null",
        "read -r l < /dev/null",
    ] {
        let out = cash(&format!("{script}; echo done"));
        assert_eq!(out.stdout, "done", "failed for: {script}");
        assert_eq!(out.stderr, "", "failed for: {script}");
    }
}

#[test]
fn a_file_called_null_in_a_folder_called_dev_is_a_file() {
    // Any resolved path that ended in `dev/null` was the device, so in a folder called
    // `dev`, which is where many people keep their work, output sent to a file called
    // `null` was thrown away without a word. bash writes the file.
    for (name, script, file) in [
        ("relative", "mkdir dev; echo kept > dev/null", "dev/null"),
        ("inside", "mkdir dev; cd dev; echo kept > null", "dev/null"),
        ("dotted", "mkdir dev; echo kept > ./dev/null", "dev/null"),
        (
            "absolute",
            r#"mkdir dev; echo kept > "$PWD/dev/null""#,
            "dev/null",
        ),
        ("appended", "mkdir dev; echo kept >> dev/null", "dev/null"),
        ("both", "mkdir dev; echo kept &> dev/null", "dev/null"),
        (
            "nested",
            "mkdir -p a/dev; echo kept > a/dev/null",
            "a/dev/null",
        ),
    ] {
        let scratch = Scratch::new(&format!("dev-null-file-{name}"));
        let out = cash(&format!(
            r#"cd {}; {script}; echo "rc=$?""#,
            scratch.as_script_path()
        ));
        assert_eq!(out.stdout, "rc=0", "failed for: {script}");
        assert_eq!(
            std::fs::read_to_string(scratch.path().join(file)).ok(),
            Some("kept\n".to_string()),
            "no file written for: {script}\nstderr: {}",
            out.stderr
        );
    }

    // And read: the file's contents, not the device's nothing.
    let scratch = Scratch::new("dev-null-file-read");
    std::fs::create_dir(scratch.path().join("dev")).unwrap();
    std::fs::write(scratch.path().join("dev").join("null"), b"kept\n").unwrap();
    let out = cash(&format!(
        "cd {}; cat < dev/null; cd dev; cat < null",
        scratch.as_script_path()
    ));
    assert_eq!(out.stdout, "kept\nkept", "stderr: {}", out.stderr);
}

// ---------------------------------------------------------------------------
// D7 — /dev/stdin, /dev/stdout, /dev/stderr, /dev/fd/N.
//
// Windows has no such files: cash sees from the name that a descriptor is meant, and
// hands over the one the command has at that moment. Every expectation here is what
// the bash 5.3 of Git for Windows prints for the same script, but for the one test
// that says otherwise.
// ---------------------------------------------------------------------------

#[test]
fn dev_stdin_is_standard_input() {
    // Each of these said "failed to redirect to C:/dev/stdin": the name had been
    // resolved to a path on the current drive before anything looked at it.
    for (script, expected) in [
        (r#"cat < /dev/stdin; echo "rc=$?""#, "x\ny\nrc=0"),
        (r#"read -r l < /dev/stdin; echo "rc=$? l=$l""#, "rc=0 l=x"),
        (
            r#"while read -r l; do echo "got $l"; done < /dev/stdin; echo "rc=$?""#,
            "got x\ngot y\nrc=0",
        ),
        (r#"cat < /dev/fd/0; echo "rc=$?""#, "x\ny\nrc=0"),
        (
            r#"mapfile -t a < /dev/stdin; echo "${#a[@]} ${a[*]}""#,
            "2 x y",
        ),
        (r#"v=$(< /dev/stdin); echo "$v""#, "x\ny"),
        (r#"v=$(cat < /dev/stdin); echo "$v""#, "x\ny"),
        (r#"(cat < /dev/stdin)"#, "x\ny"),
        (r#"cat < /dev/stdin | tr a-z A-Z"#, "X\nY"),
        (r#"n=/dev/stdin; cat < "$n""#, "x\ny"),
        // One input, read in turns: the second `read` goes on where the first stopped.
        (
            r#"read -r a < /dev/stdin; read -r b < /dev/stdin; echo "a=$a b=$b""#,
            "a=x b=y",
        ),
        (r#"exec 3< /dev/stdin; read -r -u 3 l; echo "l=$l""#, "l=x"),
    ] {
        let out = cash_reading(script, "x\ny\n");
        assert_eq!(
            out.stdout, expected,
            "failed for: {script}\nstderr: {}",
            out.stderr
        );
    }
}

#[test]
fn dev_stdin_is_the_commands_input_not_the_shells() {
    // A here-string, a pipeline and a redirected function each give a command an input
    // of its own, and that is the one the name means. The shell's input holds "shell".
    let scratch = Scratch::new("dev-stdin");
    std::fs::write(scratch.path().join("f.txt"), b"from a file\n").unwrap();

    for (script, expected) in [
        (
            r#"{ cat < /dev/stdin; } <<< "from a here-string""#,
            "from a here-string",
        ),
        (r#"echo "from a pipe" | cat < /dev/stdin"#, "from a pipe"),
        (r#"f() { cat < /dev/stdin; }; f < f.txt"#, "from a file"),
    ] {
        let script = format!("cd {}; {script}", scratch.as_script_path());
        let out = cash_reading(&script, "shell\n");
        assert_eq!(
            out.stdout, expected,
            "failed for: {script}\nstderr: {}",
            out.stderr
        );
    }
}

#[test]
fn dev_stdout_and_dev_stderr_are_the_standard_streams() {
    for name in ["/dev/stdout", "/dev/fd/1"] {
        let out = cash(&format!("echo one > {name}; echo two >> {name}"));
        assert_eq!(out.stdout, "one\ntwo", "failed for {name}: {}", out.stderr);
        assert_eq!(out.stderr, "", "{name} wrote to standard error");
    }
    for name in ["/dev/stderr", "/dev/fd/2"] {
        let out = cash(&format!("echo one > {name}; echo two >> {name}"));
        assert_eq!(out.stderr, "one\ntwo", "failed for {name}");
        assert_eq!(out.stdout, "", "{name} wrote to standard output");
    }

    let both = cash("echo both &> /dev/stderr; echo again &>> /dev/stderr");
    assert_eq!(both.stderr, "both\nagain");
    assert_eq!(both.stdout, "");

    // The idiom for an error message, and its opposite.
    let swapped = cash("{ echo oops >&2; } 2> /dev/stdout");
    assert_eq!(swapped.stdout, "oops");
    assert_eq!(swapped.stderr, "");
}

#[test]
fn dev_stdout_is_the_commands_output_not_the_shells() {
    let scratch = Scratch::new("dev-stdout");

    for (script, expected) in [
        (
            r#"v=$(echo captured > /dev/stdout); echo "v=$v""#,
            "v=captured",
        ),
        (r#"echo piped > /dev/stdout | tr a-z A-Z"#, "PIPED"),
        (
            r#"{ echo "to a file" > /dev/stderr; } 2> o.txt; cat o.txt"#,
            "to a file",
        ),
        (
            r#"f() { echo "in f" > /dev/stderr; }; f 2> o.txt; cat o.txt"#,
            "in f",
        ),
    ] {
        let script = format!("cd {}; {script}", scratch.as_script_path());
        let out = cash(&script);
        assert_eq!(
            out.stdout, expected,
            "failed for: {script}\nstderr: {}",
            out.stderr
        );
        assert_eq!(out.stderr, "", "failed for: {script}");
    }

    // Standard error is not what a command substitution collects.
    let out = cash(r#"v=$(echo aside > /dev/stderr); echo "v=$v""#);
    assert_eq!(out.stdout, "v=");
    assert_eq!(out.stderr, "aside");
}

#[test]
fn dev_fd_n_is_a_descriptor_the_script_opened() {
    let scratch = Scratch::new("dev-fd");
    std::fs::write(scratch.path().join("f.txt"), b"from a file\n").unwrap();

    for (script, expected) in [
        (r#"exec 3< f.txt; cat < /dev/fd/3"#, "from a file"),
        (r#"exec {fd}< f.txt; cat < /dev/fd/$fd"#, "from a file"),
        (
            r#"exec 3> o.txt; echo three > /dev/fd/3; exec 3>&-; cat o.txt"#,
            "three",
        ),
        (
            r#"exec 3<> o.txt; echo both-ways > /dev/fd/3; exec 3>&-; cat o.txt"#,
            "both-ways",
        ),
    ] {
        let script = format!("cd {}; {script}", scratch.as_script_path());
        let out = cash(&script);
        assert_eq!(
            out.stdout, expected,
            "failed for: {script}\nstderr: {}",
            out.stderr
        );
    }
}

#[test]
fn a_descriptor_that_is_not_open_fails_the_redirection() {
    // bash: "/dev/fd/9: No such file or directory", and the command does not run. cash
    // says the same of the name as written: no file called C:/dev/fd/9 was looked for,
    // so none can be opened or made by mistake.
    for (script, name) in [
        ("cat < /dev/fd/9", "/dev/fd/9"),
        ("echo x > /dev/fd/9", "/dev/fd/9"),
        ("echo x &> /dev/fd/9", "/dev/fd/9"),
        ("exec 0<&-; cat < /dev/stdin", "/dev/stdin"),
    ] {
        let out = cash_reading(&format!(r#"{script}; echo "rc=$?""#), "x\n");
        assert_eq!(out.stdout, "rc=1", "failed for: {script}");
        assert!(
            out.stderr
                .contains(&format!("{name}: No such file or directory"))
                && !out.stderr.contains(":/dev/"),
            "failed for: {script}\nstderr: {}",
            out.stderr
        );
    }
}

#[test]
fn a_device_name_shares_the_descriptor_instead_of_opening_the_file_again() {
    // The one deliberate difference. Linux and Git Bash open the file a second time,
    // so `> /dev/stdout` truncates what standard output has already written to it and
    // bash prints "b" alone; a read starts over from the first line. cash hands over
    // the descriptor itself, which is what bash does on a system with no /dev/stdout
    // and what `>&1` does everywhere.
    let scratch = Scratch::new("dev-shared");
    std::fs::write(scratch.path().join("f.txt"), b"l1\nl2\nl3\n").unwrap();

    for (script, expected) in [
        (
            r#"{ echo a; echo b > /dev/stdout; } > o.txt; cat o.txt"#,
            "a\nb",
        ),
        (r#"exec 3< f.txt; read -r a <&3; cat < /dev/fd/3"#, "l2\nl3"),
    ] {
        let script = format!("cd {}; {script}", scratch.as_script_path());
        let out = cash(&script);
        assert_eq!(
            out.stdout, expected,
            "failed for: {script}\nstderr: {}",
            out.stderr
        );
    }
}

#[test]
fn source_reads_a_device_name() {
    // `kubectl completion bash | source /dev/stdin` is how completions are loaded.
    let piped = cash_reading(r#". /dev/stdin; echo "v=$v""#, "v=sourced\n");
    assert_eq!(piped.stdout, "v=sourced", "stderr: {}", piped.stderr);

    let here = cash(r#"source /dev/stdin <<< 'echo "from a here-string"'"#);
    assert_eq!(here.stdout, "from a here-string", "stderr: {}", here.stderr);

    let null = cash(r#". /dev/null; echo "rc=$?""#);
    assert_eq!(null.stdout, "rc=0", "stderr: {}", null.stderr);

    // The script file is opened the same way: `curl … | cash /dev/stdin`.
    let script = run_reading(&["/dev/stdin"], "echo \"from the script\"\n");
    assert_eq!(
        script.stdout, "from the script",
        "stderr: {}",
        script.stderr
    );
    assert_eq!(run_reading(&["/dev/null"], "").code, 0);
}

#[test]
fn a_name_that_only_resembles_a_device_is_a_file() {
    let scratch = Scratch::new("dev-near");
    let cd = format!("cd {}", scratch.as_script_path());

    // A folder called `dev` is an ordinary folder.
    let relative = cash(&format!(
        "{cd}; mkdir dev; echo kept > dev/stdout; cat dev/stdout"
    ));
    assert_eq!(relative.stdout, "kept", "stderr: {}", relative.stderr);
    assert!(scratch.path().join("dev").join("stdout").is_file());

    // A longer name is a file that is not there.
    let longer = cash_reading(r#"cat < /dev/stdinx; echo "rc=$?""#, "x\n");
    assert_eq!(longer.stdout, "rc=1");

    // What a filesystem reads as the same name is the same name.
    let doubled = cash_reading(r#"d=/dev/; cat < "$d/stdin""#, "x\n");
    assert_eq!(doubled.stdout, "x", "stderr: {}", doubled.stderr);
}

// ---------------------------------------------------------------------------
// D6 — the session guarantee is actually installed.
// ---------------------------------------------------------------------------

#[test]
fn the_session_job_is_installed() {
    let out = cash_command()
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
    assert_eq!(
        cash(r#"trap "echo cleanup" EXIT; echo body"#).stdout,
        "body\ncleanup"
    );
}

#[test]
fn a_signal_with_no_win32_mechanism_is_refused_not_faked() {
    // Accepting a trap that could never fire is the silent-failure pattern D20 and D26
    // both reject. There is no Win32 mechanism behind SIGUSR1, so say so. (CHLD is not
    // here: cash sees its children exit, and emulates it, D64.)
    for signal in ["USR1", "USR2", "PIPE", "ALRM"] {
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
    for expected in ["1) SIGHUP", "2) SIGINT", "9) SIGKILL", "15) SIGTERM"] {
        assert!(
            listed.contains(expected),
            "kill -l missing {expected}:\n{listed}"
        );
    }
    // Only what cash can actually deliver: no ILL, PIPE, USR1 that would never fire.
    assert!(
        !listed.contains("USR1"),
        "kill -l lists a signal cash cannot deliver"
    );
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

#[test]
fn a_background_job_reports_the_pid_it_spawned() {
    // D11/D22. A background job runs as a tokio task executing the whole and-or list,
    // so the external it spawns is not one of the job's tasks — which left `$!` empty
    // and `kill %1` with nothing to signal. The task now reports the pid to the job.
    let out =
        cash(r#"ping.exe -n 20 127.0.0.1 >/dev/null & sleep 1; echo "$!"; kill -9 $! 2>/dev/null"#);
    let pid = out.stdout.lines().next().unwrap_or_default();
    assert!(
        pid.parse::<u32>().is_ok(),
        "$! should be a pid, got {pid:?} (stderr: {})",
        out.stderr
    );

    let listed =
        cash(r#"ping.exe -n 20 127.0.0.1 >/dev/null & sleep 1; jobs -p; kill -9 $! 2>/dev/null"#);
    assert!(
        listed
            .stdout
            .lines()
            .next()
            .unwrap_or_default()
            .parse::<u32>()
            .is_ok(),
        "jobs -p should list a pid, got {:?}",
        listed.stdout
    );
}

#[test]
fn killing_a_background_job_actually_kills_it() {
    // Both spellings D22 distinguishes: a job spec reaps the job, a bare pid the process.
    // `wait` says how the ping ended: 128 + the signal, or 0 had it run out its ten
    // echoes. `kill -0 $p` a second later would ask about the pid, which may be another
    // process's by then (see `process_identity.rs`).
    let by_spec = cash(
        r#"ping.exe -n 10 127.0.0.1 >/dev/null & sleep 1; p=$!; kill %1; wait $p; echo "status=$?""#,
    );
    assert_eq!(
        by_spec.stdout.lines().last().unwrap_or_default(),
        "status=143",
        "kill %1 did not kill"
    );

    let by_pid = cash(
        r#"ping.exe -n 10 127.0.0.1 >/dev/null & sleep 1; p=$!; kill -9 $p; wait $p; echo "status=$?""#,
    );
    assert_eq!(
        by_pid.stdout.lines().last().unwrap_or_default(),
        "status=137",
        "kill -9 $! did not kill"
    );
}

#[test]
fn the_commands_ms_coreutils_withholds_are_all_available() {
    // §2: Microsoft withholds these because the names collide with PowerShell aliases
    // and cmd builtins. cash owns name resolution, so that conflict table does not
    // apply — and it claimed the gap as the reason cash exists.
    for name in ["kill", "timeout", "whoami", "dir", "expand", "more"] {
        let out = cash(&format!("type {name}"));
        assert!(
            out.stdout.contains("builtin"),
            "{name} should be a builtin: {}{}",
            out.stdout,
            out.stderr
        );
    }
}

#[test]
fn the_commands_ms_coreutils_warns_about_are_all_available() {
    // The other half of §2's table: names Microsoft ships but flags because PowerShell
    // aliases or cmd builtins shadow them. Under cash they are unambiguous.
    for name in [
        "date", "echo", "mkdir", "rmdir", "cat", "cp", "ls", "mv", "pwd", "rm", "sleep", "sort",
        "tee", "uptime",
    ] {
        let out = cash(&format!("type {name}"));
        assert!(
            out.stdout.contains("builtin"),
            "{name} should be a builtin: {}{}",
            out.stdout,
            out.stderr
        );
    }
}

#[test]
fn timeout_works_without_an_external_one() {
    // 124 is the POSIX exit status for "the command timed out".
    let out = cash(r#"PATH=""; timeout 1 ping.exe -n 30 127.0.0.1 >/dev/null; echo $?"#);
    assert_eq!(
        out.stdout, "124",
        "timeout did not time out (stderr: {})",
        out.stderr
    );
}

// ---------------------------------------------------------------------------
// D17 — process substitution.
// ---------------------------------------------------------------------------

#[test]
fn process_substitution_works_via_temp_files() {
    // Windows has no /dev/fd and a child cannot inherit an arbitrary descriptor (D26),
    // so the pipe-and-/dev/fd/63 approach upstream uses cannot work. D17 passes the path
    // of a named pipe instead (the name of this test is from when it was a temp file).
    assert_eq!(cash("cat <(echo hello)").stdout, "hello");
    assert_eq!(cash(r#"cat <(printf 'a\nb\n')"#).stdout, "a\nb");
}

#[test]
fn process_substitution_supports_the_classic_use() {
    // `diff <(a) <(b)` is why the feature exists. Both operands must be real paths a
    // native program can open.
    assert_eq!(
        cash(r#"diff <(printf '1\n2\n') <(printf '1\n3\n') >/dev/null 2>&1; echo $?"#).stdout,
        "1"
    );
    assert_eq!(
        cash(r#"diff <(printf '1\n2\n') <(printf '1\n2\n') >/dev/null 2>&1; echo $?"#).stdout,
        "0"
    );
}

#[test]
fn process_substitution_works_as_a_redirect() {
    assert_eq!(
        cash(r#"while read -r l; do echo "got:$l"; done < <(printf 'x\ny\n')"#).stdout,
        "got:x\ngot:y"
    );
}

#[test]
fn write_process_substitution_works() {
    // Process substitution supports real-time streaming: `>(...)` connects the
    // consuming subshell via in-memory pipe and receives the stream cleanly.
    //
    // The outer `cat` reads until the substitution has closed its output, here and in
    // Bash. No length of sleep decides the result, as `echo x > >(cat); sleep 0.05` did
    // on a busy machine.
    assert_eq!(cash("{ echo x > >(cat); } | cat").stdout, "x");
    // A consumer slower than the rest of the script is waited for as well, and its
    // output arrives before the script goes on.
    assert_eq!(
        cash("{ echo x > >(sleep 0.2; cat); } | cat; echo after").stdout,
        "x\nafter"
    );
}

#[test]
fn a_write_substitution_still_running_when_the_shell_exits_finishes_first() {
    // Bash's `>(...)` is a process that outlives the shell and still writes. cash's is a
    // thread of the shell, which ended it unfinished at exit: this printed nothing, and
    // about one run in fifteen left a `cash --invoke-bundled cat` suspended for ever,
    // with cash.exe locked (2026-09-30). The shell waits for it now.
    assert_eq!(cash("echo x > >(sleep 1; cat)").stdout, "x");
}

#[test]
fn exec_into_a_write_substitution_loses_nothing_at_exit() {
    // The logging idiom: everything the script writes goes through the substitution,
    // which gets the end of its input only when the shell has gone. The sleep makes it
    // slower than the script, as a real log writer on a busy machine is.
    assert_eq!(
        cash("exec > >(sleep 1; tr a-z A-Z); echo shout; echo more").stdout,
        "SHOUT\nMORE"
    );
}

#[test]
fn a_write_substitution_no_program_opened_does_not_keep_the_shell_from_exiting() {
    // The path is printed, not opened: the substitution waits for a writer that never
    // comes, and is given the end of its input as the shell exits.
    let out = cash(r#"echo >(cat) > /dev/null; echo done"#);
    assert_eq!(out.stdout, "done");
}

#[test]
fn tee_writes_into_a_write_substitution() {
    // `tee >(cmd)` is the classic use of `>(...)`, and it failed: handed a named pipe,
    // tee opened it as a new file, which a pipe does not allow ("The parameter is
    // incorrect"), and `tee -a` appended to it, "Access is denied" (2026-09-30). The
    // bundled tee is patched to open it as the pipe it is.
    let out = cash("echo x | tee >(cat >&2)");
    assert_eq!((out.code, out.stdout.as_str()), (0, "x"), "{}", out.stderr);
    assert_eq!(out.stderr, "x");

    let out = cash("echo y | tee -a >(cat >&2)");
    assert_eq!((out.code, out.stdout.as_str()), (0, "y"), "{}", out.stderr);
    assert_eq!(out.stderr, "y");
}

#[test]
fn the_other_patched_bundled_tools_write_into_a_write_substitution() {
    // `sort` also truncated what it had opened, which Windows allows for a regular file
    // and says a named pipe is.
    let sorted = cash(r"printf 'b\na\n' | sort -o >(cat)");
    assert_eq!(sorted.stdout, "a\nb", "{}", sorted.stderr);
    let unique = cash(r"printf 'a\na\nb\n' | uniq - >(cat)");
    assert_eq!(unique.stdout, "a\nb", "{}", unique.stderr);
    let shuffled = cash(r"printf 'z\n' | shuf -o >(cat)");
    assert_eq!(shuffled.stdout, "z", "{}", shuffled.stderr);
}

#[test]
fn a_program_writes_into_a_write_substitution_through_a_file() {
    // A program opens the file it is to write as a new one and cannot be made to ask
    // otherwise, so it is handed a temp file, which the substitution reads as it is
    // written, and to its end once the program has exited. .NET's `WriteAllText`
    // creates or truncates, and lets others only read the file while it writes.
    let scratch = Scratch::new("procsub-program");
    std::fs::write(
        scratch.path().join("write.ps1"),
        "[IO.File]::WriteAllText($args[0], \"from dotnet`n\")\n",
    )
    .unwrap();
    let out = cash_in_scratch(
        scratch.path(),
        r#"powershell.exe -NoProfile -ExecutionPolicy Bypass -File write.ps1 >(cat)
           echo "status $?""#,
    );
    let mut lines: Vec<&str> = out.stdout.lines().map(str::trim_end).collect();
    lines.sort_unstable();
    assert_eq!(lines, ["from dotnet", "status 0"], "{}", out.stderr);
}

#[test]
#[ignore = "flaky: cp sizes its target before writing it, and the temp file is read as it \
            grows, so a read in between gets zeros (TODO.md)"]
fn an_unpatched_bundled_tool_writes_into_a_write_substitution_through_a_file() {
    // `cp` opens its target as a new file, as a program on PATH does, and gets the file.
    let scratch = Scratch::new("procsub-cp");
    std::fs::write(scratch.path().join("src"), "copied\n").unwrap();
    let out = cash_in_scratch(scratch.path(), "cp src >(cat)");
    assert_eq!(
        (out.code, out.stdout.as_str()),
        (0, "copied"),
        "{}",
        out.stderr
    );
}

#[test]
fn a_function_writes_into_a_write_substitution() {
    // A function may hand the path on to a program or to cash's own redirections, and
    // gets the file, which both can create, truncate and append to.
    assert_eq!(
        cash(r#"f() { echo one > "$1"; echo two >> "$1"; }; f >(cat)"#).stdout,
        "one\ntwo"
    );
}

#[test]
fn a_write_substitution_ends_with_the_command_it_was_handed_to() {
    // `$(...)` reads until everything that could write to it is done, and the
    // substitution could until its input ended; for a path no program opened, that was
    // when the shell exited, so this waited for ever. The input ends with the command,
    // as the last copy of the descriptor is closed in Bash.
    let scratch = Scratch::new("procsub-end");
    let out = cash_in_scratch(scratch.path(), r#"x=$(echo >(cat)); echo "got ${#x}""#);
    assert!(out.stdout.starts_with("got "), "{}", out.stderr);

    // A program's file is read to its end as the program exits, while the script goes
    // on, as Bash's substitution runs beside it.
    let out = cash_in_scratch(
        scratch.path(),
        r#"cmd.exe /d /c "echo late>" >(sleep 0.3; cat > out.txt); sleep 2; cat out.txt"#,
    );
    assert_eq!(out.stdout, "late", "{}", out.stderr);
}

#[test]
fn process_substitutions_leave_no_temp_file_behind() {
    // `diff <(a) <(b)` gets files written before it starts, which were never deleted:
    // 2,614 of them in one user's temp directory on 2026-09-30. Neither is a program's
    // `>(...)` left, nor, once the shell has exited, the directory they were in.
    let scratch = Scratch::new("procsub-temp");
    let out = cash_in_scratch(
        scratch.path(),
        r#"diff <(echo a) <(echo a) && echo same
           cmd.exe /d /c "echo x>" >(cat > /dev/null)
           sleep 0.5; find "$TEMP" -type f"#,
    );
    assert_eq!(out.stdout, "same", "{}", out.stderr);
    let left: Vec<_> = std::fs::read_dir(scratch.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert!(left.is_empty(), "left behind: {left:?}");
}

// ---------------------------------------------------------------------------
// exec — Windows has no execve.
// ---------------------------------------------------------------------------

#[test]
fn exec_with_only_redirections_rebinds_the_shells_files() {
    // The common form, and entirely platform-neutral: it swaps the shell's own open
    // files rather than replacing the process.
    assert_eq!(
        cash(r#"exec > /dev/null; echo discarded; exec 1>&2; echo visible"#).stderr,
        "visible"
    );
}

#[test]
fn exec_replaces_the_shell() {
    // Windows cannot replace a process image, so cash runs the command and exits with
    // its status. Observably the same for a script: nothing after the exec runs.
    let out = cash("exec echo replaced; echo SHOULD-NOT-PRINT");
    assert_eq!(out.stdout, "replaced");
}

#[test]
fn exec_propagates_the_commands_exit_status() {
    assert_eq!(cash("exec cmd.exe /d /s /c exit 7").code, 7);
}

#[test]
fn a_failed_exec_exits_a_non_interactive_shell() {
    // POSIX. Without it a script carries on past a line that was meant to replace it.
    // bash gives 127 here too.
    let out = cash("exec definitely-not-a-command; echo CONTINUED");
    assert_eq!(out.code, 127);
    assert!(
        !out.stdout.contains("CONTINUED"),
        "shell continued past a failed exec"
    );
    assert!(
        out.stderr.contains("not found"),
        "no diagnostic: {}",
        out.stderr
    );
}

// ---------------------------------------------------------------------------
// A program's streams, sent to one of the shell's own.
// ---------------------------------------------------------------------------

/// `tool 2>&1` in a script whose output goes to a log puts the tool's errors in the log.
/// It left them on the shell's standard error: a program was given whichever of the
/// shell's streams had the number being set, so `2>&1` and `>&2` changed nothing for it
/// unless the script had sent the stream somewhere itself.
#[test]
fn a_program_is_given_the_stream_its_own_is_sent_to() {
    let errors_to_output = cash(r#"cmd.exe /d /c "echo an error 1>&2" 2>&1"#);
    assert_eq!(errors_to_output.stdout, "an error");
    assert_eq!(errors_to_output.stderr, "");

    let output_to_errors = cash(r#"cmd.exe /d /c "echo a message" >&2"#);
    assert_eq!(output_to_errors.stdout, "");
    assert_eq!(output_to_errors.stderr, "a message");

    let after_exec = cash(r#"exec 2>&1; cmd.exe /d /c "echo an error 1>&2""#);
    assert_eq!(after_exec.stdout, "an error");
    assert_eq!(after_exec.stderr, "");

    let both = cash(r#"cmd.exe /d /c "echo a message & echo an error 1>&2""#);
    assert_eq!(both.stdout, "a message");
    assert_eq!(both.stderr, "an error");
}

// ---------------------------------------------------------------------------
// Builtins that exist because their absence would break scripts (D2).
// ---------------------------------------------------------------------------

#[test]
fn umask_exists_and_round_trips() {
    // Windows has no umask — permissions come from inherited ACLs (D23). But `umask 022`
    // is a commonplace line, and answering `command not found` kills any script under
    // `set -e`. The value is remembered so a script that sets and re-reads it agrees
    // with itself.
    assert_eq!(cash("umask").stdout, "0022");
    assert_eq!(cash("umask 077; umask").stdout, "0077");
    assert_eq!(cash("umask -p").stdout, "umask 0022");
    assert!(cash("umask -S").stdout.starts_with("u="));
    assert_eq!(
        cash(r#"set -e; umask 022; echo survived"#).stdout,
        "survived"
    );
}

#[test]
fn ulimit_exists_and_reports_unlimited() {
    // Also no Win32 equivalent — and `unlimited` is accurate rather than evasive: there
    // is no per-process descriptor cap on Windows to report.
    assert_eq!(cash("ulimit").stdout, "unlimited");
    assert_eq!(cash("ulimit -n").stdout, "unlimited");
    assert_eq!(
        cash(r#"set -e; ulimit -n 4096; echo survived"#).stdout,
        "survived"
    );
    assert!(cash("ulimit -a").stdout.contains("open files"));
}

// ---------------------------------------------------------------------------
// D37 — Starship. §1 states this as a requirement, not a nice-to-have.
// ---------------------------------------------------------------------------

#[test]
fn a_unix_spelled_command_path_resolves() {
    // D3/D8: a command spelled as a path is a path *cash* resolves, so every accepted
    // spelling works. D4 only forbids rewriting arguments, where cash cannot know which
    // are paths; a command name is unambiguous.
    //
    // This is what makes D37 possible — see below.
    assert_eq!(
        cash(r#""/c/Windows/System32/ping.exe" -n 1 127.0.0.1 >/dev/null && echo ok"#).stdout,
        "ok"
    );
}

#[test]
fn starship_initialises_and_renders_a_prompt() {
    // Skipped rather than failed where starship is absent: CI runners do not have it,
    // and the requirement is about cash not breaking it, not about installing it.
    if cash("command -v starship >/dev/null && echo yes").stdout != "yes" {
        eprintln!("skipped: starship is not installed");
        return;
    }

    // `starship init bash` emits an eval of an absolute path that, on this platform, is
    // Unix-spelled: '/c/Program Files/starship/bin/starship.exe'. Before commands
    // accepted that spelling it failed with `command not found` on a path that plainly
    // existed.
    let init = cash(r#"eval "$(starship init bash)" && echo init-ok"#);
    assert_eq!(
        init.stdout, "init-ok",
        "starship init failed: {}",
        init.stderr
    );

    // Drive one prompt cycle the way the interactive loop does. Starship installs its
    // prompt through PROMPT_COMMAND, which needs the DEBUG trap for command timing —
    // hence D37's dependence on the signal work.
    let rendered = cash(
        r#"eval "$(starship init bash)"; eval "$PROMPT_COMMAND" 2>/dev/null; eval "echo \"$PS1\"""#,
    );
    assert!(
        !rendered.stdout.trim().is_empty(),
        "starship rendered nothing (stderr: {})",
        rendered.stderr
    );
}

// ---------------------------------------------------------------------------
// D6 / D22 — killing a job reaps its tree, not just its root.
// ---------------------------------------------------------------------------

#[test]
fn killing_a_job_reaps_its_whole_tree() {
    // Before per-job containment, `kill %1` reaped only the process cash spawned
    // directly and orphaned everything below it:
    //
    //     before kill : ping=1 cmd=1
    //     after kill  : ping=1 cmd=0
    //
    // The session job eventually caught the orphan, but only when cash itself exited.
    //
    // A nested cash is the middle of the tree, and the grandchild is a ping under a name
    // of its own, looked for by that name: counting `ping` processes would race with
    // other tests, and the grandchild's pid may be another process's three seconds after
    // it dies (see `process_identity.rs`).
    let grandchild = OwnPing::new();
    let (ping, name) = (grandchild.path(), grandchild.name());
    let cash_path = CASH.replace('\\', "/");

    let script = format!(
        r#""{cash_path}" --no-config -c '"{ping}" -n 40 127.0.0.1 >/dev/null & sleep 30' &
sleep 3
pidof {name} >/dev/null && echo started
kill %1
sleep 3
pidof {name} >/dev/null && echo alive || echo dead"#
    );

    let out = cash(&script);
    assert_eq!(
        out.stdout, "started\ndead",
        "the grandchild survived `kill %1`, or never started (stderr: {})",
        out.stderr
    );
}

// ---------------------------------------------------------------------------
// D39 — terminal shell integration.
// ---------------------------------------------------------------------------

#[test]
fn terminal_integration_is_on_by_default() {
    // D39: Windows Terminal and VS Code use OSC 133 and OSC 9;9 for clickable command
    // blocks, jump-to-previous-command, exit-code decorations and cwd-aware new tabs.
    // It shipped behind an off-by-default experimental flag; §1 holds cash to being
    // pleasant enough to replace the user's shell, so it is on.
    let help = run(&["--help".to_string()]);
    assert!(
        help.stdout.contains("enable-terminal-integration"),
        "the option is missing entirely"
    );

    // Explicitly disabling it must still work — the point is the default, not removing
    // the choice.
    let disabled = run(&[
        "--enable-terminal-integration=false".to_string(),
        "-c".to_string(),
        "echo ok".to_string(),
    ]);
    assert_eq!(
        disabled.stdout, "ok",
        "could not opt out: {}",
        disabled.stderr
    );

    let enabled = run(&[
        "--enable-terminal-integration=true".to_string(),
        "-c".to_string(),
        "echo ok".to_string(),
    ]);
    assert_eq!(enabled.stdout, "ok", "could not opt in: {}", enabled.stderr);
}
