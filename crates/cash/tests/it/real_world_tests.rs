//! Real-World Open-Source Complex Bash Suites & Differential Tests.
//!
//! Verifies that `cash` accurately runs famous, complex, real-world open-source
//! Bash programs (such as Dominic Tarr's `JSON.sh`, Dylan Araps' `pure-bash-bible`,
//! and upstream GNU Bash test suites), gracefully catches infinite recursions/crashes,
//! and compares differential behavior with GNU Bash in WSL2 Kali Linux.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::needless_raw_string_hashes,
    clippy::uninlined_format_args,
    reason = "integration tests abort loudly on unexpected failures"
)]

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const CASH: &str = env!("CARGO_BIN_EXE_cash");

#[derive(Debug, PartialEq, Eq)]
struct Output {
    stdout: String,
    stderr: String,
    code: i32,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("manifest parent")
        .parent()
        .expect("repo root")
        .to_path_buf()
}

fn cash_eval(script: &str) -> Output {
    let out = Command::new(CASH)
        .args(["-c", script])
        .output()
        .expect("failed to execute cash");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout)
            .replace("\r\n", "\n")
            .trim_end()
            .to_string(),
        stderr: String::from_utf8_lossy(&out.stderr)
            .replace("\r\n", "\n")
            .trim_end()
            .to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

/// Runs the script at `script_path` on `input`, with `dir` searched first for commands
/// if it exists.
fn cash_stdin_with_path_first(script_path: &Path, input: &str, dir: &Path) -> Output {
    let mut command = Command::new(CASH);
    if dir.is_dir() {
        let mut entries = vec![dir.to_path_buf()];
        entries.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        command.env("PATH", std::env::join_paths(entries).expect("join PATH"));
    }
    run_cash_stdin(command, script_path, input)
}

fn run_cash_stdin(mut command: Command, script_path: &Path, input: &str) -> Output {
    let mut child = command
        .arg(script_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn cash");

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes());
    }

    let out = child.wait_with_output().expect("failed to wait on cash");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout)
            .replace("\r\n", "\n")
            .trim_end()
            .to_string(),
        stderr: String::from_utf8_lossy(&out.stderr)
            .replace("\r\n", "\n")
            .trim_end()
            .to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

fn wsl_kali_stdin(script_path: &Path, input: &str) -> Option<Output> {
    // Convert Windows path to WSL /mnt path
    let path_str = script_path.to_str()?.replace('\\', "/");
    let wsl_path = if let Some(stripped) = path_str.strip_prefix("c:/") {
        format!("/mnt/c/{stripped}")
    } else if let Some(stripped) = path_str.strip_prefix("C:/") {
        format!("/mnt/c/{stripped}")
    } else {
        path_str
    };

    let mut child = Command::new("wsl.exe")
        .args(["-d", "kali-linux", "bash", &wsl_path])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes());
    }

    let out = child.wait_with_output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(Output {
        stdout: String::from_utf8_lossy(&out.stdout)
            .replace("\r\n", "\n")
            .trim_end()
            .to_string(),
        stderr: String::from_utf8_lossy(&out.stderr)
            .replace("\r\n", "\n")
            .trim_end()
            .to_string(),
        code: out.status.code().unwrap_or(-1),
    })
}

// ---------------------------------------------------------------------------
// 1. Real-World Suite: Dominic Tarr's JSON.sh
// ---------------------------------------------------------------------------

#[test]
fn test_real_world_dominictarr_json_sh() {
    let script_path = repo_root().join("crates/cash/tests/real_world/JSON.sh");
    assert!(
        script_path.exists(),
        "JSON.sh not found at {:?}",
        script_path
    );

    let complex_json = r#"{
  "name": "cash",
  "version": "0.8.0",
  "boolean_true": true,
  "boolean_false": false,
  "null_val": null,
  "nested": {
    "num": 42,
    "arr": ["one", "two", 3]
  }
}"#;

    // JSON.sh tokenizes with the host's `egrep`. Git for Windows' is an MSYS2 program,
    // which decodes its command line by Cygwin's rules; its tools go first on PATH so
    // that path is what is tested, as on GitHub's runner, rather than whatever native
    // grep a developer's machine happens to find first.
    let git_usr_bin = Path::new(r"C:\Program Files\Git\usr\bin");
    let cash_out = cash_stdin_with_path_first(&script_path, complex_json, git_usr_bin);
    assert_eq!(cash_out.code, 0, "cash stderr: {}", cash_out.stderr);
    assert!(cash_out.stdout.contains("[\"name\"]\t\"cash\""));
    assert!(cash_out.stdout.contains("[\"nested\",\"num\"]\t42"));
    assert!(cash_out.stdout.contains("[\"nested\",\"arr\",0]\t\"one\""));
    assert!(cash_out.stdout.contains("[\"nested\",\"arr\",2]\t3"));

    // Differential test against WSL2 Kali Linux GNU Bash if available
    if let Some(kali_out) = wsl_kali_stdin(&script_path, complex_json) {
        assert_eq!(
            cash_out.stdout, kali_out.stdout,
            "cash output must match WSL2 Kali GNU Bash output byte-for-byte"
        );
        assert_eq!(cash_out.code, kali_out.code);
    }
}

// ---------------------------------------------------------------------------
// 2. Real-World Suite: Dylan Araps' pure-bash-bible
// ---------------------------------------------------------------------------

#[test]
fn test_real_world_pure_bash_bible_suite() {
    let run_script = repo_root().join("crates/cash/tests/real_world/pbb/run_tests.sh");
    assert!(
        run_script.exists(),
        "run_tests.sh not found at {:?}",
        run_script
    );

    let out = Command::new(CASH)
        .arg(&run_script)
        .output()
        .expect("failed to run pure-bash-bible runner");

    let stdout = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&out.stderr).replace("\r\n", "\n");

    assert_eq!(out.status.code().unwrap_or(-1), 0, "stderr: {}", stderr);
    assert!(
        stdout.contains("ALL TESTS COMPLETED: pass=10 fail=0"),
        "pure-bash-bible should pass 10/10 tests, got:\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("reverse_case"),
        "reverse_case (${{var~~}}) must run"
    );
    assert!(
        !stdout.contains("✖"),
        "no tests in pure-bash-bible should fail"
    );
}

// ---------------------------------------------------------------------------
// 3. Upstream GNU Bash Official Tests
// ---------------------------------------------------------------------------

#[test]
fn test_gnu_bash_strip_tests() {
    let test_file = repo_root().join("crates/cash/tests/gnu_bash_tests/strip.tests");
    let right_file = repo_root().join("crates/cash/tests/gnu_bash_tests/strip.right");
    assert!(test_file.exists());
    assert!(right_file.exists());

    let out = Command::new(CASH)
        .arg(&test_file)
        .current_dir(test_file.parent().unwrap())
        .output()
        .expect("failed to run strip.tests");

    let stdout = String::from_utf8_lossy(&out.stdout)
        .replace("\r\n", "\n")
        .trim_end()
        .to_string();
    let expected = fs::read_to_string(&right_file)
        .expect("read strip.right")
        .replace("\r\n", "\n")
        .trim_end()
        .to_string();

    assert_eq!(out.status.code().unwrap_or(-1), 0);
    assert_eq!(
        stdout, expected,
        "strip.tests must match strip.right exactly"
    );
}

#[test]
fn test_gnu_bash_herestr_tests() {
    let test_file = repo_root().join("crates/cash/tests/gnu_bash_tests/herestr.tests");
    let right_file = repo_root().join("crates/cash/tests/gnu_bash_tests/herestr.right");
    assert!(test_file.exists());
    assert!(right_file.exists());

    let out = Command::new(CASH)
        .arg(&test_file)
        .current_dir(test_file.parent().unwrap())
        .output()
        .expect("failed to run herestr.tests");

    let stdout = String::from_utf8_lossy(&out.stdout)
        .replace("\r\n", "\n")
        .trim_end()
        .to_string();
    let expected = fs::read_to_string(&right_file)
        .expect("read herestr.right")
        .replace("\r\n", "\n")
        .trim_end()
        .to_string();

    assert_eq!(out.status.code().unwrap_or(-1), 0);
    assert_eq!(
        stdout, expected,
        "herestr.tests must match herestr.right exactly"
    );
}

#[test]
fn test_gnu_bash_casemod_tests() {
    let test_file = repo_root().join("crates/cash/tests/gnu_bash_tests/casemod.tests");
    assert!(test_file.exists());

    let out = Command::new(CASH)
        .arg(&test_file)
        .current_dir(test_file.parent().unwrap())
        .output()
        .expect("failed to run casemod.tests");

    let stdout = String::from_utf8_lossy(&out.stdout)
        .replace("\r\n", "\n")
        .trim_end()
        .to_string();
    assert_eq!(out.status.code().unwrap_or(-1), 0);

    // Verify key case transformation outputs
    assert!(stdout.contains("ACKNOWLEDGEMENT"));
    assert!(stdout.contains("oENOPHILE"));
    assert!(stdout.contains("oeNoPHiLe"));
    assert!(stdout.contains("BE CONSERVATIVE IN WHAT YOU SEND AND LIBERAL IN WHAT YOU ACCEPT"));
    assert!(stdout.contains("ABCDEXYZ"));
}

// ---------------------------------------------------------------------------
// 4. Infinite Recursion, Stack Overflow & Crash Prevention
// ---------------------------------------------------------------------------

#[test]
fn test_crash_prevention_arithmetic_self_recursion() {
    // x="x+1"; echo $((x)) triggers variable expansion loops in arithmetic.
    // Must gracefully error out with exit code 1, never crashing or overflowing stack.
    let out = cash_eval(r#"x="x+1"; echo $((x))"#);
    assert_eq!(out.code, 1);
    assert!(
        out.stderr.contains("expression recursion level exceeded"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn test_crash_prevention_arithmetic_mutual_recursion() {
    // x="y"; y="x"; echo $((x))
    let out = cash_eval(r#"x="y"; y="x"; echo $((x))"#);
    assert_eq!(out.code, 1);
    assert!(
        out.stderr.contains("expression recursion level exceeded"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn test_crash_prevention_funcnest_limit() {
    // Infinite function recursion guarded by FUNCNEST=25
    let out = cash_eval(r#"FUNCNEST=25; recurse() { recurse; }; recurse"#);
    assert_eq!(out.code, 1);
    assert!(
        out.stderr.contains("maximum function call depth exceeded"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn test_crash_prevention_deep_arithmetic_loop() {
    // Large arithmetic loop should execute without memory exhaustion or stack overflow
    let out = cash_eval(
        r#"
sum=0
for (( i = 0; i < 5000; i++ )); do
    (( sum += 1 ))
done
echo "sum=$sum"
"#,
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "sum=5000");
}

// ---------------------------------------------------------------------------
// 5. External Tool Pipelines: Scoop / Windows awk & sed
// ---------------------------------------------------------------------------

#[test]
fn test_external_awk_and_sed_pipeline_combos() {
    // Only run if awk and sed exist in PATH (e.g. from Scoop or MSYS2)
    let awk_exists = Command::new("where.exe")
        .arg("awk")
        .output()
        .is_ok_and(|o| o.status.success());
    let sed_exists = Command::new("where.exe")
        .arg("sed")
        .output()
        .is_ok_and(|o| o.status.success());
    if !awk_exists || !sed_exists {
        return;
    }

    // 1. Multi-stage pipeline with awk, sed, and while-read loop
    let script = r#"
printf "%s\n" "apple 5 red" "banana 12 yellow" "cherry 8 red" "date 20 brown" | \
  awk '$2 >= 8 {print $1, $2, $3}' | \
  sed 's/red/crimson/' | \
  awk '{print toupper($1), $2 * 10, $3}' | \
  while read -r name score color; do
    echo "ITEM: $name (score: $score, color: $color)"
  done
"#;
    let out = cash_eval(script);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "ITEM: BANANA (score: 120, color: yellow)\nITEM: CHERRY (score: 80, color: crimson)\nITEM: DATE (score: 200, color: brown)"
    );

    // 2. Heredoc piped into sed and awk
    let heredoc_script = r#"
cat << 'EOF' | sed 's/foo/bar/g' | awk '{print NF, $0}'
foo foo foo
foo and foo
plain line
EOF
"#;
    let out2 = cash_eval(heredoc_script);
    assert_eq!(out2.code, 0, "stderr: {}", out2.stderr);
    assert_eq!(out2.stdout, "3 bar bar bar\n3 bar and bar\n2 plain line");

    // 3. Process substitution with awk and sed
    let procsub_script = r#"cat <(printf "%s\n" 1 2 3 | awk '{print $1 * 10}') <(printf "%s\n" x y z | sed 's/./&_item/')"#;
    let out3 = cash_eval(procsub_script);
    assert_eq!(out3.code, 0, "stderr: {}", out3.stderr);
    assert_eq!(out3.stdout, "10\n20\n30\nx_item\ny_item\nz_item");
}

// ---------------------------------------------------------------------------
// 6. Fork Bomb & Runaway Concurrency Safety
// ---------------------------------------------------------------------------

#[test]
fn test_fork_bomb_resource_limit() {
    // The classic Bash fork bomb: :(){ :|:& };:
    // In GNU Bash, this triggers: -bash: fork: retry: Resource temporarily unavailable
    // In Cash on Windows, we enforce SubshellSlotGuard to limit concurrent subshells / tasks
    // and prevent host OS thread or process starvation, outputting the exact same message.
    let out = cash_eval(":(){ :|:& }; :");
    assert!(
        out.stderr
            .contains("fork: retry: Resource temporarily unavailable"),
        "expected fork retry error in stderr, got: {}",
        out.stderr
    );
}
