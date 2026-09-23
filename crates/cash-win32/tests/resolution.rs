//! D8 command resolution, D46's ordering constraint, D32 argument encoding.

#![cfg(windows)]
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

use std::fs;
use std::path::PathBuf;

use cash_win32::cmd::{
    CmdHazard, build_cmd_command_line, build_command_line, escape_for_cmd, is_safe_for_cmd,
    quote_argument,
};
use cash_win32::resolve::{
    DEFAULT_PATHEXT, Dispatch, classify, parse_pathext, read_shebang, resolve,
};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cash-resolve-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn pathext() -> Vec<String> {
    DEFAULT_PATHEXT.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn pathext_is_parsed_and_normalised() {
    let parsed = parse_pathext(".COM;.EXE;.BAT;.CMD;.PY");
    assert_eq!(parsed, vec![".COM", ".EXE", ".BAT", ".CMD", ".PY"]);

    // Tolerate missing dots and lowercase, both of which occur in the wild.
    assert_eq!(parse_pathext("exe;.bat"), vec![".EXE", ".BAT"]);
    assert!(parse_pathext("").is_empty());
}

#[test]
fn a_bare_name_is_found_via_pathext() {
    let dir = scratch("pathext");
    fs::write(dir.join("tool.exe"), b"MZ").unwrap();

    let found =
        resolve("tool", std::slice::from_ref(&dir), &pathext(), &dir).expect("tool resolves");
    assert_eq!(found, Dispatch::Native(dir.join("tool.exe")));
}

#[test]
fn dispatch_follows_extension() {
    let dir = scratch("dispatch");
    for name in ["a.exe", "b.cmd", "c.bat", "d.ps1"] {
        fs::write(dir.join(name), b"x").unwrap();
    }

    assert_eq!(
        classify(&dir.join("a.exe")),
        Dispatch::Native(dir.join("a.exe"))
    );
    assert_eq!(
        classify(&dir.join("b.cmd")),
        Dispatch::Batch(dir.join("b.cmd"))
    );
    assert_eq!(
        classify(&dir.join("c.bat")),
        Dispatch::Batch(dir.join("c.bat"))
    );
    assert_eq!(
        classify(&dir.join("d.ps1")),
        Dispatch::PowerShell(dir.join("d.ps1"))
    );
}

#[test]
fn a_zero_byte_exe_still_dispatches_natively() {
    // D46: App Execution Aliases are 0-byte reparse points. Reading one yields nothing,
    // so resolution must decide from the extension alone. If this regresses, `python`
    // stops working on every machine with the Store aliases enabled — which is all of
    // them by default.
    let dir = scratch("appexeclink");
    fs::write(dir.join("python.exe"), b"").unwrap();

    let found =
        resolve("python", std::slice::from_ref(&dir), &pathext(), &dir).expect("python resolves");
    assert_eq!(found, Dispatch::Native(dir.join("python.exe")));
}

#[test]
fn shebang_scripts_name_their_interpreter() {
    let dir = scratch("shebang");
    fs::write(dir.join("deploy"), b"#!/bin/bash\necho hi\n").unwrap();

    let Dispatch::Shebang {
        interpreter,
        args,
        script,
    } = classify(&dir.join("deploy"))
    else {
        panic!("expected a shebang dispatch");
    };
    assert_eq!(interpreter, "/bin/bash");
    assert!(args.is_empty());
    assert_eq!(script, dir.join("deploy"));
}

#[test]
fn shebang_survives_a_bom_and_crlf() {
    // D41 and D20 together: a script saved by a Windows editor has both, and the
    // shebang is invisible without handling them.
    let dir = scratch("shebang-bom");
    fs::write(
        dir.join("s.sh"),
        b"\xEF\xBB\xBF#!/usr/bin/env bash\r\necho hi\r\n",
    )
    .unwrap();

    let (interpreter, args) = read_shebang(&dir.join("s.sh")).expect("shebang found");
    assert_eq!(interpreter, "/usr/bin/env");
    assert_eq!(args, vec!["bash"]);
}

#[test]
fn shebang_resolves_all_common_interpreters() {
    let dir = scratch("shebang-interp");
    let pe = pathext();

    // 1. /bin/sh -> current_exe
    let (d, args) = cash_win32::resolve::resolve_interpreter("/bin/sh", &[], std::slice::from_ref(&dir), &pe, &dir)
        .expect("/bin/sh resolves");
    assert!(matches!(d, Dispatch::Native(_)));
    assert!(args.is_empty());

    // 2. /bin/bash -> current_exe
    let (d, args) = cash_win32::resolve::resolve_interpreter("/bin/bash", &["-e".to_string()], std::slice::from_ref(&dir), &pe, &dir)
        .expect("/bin/bash resolves");
    assert!(matches!(d, Dispatch::Native(_)));
    assert_eq!(args, vec!["-e"]);

    // 3. /bin/false -> Dispatch::Exit(1)
    let (d, _) = cash_win32::resolve::resolve_interpreter("/bin/false", &[], std::slice::from_ref(&dir), &pe, &dir)
        .expect("/bin/false resolves");
    assert_eq!(d, Dispatch::Exit(1));

    // 4. /bin/true -> Dispatch::Exit(0)
    let (d, _) = cash_win32::resolve::resolve_interpreter("/bin/true", &[], std::slice::from_ref(&dir), &pe, &dir)
        .expect("/bin/true resolves");
    assert_eq!(d, Dispatch::Exit(0));

    // 5. /usr/bin/pwsh -> Dispatch::PowerShell
    fs::write(dir.join("pwsh.exe"), b"MZ").unwrap();
    let (d, _) = cash_win32::resolve::resolve_interpreter("/usr/bin/pwsh", &[], std::slice::from_ref(&dir), &pe, &dir)
        .expect("/usr/bin/pwsh resolves");
    assert!(matches!(d, Dispatch::PowerShell(_)));

    // 6. /usr/bin/env python3 -> resolves to python/python3
    fs::write(dir.join("python.exe"), b"MZ").unwrap();
    let (d, args) = cash_win32::resolve::resolve_interpreter("/usr/bin/env", &["python3".to_string(), "-u".to_string()], std::slice::from_ref(&dir), &pe, &dir)
        .expect("/usr/bin/env python3 resolves");
    assert!(matches!(d, Dispatch::Native(_)));
    assert_eq!(args, vec!["-u"]);
}

#[test]
fn a_path_like_name_is_not_searched_on_path() {
    // POSIX: anything containing a separator is a path, not a PATH lookup.
    let dir = scratch("explicit");
    fs::write(dir.join("here.exe"), b"MZ").unwrap();

    let elsewhere = scratch("elsewhere");
    let as_path = format!("{}/here.exe", dir.to_string_lossy().replace('\\', "/"));
    let found = resolve(&as_path, &[elsewhere], &pathext(), &dir).expect("resolves by path");
    assert_eq!(found.target(), dir.join("here.exe"));
}

#[test]
fn path_entries_are_searched_in_order() {
    let first = scratch("order-1");
    let second = scratch("order-2");
    fs::write(first.join("dup.exe"), b"MZ").unwrap();
    fs::write(second.join("dup.exe"), b"MZ").unwrap();

    let found = resolve("dup", &[first.clone(), second], &pathext(), &first).unwrap();
    assert_eq!(found.target(), first.join("dup.exe"));
}

#[test]
fn a_missing_command_resolves_to_nothing() {
    let dir = scratch("missing");
    assert!(
        resolve(
            "definitely-not-here",
            std::slice::from_ref(&dir),
            &pathext(),
            &dir
        )
        .is_none()
    );
}

// --- D32: argument encoding ---

#[test]
fn simple_arguments_are_not_quoted() {
    assert_eq!(quote_argument("plain"), "plain");
    assert_eq!(quote_argument("--flag=value"), "--flag=value");
}

#[test]
fn spaces_and_quotes_follow_the_crt_rules() {
    assert_eq!(quote_argument("two words"), "\"two words\"");
    assert_eq!(quote_argument(""), "\"\"");
    assert_eq!(quote_argument(r#"say "hi""#), r#""say \"hi\"""#);
}

#[test]
fn backslashes_before_a_quote_are_doubled() {
    // The rule that catches everyone: backslashes are literal EXCEPT before a quote.
    // A trailing backslash therefore only needs doubling when a closing quote follows
    // it — which is to say, only when the argument needed quoting in the first place.
    assert_eq!(
        quote_argument(r"C:\Program Files\"),
        r#""C:\Program Files\\""#
    );
    assert_eq!(quote_argument(r#"a\"b"#), r#""a\\\"b""#);

    // No spaces, no quotes: nothing to escape, because nothing is being quoted.
    assert_eq!(quote_argument(r"C:\dir\"), r"C:\dir\");
}

#[test]
fn a_windows_path_argument_survives_intact() {
    // D4 says cash never rewrites arguments; encoding must not corrupt one either.
    let line = build_command_line("tool.exe", &[r"C:\Program Files\x".to_string()]);
    assert_eq!(line, r#"tool.exe "C:\Program Files\x""#);
}

#[test]
fn cmd_metacharacters_are_caret_escaped() {
    for ch in ['&', '|', '<', '>', '(', ')', '^'] {
        let escaped = escape_for_cmd(&format!("a{ch}b"));
        assert!(escaped.contains('^'), "{ch} was not escaped: {escaped}");
    }
}

#[test]
fn cmd_hazards_are_reported_rather_than_hidden() {
    // D32 accepts that cmd's parser has ambiguous corners and requires the gap to be
    // documented. This makes it checkable instead of merely written down.
    assert_eq!(is_safe_for_cmd("ordinary"), Ok(()));
    assert_eq!(is_safe_for_cmd(r"C:\path\file"), Ok(()));

    assert_eq!(is_safe_for_cmd("%PATH%"), Err(CmdHazard::PercentExpansion));
    assert_eq!(is_safe_for_cmd("two\nlines"), Err(CmdHazard::Newline));
    assert_eq!(is_safe_for_cmd("nul\0byte"), Err(CmdHazard::Nul));
}

#[test]
fn the_cmd_command_line_has_the_expected_shape() {
    let line = build_cmd_command_line("deploy.bat", &["arg".to_string()]);
    assert!(line.starts_with("cmd.exe /d /s /c \""), "got {line}");
    assert!(line.ends_with('"'), "got {line}");
    assert!(line.contains("deploy.bat"));
}
