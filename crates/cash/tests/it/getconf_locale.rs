//! `getconf` and `locale`: glibc's interfaces with the values Windows has.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction"
)]

use std::process::{Command, Stdio};

use crate::common::{Output, cash_command, output_of};

/// Runs `script` with standard input closed and `PATH` reduced to System32, so that Git
/// for Windows' own `getconf` and `locale` are not on it; `env` is added.
fn cash(script: &str, env: &[(&str, &str)]) -> Output {
    let mut command = cash_command();
    command
        .args(["-c", script])
        .env("PATH", r"C:\Windows\System32")
        .stdin(Stdio::null());
    for (name, value) in env {
        command.env(name, value);
    }
    for name in ["LANG", "LANGUAGE", "LC_ALL", "LC_TIME", "LC_CTYPE"] {
        if !env.iter().any(|(set, _)| set == &name) {
            command.env_remove(name);
        }
    }
    output_of(&mut command)
}

#[test]
fn getconf_answers_with_the_values_windows_has() {
    let processors = std::thread::available_parallelism().unwrap().get();
    let out = cash(
        "getconf _NPROCESSORS_ONLN; getconf _NPROCESSORS_CONF; getconf PAGESIZE; \
         getconf PAGE_SIZE; getconf LONG_BIT; getconf WORD_BIT; getconf ARG_MAX; \
         getconf NAME_MAX /; getconf CHAR_BIT; getconf INT_MAX; getconf UINT_MAX",
        &[],
    );
    assert_eq!(out.stderr, "");
    assert_eq!(
        out.stdout,
        format!(
            "{processors}\n{processors}\n4096\n4096\n64\n32\n32767\n255\n8\n2147483647\n4294967295"
        )
    );
    let out = cash("getconf PATH_MAX /", &[]);
    assert!(
        out.stdout == "260" || out.stdout == "32767",
        "{}",
        out.stdout
    );
    let out = cash("[ \"$(getconf PATH)\" = \"$PATH\" ] && echo same", &[]);
    assert_eq!(out.stdout, "same");
}

#[test]
fn getconf_a_lists_every_name_in_glibcs_two_columns() {
    let out = cash("getconf -a", &[]);
    assert_eq!((out.code, out.stderr.as_str()), (0, ""));
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert!(lines.len() > 30, "{}", out.stdout);
    let mut names: Vec<&str> = Vec::new();
    for line in &lines {
        let (name, value) = line.split_at(36);
        assert_eq!(name.trim_end().len() + 1, name.trim_end().len() + 1);
        assert!(name.ends_with(' ') && !value.is_empty(), "{line:?}");
        names.push(name.trim_end());
    }
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted);
    for name in [
        "_NPROCESSORS_ONLN",
        "PAGESIZE",
        "PATH",
        "ARG_MAX",
        "LONG_BIT",
    ] {
        assert!(names.contains(&name), "{name} is not listed");
    }
}

#[test]
fn getconf_refuses_what_it_does_not_know_in_glibcs_words() {
    let out = cash(
        "getconf NOPE; echo \"st=$?\"; getconf GNU_LIBC_VERSION; echo \"st=$?\"",
        &[],
    );
    assert_eq!(out.stdout, "st=1\nst=1");
    assert_eq!(
        out.stderr,
        "getconf: Unrecognized variable 'NOPE'\ngetconf: Unrecognized variable 'GNU_LIBC_VERSION'"
    );
    let out = cash("getconf; echo \"st=$?\"", &[]);
    assert_eq!(out.stdout, "st=1");
    assert!(
        out.stderr
            .starts_with("Usage: getconf [-v specification] variable_name [pathname]\n"),
        "{}",
        out.stderr
    );
    let out = cash("getconf --help | head -1; getconf --version", &[]);
    assert!(out.stdout.starts_with("Usage: getconf"), "{}", out.stdout);
    assert!(out.stdout.contains("getconf (cash)"), "{}", out.stdout);
}

#[test]
fn locale_prints_the_environments_view_as_glibc_does() {
    let out = cash("locale", &[]);
    assert_eq!(out.stderr, "");
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines[0], "LANG=");
    assert_eq!(lines[1], "LANGUAGE=");
    assert_eq!(lines[2], "LC_CTYPE=\"C.UTF-8\"");
    assert_eq!(lines[4], "LC_TIME=\"C.UTF-8\"");
    assert_eq!(lines.last().copied(), Some("LC_ALL="));
    assert_eq!(lines.len(), 15);

    let out = cash("locale", &[("LANG", "en_US.UTF-8"), ("LC_TIME", "C")]);
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines[0], "LANG=en_US.UTF-8");
    assert_eq!(lines[2], "LC_CTYPE=\"en_US.UTF-8\"");
    assert_eq!(lines[4], "LC_TIME=C");

    let out = cash("locale", &[("LANG", "en_US.UTF-8"), ("LC_ALL", "C")]);
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines[2], "LC_CTYPE=\"C\"");
    assert_eq!(lines.last().copied(), Some("LC_ALL=C"));
}

#[test]
fn locale_a_lists_c_posix_and_the_windows_locales_by_posix_name() {
    let out = cash("locale -a", &[]);
    assert_eq!(out.stderr, "");
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(&lines[..3], ["C", "C.UTF-8", "POSIX"]);
    assert!(lines.contains(&"en_US.UTF-8"), "{}", out.stdout);
    for name in &lines[3..] {
        let (language, rest) = name
            .split_once('_')
            .expect("a Windows locale name has a language and a region");
        assert!(language.chars().all(|c| c.is_ascii_lowercase()), "{name}");
        assert!(rest.ends_with(".UTF-8"), "{name}");
    }
    let out = cash("locale -m", &[]);
    assert_eq!(out.stdout, "UTF-8");
}

#[test]
fn locale_u_s_f_name_windows_locales_and_definitions_are_refused() {
    let out = cash("locale -uU; locale -s; locale -f", &[]);
    assert_eq!(out.stderr, "");
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.len(), 3);
    assert!(
        lines[0].ends_with(".UTF-8") && lines[0].contains('_'),
        "{}",
        lines[0]
    );
    assert!(
        !lines[1].contains('.') && !lines[2].contains('.'),
        "{}",
        out.stdout
    );

    let out = cash(
        "locale -k LC_CTYPE; echo \"st=$?\"; locale LC_CTYPE; echo \"st=$?\"",
        &[],
    );
    assert_eq!(out.stdout, "st=1\nst=1");
    assert_eq!(
        out.stderr,
        "locale: -k: cash has no locale definitions to show; it has one locale, C.UTF-8 (see 'locale -a')\n\
         locale: LC_CTYPE: cash has no locale definitions to show; it has one locale, C.UTF-8 (see 'locale -a')"
    );
    let out = cash("locale --nope; echo \"st=$?\"", &[]);
    assert_eq!(out.stdout, "st=1");
    assert_eq!(
        out.stderr,
        "locale: unrecognized option '--nope'\nTry 'locale --help' for more information."
    );
}

#[test]
fn both_are_builtins_ahead_of_git_for_windows_copies() {
    let out = output_of(
        Command::new(crate::common::CASH)
            .args(["--no-config", "-c", "type -t getconf locale"])
            .stdin(Stdio::null()),
    );
    assert_eq!(out.stdout, "builtin\nbuiltin");
}
