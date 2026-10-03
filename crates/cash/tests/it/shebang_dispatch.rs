//! Shebang (`#!`) script dispatch — D7 / Windows script execution.
//!
//! Unlike POSIX where the kernel's `execve` interprets `#!`, Windows NT has no in-kernel
//! shebang support. The shell itself must inspect the file header and dispatch to the
//! appropriate interpreter.
//!
//! This integration test suite verifies all major shebang use cases:
//! - `#!/bin/sh`
//! - `#!/bin/bash`
//! - `#!/usr/bin/pwsh`
//! - `#!/usr/bin/env python3`
//! - `#!/bin/false`
//! - `#!/bin/true`
//! - Scripts without shebang lines (POSIX fallback to cash)
//! - Direct invocation and PATH resolution

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::needless_raw_string_hashes,
    clippy::literal_string_with_formatting_args,
    reason = "an integration test is outside a test module by construction"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

const CASH: &str = env!("CARGO_BIN_EXE_cash");

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("cash-shebang-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch");
        Self(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Output {
    stdout: String,
    stderr: String,
    code: i32,
}

fn run_cash_cmd(cmd: &str) -> Output {
    let out = Command::new(CASH)
        .arg("-c")
        .arg(cmd)
        .output()
        .expect("run cash");

    Output {
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

#[test]
fn shebang_bin_sh_executes_with_cash() {
    let scratch = Scratch::new("bin-sh");
    let script = scratch.path().join("tool.sh");
    std::fs::write(&script, b"#!/bin/sh\necho SH_OK: $1 $2\nexit 0\n").expect("write");

    let script_str = script.to_string_lossy().replace('\\', "/");
    let out = run_cash_cmd(&format!("{script_str} foo bar"));
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout.trim(), "SH_OK: foo bar");
}

#[test]
fn shebang_bin_bash_executes_with_cash() {
    let scratch = Scratch::new("bin-bash");
    let script = scratch.path().join("deploy.sh");
    std::fs::write(&script, b"#!/bin/bash\necho BASH_OK: $1 $2\nexit 0\n").expect("write");

    let script_str = script.to_string_lossy().replace('\\', "/");
    let out = run_cash_cmd(&format!("{script_str} alpha beta"));
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout.trim(), "BASH_OK: alpha beta");
}

#[test]
fn shebang_bin_false_exits_one_without_running_body() {
    let scratch = Scratch::new("bin-false");
    let script = scratch.path().join("fail.sh");
    std::fs::write(&script, b"#!/bin/false\necho UNREACHABLE\n").expect("write");

    let script_str = script.to_string_lossy().replace('\\', "/");
    let out = run_cash_cmd(&format!("{script_str}; echo EXIT: $?"));
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(!out.stdout.contains("UNREACHABLE"));
    assert!(out.stdout.contains("EXIT: 1"), "got stdout: {}", out.stdout);
}

#[test]
fn shebang_bin_true_exits_zero() {
    let scratch = Scratch::new("bin-true");
    let script = scratch.path().join("pass.sh");
    std::fs::write(&script, b"#!/bin/true\necho UNREACHABLE\n").expect("write");

    let script_str = script.to_string_lossy().replace('\\', "/");
    let out = run_cash_cmd(&format!("{script_str}; echo EXIT: $?"));
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(!out.stdout.contains("UNREACHABLE"));
    assert!(out.stdout.contains("EXIT: 0"), "got stdout: {}", out.stdout);
}

#[test]
fn shebang_usr_bin_pwsh_executes_powershell() {
    let scratch = Scratch::new("bin-pwsh");
    let script = scratch.path().join("task.sh");
    std::fs::write(
        &script,
        b"#!/usr/bin/pwsh\nparam($a, $b); Write-Host \"PWSH_OK: $a $b\"\n",
    )
    .expect("write");

    let script_str = script.to_string_lossy().replace('\\', "/");
    let out = run_cash_cmd(&format!("{script_str} blue green"));
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout.trim(), "PWSH_OK: blue green");
}

#[test]
fn shebang_usr_bin_env_python3_executes_python() {
    let scratch = Scratch::new("env-python");
    let script = scratch.path().join("calc.py");
    std::fs::write(
        &script,
        b"#!/usr/bin/env python3\nimport sys\nprint(f\"PY_OK: {sys.argv[1]} {sys.argv[2]}\")\n",
    )
    .expect("write");

    let script_str = script.to_string_lossy().replace('\\', "/");
    let out = run_cash_cmd(&format!("{script_str} hello world"));
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout.trim(), "PY_OK: hello world");
}

#[test]
fn script_without_shebang_falls_back_to_posix_shell() {
    let scratch = Scratch::new("no-shebang");
    let script = scratch.path().join("simple.sh");
    std::fs::write(&script, b"echo NOSHEBANG_OK: $1 $2\nexit 0\n").expect("write");

    let script_str = script.to_string_lossy().replace('\\', "/");
    let out = run_cash_cmd(&format!("{script_str} one two"));
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout.trim(), "NOSHEBANG_OK: one two");
}

#[test]
fn shebang_scripts_resolve_from_path() {
    let scratch = Scratch::new("path-resolve");
    let script = scratch.path().join("myscript.sh");
    std::fs::write(&script, b"#!/bin/sh\necho PATH_OK: $1\n").expect("write");

    let dir_str = scratch.path().to_string_lossy().replace('\\', "/");
    let out = run_cash_cmd(&format!("PATH=\"{dir_str}:$PATH\"; myscript.sh success"));
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout.trim(), "PATH_OK: success");
}

#[test]
fn extensionless_script_with_shebang_executes_directly() {
    let scratch = Scratch::new("no-ext-shebang");
    let script = scratch.path().join("mybinary");
    std::fs::write(&script, b"#!/bin/sh\necho NOEXT_SHEBANG_OK: $1\n").expect("write");

    let script_str = script.to_string_lossy().replace('\\', "/");
    let out = run_cash_cmd(&format!("{script_str} hello"));
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout.trim(), "NOEXT_SHEBANG_OK: hello");
}

#[test]
fn extensionless_script_without_shebang_executes_directly() {
    let scratch = Scratch::new("no-ext-noshebang");
    let script = scratch.path().join("plaintool");
    std::fs::write(&script, b"echo NOEXT_NOSHEBANG_OK: $1\n").expect("write");

    let script_str = script.to_string_lossy().replace('\\', "/");
    let out = run_cash_cmd(&format!("{script_str} world"));
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout.trim(), "NOEXT_NOSHEBANG_OK: world");
}

#[test]
fn extensionless_script_resolves_from_path() {
    let scratch = Scratch::new("no-ext-path");
    let script = scratch.path().join("runme");
    std::fs::write(&script, b"#!/bin/sh\necho NOEXT_PATH_OK: $1\n").expect("write");

    let dir_str = scratch.path().to_string_lossy().replace('\\', "/");
    let out = run_cash_cmd(&format!("PATH=\"{dir_str}:$PATH\"; runme verified"));
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout.trim(), "NOEXT_PATH_OK: verified");
}

#[test]
fn env_dash_s_splits_the_shebang_line() {
    // `#!/usr/bin/env -S bash -e` hands `env` one string to split, and cash took `-S` for
    // the command (W32-08). The words are `env`'s: assignments, `-i`, `-u`, the command.
    let scratch = Scratch::new("env-split");
    let dir = scratch.path();
    let scripts: [(&str, &str); 5] = [
        (
            "strict",
            "#!/usr/bin/env -S bash -e\nfalse\necho not reached\n",
        ),
        (
            "greet",
            "#!/usr/bin/env -S GREETING=hi bash\necho \"$GREETING $*\"\n",
        ),
        (
            "quoted",
            "#!/usr/bin/env -S bash -c 'echo \"[$0] [${1##*/}]\"' zero\n",
        ),
        (
            "bare",
            "#!/usr/bin/env -S -i bash\necho \"[${CASH_SHEBANG_MARK-unset}]\"\n",
        ),
        (
            "unclosed",
            "#!/usr/bin/env -S bash 'unclosed\necho not run\n",
        ),
    ];
    for (name, text) in scripts {
        std::fs::write(dir.join(name), text).expect("write");
    }

    let dir_str = dir.to_string_lossy().replace('\\', "/");
    let out = run_cash_cmd(&format!(
        r#"cd '{dir_str}'; export CASH_SHEBANG_MARK=set; ./strict; echo "strict $?"; ./greet a b; ./quoted; ./bare; ./unclosed; echo "unclosed $?""#
    ));
    assert_eq!(
        out.stdout, "strict 1\nhi a b\n[zero] [quoted]\n[unset]\nunclosed 125\n",
        "stderr: {}",
        out.stderr
    );
    assert!(
        out.stderr
            .contains("/usr/bin/env: no terminating quote in -S string"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn a_script_sees_itself_as_it_was_named() {
    // The kernel hands the interpreter the path as typed, so `./sub/z` is `$0`; cash
    // made it absolute. One found on `PATH` came with a `\` from the join.
    let scratch = Scratch::new("named");
    let dir = scratch.path();
    std::fs::create_dir_all(dir.join("sub")).expect("mkdir");
    std::fs::create_dir_all(dir.join("bin")).expect("mkdir");
    std::fs::write(dir.join("sub").join("z"), "#!/bin/bash\necho \"[$0]\"\n").expect("write");
    std::fs::write(dir.join("bin").join("zz"), "#!/bin/bash\necho \"[$0]\"\n").expect("write");

    let dir_str = dir.to_string_lossy().replace('\\', "/");
    let out = run_cash_cmd(&format!(
        r#"cd '{dir_str}'; ./sub/z; sub/z; cd sub; ../sub/z; PATH="{dir_str}/bin:$PATH"; zz"#
    ));
    let rendered = cash_rendered(&dir_str);
    assert_eq!(
        out.stdout,
        format!("[./sub/z]\n[sub/z]\n[../sub/z]\n[{rendered}/bin/zz]\n"),
        "stderr: {}",
        out.stderr
    );
}

/// A Windows path as cash spells it (D3): forward slashes and an upper-case drive.
fn cash_rendered(path: &str) -> String {
    let mut chars = path.chars();
    chars.next().map_or_else(String::new, |drive| {
        format!("{}{}", drive.to_ascii_uppercase(), chars.as_str())
    })
}
