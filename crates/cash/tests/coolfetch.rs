//! `coolfetch` — the banner, and the reason it is cash's rather than a script's.
//!
//! `neofetch` and its descendants must identify any system, so on Windows they spawn
//! `wmic`, `reg`, `uname` and `df` to find out what they are running on. cash already
//! holds every one of those numbers: the memory figures are `top`'s, the process count is
//! `ps`'s, the account is the one `id` reports, the machine's name is `$HOSTNAME`'s. So
//! this spawns nothing, and the tests below are really asking one question — do the
//! banner and the rest of the shell agree about the machine they are on?

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

struct Output {
    stdout: String,
    stderr: String,
    code: i32,
}

fn cash(script: &str) -> Output {
    let out = Command::new(CASH)
        .args(["-c", script])
        .output()
        .expect("failed to run cash");
    Output {
        stdout: String::from_utf8_lossy(&out.stdout).trim_end().to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

/// The value of one labelled line, e.g. `OS`.
fn field(stdout: &str, label: &str) -> String {
    stdout
        .lines()
        .find_map(|line| {
            let (_, rest) = line
                .split_once(&std::format!("{label}: "))
                .or_else(|| line.split_once(&std::format!("{label}=")))?;
            Some(rest.trim().to_string())
        })
        .unwrap_or_default()
}

#[test]
fn it_is_a_builtin() {
    let out = cash("type coolfetch");
    assert!(
        out.stdout.contains("shell builtin"),
        "coolfetch is not the builtin: {} {}",
        out.stdout,
        out.stderr
    );
}

#[test]
fn it_names_the_account_and_the_machine_the_shell_does() {
    // The first line is `user@host`, and both halves have to be the ones the rest of the
    // shell reports — a banner disagreeing with `id` and `$HOSTNAME` about where it is
    // running would be the §4 #20 problem all over again.
    let out = cash(r#"coolfetch --no-color --no-logo; echo "id=$(id -un)"; echo "host=$HOSTNAME""#);
    let first = out.stdout.lines().next().unwrap_or_default().to_string();
    let (user, host) = first.split_once('@').expect("a user@host line");

    let id = field(&out.stdout, "id");
    let hostname = field(&out.stdout, "host");

    assert!(
        id.ends_with(user),
        "the banner's account is not the shell's: [{user}] [{id}]"
    );
    assert_eq!(
        host, hostname,
        "the banner's machine is not the shell's: [{host}] [{hostname}]"
    );
}

#[test]
fn the_shell_line_carries_the_shells_own_version() {
    // The same version the shell publishes, rather than whichever crate the code happens
    // to live in.
    let out = cash(r#"coolfetch --no-color --no-logo; echo "ver=$BRUSH_VERSION""#);
    let shell = field(&out.stdout, "Shell");
    let version = field(&out.stdout, "ver");

    assert!(!version.is_empty(), "the shell publishes no version");
    assert!(
        shell.ends_with(&version),
        "the banner's version is not the shell's: [{shell}] [{version}]"
    );
}

#[test]
fn the_memory_line_reports_the_system_total_and_percentage() {
    let banner = cash("coolfetch --no-color --no-logo");
    let memory = field(&banner.stdout, "Memory");
    let total = memory.split('/').nth(1).expect("used / total");
    let mut parts = total.split_whitespace();
    let value: f64 = parts.next().unwrap().parse().unwrap();
    let scale = match parts.next().unwrap() {
        "TiB" => 1024f64.powi(4),
        "GiB" => 1024f64.powi(3),
        "MiB" => 1024f64.powi(2),
        unit => panic!("unexpected memory unit: {unit}"),
    };
    #[allow(clippy::cast_precision_loss)]
    let expected = cash_win32::process::total_physical_memory().unwrap() as f64;
    assert!(
        value.mul_add(scale, -expected).abs() <= scale * 0.005,
        "{memory}"
    );
    assert!(memory.ends_with("%)"), "{memory}");
}

#[test]
fn the_process_count_is_in_the_same_region_as_ps() {
    // Not equal: processes start and stop between two snapshots taken seconds apart. A
    // count off by hundreds would mean they are counting different things.
    let banner = cash("coolfetch --no-color --no-logo");
    let counted: i64 = field(&banner.stdout, "Processes").parse().unwrap_or(0);

    let from_ps: i64 = cash("ps -e | wc -l")
        .stdout
        .trim()
        .parse::<i64>()
        .unwrap_or(0)
        - 1;

    assert!(counted > 20, "only {counted} processes on a live machine");
    assert!(
        (counted - from_ps).abs() < 40,
        "coolfetch counted {counted} processes, ps counted {from_ps}"
    );
}

#[test]
fn no_logo_prints_only_the_facts() {
    let out = cash("coolfetch --no-color --no-logo");
    assert!(
        !out.stdout.contains('#'),
        "the logo survived --no-logo: {}",
        out.stdout
    );
    assert!(
        out.stdout.contains("OS: "),
        "the facts are missing: {}",
        out.stdout
    );
}

#[test]
fn lists_all_accessible_drives_regardless_of_working_directory() {
    let drives: Vec<_> = cash_win32::sysinfo::logical_drives()
        .into_iter()
        .filter(|drive| {
            let root = std::path::PathBuf::from(format!("{drive}/"));
            !cash_win32::sysinfo::is_network_drive(&root)
                && cash_win32::sysinfo::disk_usage(&root).is_some()
        })
        .collect();
    assert!(!drives.is_empty(), "expected at least the system drive");

    for cwd in &drives {
        let out = Command::new(CASH)
            .current_dir(format!("{cwd}/"))
            .args(["-c", "coolfetch --no-color --no-logo"])
            .output()
            .expect("failed to run coolfetch from a drive root");
        assert!(out.status.success(), "{out:?}");
        let stdout = String::from_utf8_lossy(&out.stdout);
        for drive in &drives {
            let label = format!("Disk ({drive})");
            assert!(
                !field(&stdout, &label).is_empty(),
                "missing {drive} from {cwd}: {stdout}"
            );
            assert_eq!(stdout.matches(&format!("{label}: ")).count(), 1);
        }
    }
}

#[test]
fn no_color_emits_no_escape_sequences() {
    // And a pipe gets plain text either way, which is what makes the banner safe to
    // capture in a log.
    let out = cash("coolfetch --no-color | cat");
    assert!(
        !out.stdout.contains('\x1b'),
        "an escape sequence survived --no-color"
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
}

#[test]
fn every_line_is_filled_in() {
    // A `?` means a query failed rather than the machine being unusual — the whole point
    // is that these come from calls cash already makes successfully elsewhere.
    let out = cash("coolfetch --no-color --no-logo");
    for label in [
        "OS",
        "Kernel",
        "Uptime",
        "Shell",
        "Terminal",
        "CPU",
        "Memory",
        "Processes",
    ] {
        let value = field(&out.stdout, label);
        assert!(!value.is_empty(), "{label} is missing: {}", out.stdout);
        assert!(value != "?", "{label} could not be determined");
    }
}

#[test]
fn the_logo_sits_beside_os_to_the_first_disk_split_at_terminal() {
    // The layout the user drew: a 20-column gutter, the title and its rule clear of the
    // logo, the panes beside OS through the first disk, and a gap beside Terminal.
    let out = cash("coolfetch --no-color");
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert!(lines.len() >= 11, "{}", out.stdout);
    let pane = "  #######  #######  ";
    let blank = " ".repeat(20);
    for (row, line) in lines.iter().enumerate() {
        let art = if (2..=5).contains(&row) || (7..=10).contains(&row) {
            pane
        } else {
            blank.as_str()
        };
        assert!(line.starts_with(art), "row {row}: {line:?}");
        assert!(
            !line.get(20..).unwrap_or("").starts_with(' '),
            "row {row}: {line:?}"
        );
        assert_eq!(line.trim_end(), *line, "row {row} has trailing spaces");
    }
    assert!(
        lines[2].get(20..).unwrap_or("").starts_with("OS: "),
        "{}",
        out.stdout
    );
    assert!(
        lines[3]
            .get(20..)
            .unwrap_or("")
            .starts_with("Kernel: Windows_NT 10."),
        "{}",
        out.stdout
    );
    assert!(
        lines[6].get(20..).unwrap_or("").starts_with("Terminal: "),
        "{}",
        out.stdout
    );
}
