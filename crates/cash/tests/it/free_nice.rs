//! `free`, `nice` and `renice`: procps' layout on Windows' memory counters, and the
//! priority classes a niceness picks.
//!
//! `free`'s numbers are the machine's, so the tests hold the layout (procps-ng 4.0.7's,
//! read off its output) and the arithmetic: the columns add up, and nothing exceeds the
//! total. `nice` and `renice` are checked against the class Windows reports for the
//! process afterwards, through `cash_win32::priority` and once through PowerShell's
//! `Get-Process`, an oracle that owes nothing to cash.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly"
)]

use std::os::windows::process::CommandExt as _;
use std::process::{Child, Command, Stdio};

use cash_win32::priority::{PriorityClass, of_process, set_process};

use crate::common::{Output, cash_command, output_of, run as cash};

/// A console of its own, without a window, so no signal can reach the test runner.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// procps's header, which every `free` report starts with.
const HEADER: &str =
    "               total        used        free      shared  buff/cache   available";
/// With `-w`.
const WIDE_HEADER: &str =
    "               total        used        free      shared     buffers       cache   available";

/// The numbers in a row of `free`, after its label.
fn numbers(line: &str) -> Vec<u64> {
    line.split_whitespace()
        .skip(1)
        .map(|field| {
            field
                .parse()
                .unwrap_or_else(|_| panic!("not a number: {field:?} in {line:?}"))
        })
        .collect()
}

/// The row starting with `label`.
fn row<'a>(stdout: &'a str, label: &str) -> &'a str {
    stdout
        .lines()
        .find(|line| line.starts_with(label))
        .unwrap_or_else(|| panic!("no {label} row in:\n{stdout}"))
}

// ---------------------------------------------------------------------------
// free
// ---------------------------------------------------------------------------

#[test]
fn free_is_a_builtin_with_procps_layout() {
    let out = cash("type free; free");
    assert!(
        out.stdout.contains("free is a shell builtin"),
        "{}",
        out.stdout
    );
    let lines: Vec<&str> = out.stdout.lines().skip(1).collect();
    assert_eq!(lines.len(), 3, "{}", out.stdout);
    assert_eq!(lines[0], HEADER);
    assert!(lines[1].starts_with("Mem:"), "{}", lines[1]);
    assert!(lines[2].starts_with("Swap:"), "{}", lines[2]);
    // Every column is 12 wide, the label 9: the header and the rows line up.
    assert_eq!(lines[1].len(), HEADER.len());
    assert_eq!(lines[2].len(), 9 + 3 * 12 - 1);
    assert_eq!(numbers(lines[1]).len(), 6);
    assert_eq!(numbers(lines[2]).len(), 3);
}

#[test]
fn the_columns_add_up_in_bytes() {
    let out = cash("free -b");
    let mem = numbers(row(&out.stdout, "Mem:"));
    let [total, used, free, shared, cache, available] = mem[..] else {
        panic!("{mem:?}");
    };
    assert!(total > 1 << 30, "less than a gibibyte of memory: {total}");
    assert_eq!(used + free + cache, total, "{}", out.stdout);
    assert_eq!(used, total - available, "{}", out.stdout);
    assert!(available <= total);
    assert_eq!(shared, 0);
    let swap = numbers(row(&out.stdout, "Swap:"));
    assert_eq!(swap[1] + swap[2], swap[0], "{}", out.stdout);
    // Bytes are 1024 times the default kibibytes, give or take the truncation.
    let kib = numbers(row(&cash("free").stdout, "Mem:"))[0];
    assert!(kib * 1024 <= total && total < (kib + 1) * 1024 + (1 << 20));
}

#[test]
fn t_adds_a_total_row_that_is_the_sum() {
    let out = cash("free -t -k");
    let mem = numbers(row(&out.stdout, "Mem:"));
    let swap = numbers(row(&out.stdout, "Swap:"));
    let total = numbers(row(&out.stdout, "Total:"));
    assert_eq!(
        total,
        vec![mem[0] + swap[0], mem[1] + swap[1], mem[2] + swap[2]]
    );
    assert_eq!(out.stdout.lines().count(), 4);
}

#[test]
fn h_picks_a_unit_per_number() {
    let out = cash("free -h");
    let mem = row(&out.stdout, "Mem:");
    let fields: Vec<&str> = mem.split_whitespace().skip(1).collect();
    assert_eq!(fields.len(), 6, "{mem}");
    assert!(
        fields[0].ends_with("Gi") || fields[0].ends_with("Ti"),
        "total is not in gibibytes: {mem}"
    );
    for field in &fields {
        assert!(
            field.ends_with("Ki")
                || field.ends_with("Mi")
                || field.ends_with("Gi")
                || field.ends_with("Ti")
                || *field == "0B",
            "{field} in {mem}"
        );
    }
    assert_eq!(mem.len(), HEADER.len(), "{mem}");
    // --si: thousands, and no `i`.
    let si_out = cash("free -h --si");
    let si = row(&si_out.stdout, "Mem:");
    let fields: Vec<&str> = si.split_whitespace().skip(1).collect();
    assert!(
        fields[0].ends_with('G') || fields[0].ends_with('T'),
        "total is not in gigabytes: {si}"
    );
    assert!(!si.contains("Gi") && !si.contains("Mi"), "{si}");
}

#[test]
fn m_and_g_are_the_default_divided() {
    let kib = numbers(row(&cash("free").stdout, "Mem:"))[0];
    let mib = numbers(row(&cash("free -m").stdout, "Mem:"))[0];
    let gib = numbers(row(&cash("free -g").stdout, "Mem:"))[0];
    assert_eq!(mib, kib / 1024);
    assert_eq!(gib, kib / 1024 / 1024);
    let bytes = numbers(row(&cash("free --bytes").stdout, "Mem:"))[0];
    let kilo = numbers(row(&cash("free --kilo").stdout, "Mem:"))[0];
    assert_eq!(kilo, bytes / 1000);
}

#[test]
fn w_splits_buffers_from_cache() {
    let out = cash("free -w");
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines[0], WIDE_HEADER);
    let mem = numbers(lines[1]);
    assert_eq!(mem.len(), 7, "{}", lines[1]);
    assert_eq!(lines[1].len(), WIDE_HEADER.len());
    let plain = numbers(row(&cash("free -b").stdout, "Mem:"));
    let wide = numbers(row(&cash("free -w -b").stdout, "Mem:"));
    // buff/cache is buffers + cache; the rest of the columns are the same figures
    // (the machine moves a little between two runs).
    let close = |a: u64, b: u64| a.abs_diff(b) < (1 << 28);
    assert!(close(wide[4] + wide[5], plain[4]), "{wide:?} vs {plain:?}");
    assert_eq!(wide[0], plain[0]);
}

#[test]
fn l_and_v_add_the_low_high_and_commit_rows() {
    let out = cash("free -l -v");
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(
        lines
            .iter()
            .map(|l| l.split(':').next().unwrap())
            .collect::<Vec<_>>(),
        [
            "               total        used        free      shared  buff/cache   available",
            "Mem",
            "Low",
            "High",
            "Swap",
            "Comm"
        ]
    );
    let mem = numbers(lines[1]);
    let low = numbers(lines[2]);
    assert_eq!(low, vec![mem[0], mem[0] - mem[2], mem[2]]);
    assert_eq!(numbers(lines[3]), vec![0, 0, 0]);
    let comm = numbers(lines[5]);
    assert_eq!(comm.len(), 3);
    assert_eq!(comm[1] + comm[2], comm[0], "{}", lines[5]);
    assert!(comm[1] > 0, "nothing committed: {}", lines[5]);
}

#[test]
fn big_l_puts_the_figures_on_one_line() {
    let out = cash("free -L");
    assert_eq!(out.stdout.lines().count(), 1);
    let fields: Vec<&str> = out.stdout.split_whitespace().collect();
    assert_eq!(fields.len(), 8, "{}", out.stdout);
    assert_eq!(
        [fields[0], fields[2], fields[4], fields[6]],
        ["SwapUse", "CachUse", "MemUse", "MemFree"]
    );
    for value in [fields[1], fields[3], fields[5], fields[7]] {
        value
            .parse::<u64>()
            .unwrap_or_else(|_| panic!("{value} in {}", out.stdout));
    }
}

#[test]
fn s_and_c_repeat_with_a_blank_line_between() {
    let out = cash("free -s 0.1 -c 2");
    let blocks: Vec<&str> = out.stdout.split("\n\n").collect();
    assert_eq!(blocks.len(), 2, "{}", out.stdout);
    for block in blocks {
        assert_eq!(block.lines().count(), 3, "{block}");
        assert_eq!(block.lines().next().unwrap(), HEADER);
    }
    // `-c` alone repeats every second; `-c 1` is one report.
    let once = cash("free -c 1 -s 0.1");
    assert_eq!(once.stdout.lines().count(), 3);
}

#[test]
fn free_help_version_and_mistakes() {
    let help = cash("free --help");
    assert_eq!(help.code, 0);
    assert!(
        help.stdout.starts_with("Usage:\n free [options]"),
        "{}",
        help.stdout
    );
    assert!(
        help.stdout
            .contains(" -s N, --seconds N   repeat printing every N seconds")
    );
    assert_eq!(help.stderr, "");

    let version = cash("free -V");
    assert_eq!(version.stdout, "free (cash): procps-ng 4.0.7's options");

    let bad = cash("free -x");
    assert_eq!(bad.code, 1);
    assert_eq!(bad.stdout, "");
    assert!(
        bad.stderr
            .starts_with("free: invalid option -- 'x'\n\nUsage:"),
        "{}",
        bad.stderr
    );
    let units = cash("free -b -k");
    assert_eq!(units.code, 1);
    assert_eq!(
        units.stderr,
        "free: Multiple unit options don't make sense."
    );
    let count = cash("free -c 0");
    assert_eq!(count.code, 1);
    assert_eq!(
        count.stderr,
        "free: failed to parse count argument: '0': Numerical result out of range"
    );
    let seconds = cash("free -s abc");
    assert_eq!(seconds.code, 1);
    assert_eq!(
        seconds.stderr,
        "free: seconds argument failed: 'abc': Invalid argument"
    );
    let operand = cash("free extra");
    assert_eq!(operand.code, 1);
    assert!(operand.stderr.starts_with("\nUsage:"), "{}", operand.stderr);
}

// ---------------------------------------------------------------------------
// nice
// ---------------------------------------------------------------------------

/// Runs `script` in a cash started with the test's own class, which is normal unless the
/// test runner itself was started under `nice`.
fn at_normal(script: &str) -> Output {
    let _held = cash_win32::priority::hold(PriorityClass::Normal).expect("own class");
    cash(script)
}

#[test]
fn nice_alone_prints_the_shells_niceness() {
    // Under `nice -n 10`, the builtin reports the class the outer one asked for, as GNU's
    // reports the niceness of the process the outer one adjusted.
    let out = at_normal("type nice renice; nice -n 10 nice; nice -n -5 nice; nice");
    assert!(
        out.stdout.contains("nice is a shell builtin"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("renice is a shell builtin"),
        "{}",
        out.stdout
    );
    assert_eq!(
        out.stdout.lines().rev().take(3).collect::<Vec<_>>(),
        ["0", "-5", "5"]
    );
    assert_eq!(out.code, 0);
}

#[test]
fn a_command_runs_in_the_class_its_niceness_picks_and_the_shell_gets_its_own_back() {
    // The program is created in the class, so a second cash reports what the first set:
    // each niceness read back as the middle of its class.
    let out = at_normal(
        "nice -n 10 cash --no-config -c nice; \
         nice cash --no-config -c nice; \
         nice -n 19 cash --no-config -c nice; \
         nice -n -5 cash --no-config -c nice; \
         nice --adjustment=-20 cash --no-config -c nice; \
         nice -10 cash --no-config -c nice; \
         nice -n 2 cash --no-config -c nice; \
         nice -n 100 cash --no-config -c nice; \
         nice",
    );
    assert_eq!(
        out.stdout.lines().collect::<Vec<_>>(),
        ["5", "5", "15", "-5", "-15", "5", "5", "15", "0"],
        "{}",
        out.stderr
    );
}

#[test]
fn nice_nests_by_adding_to_what_it_has() {
    // Below normal reads back as 5; 5 + 10 is idle; idle plus anything stays idle.
    let out = at_normal(
        "nice -n 10 cash --no-config -c 'nice -n 10 cash --no-config -c nice'; \
         nice -n 10 cash --no-config -c 'nice -n -5 cash --no-config -c nice'",
    );
    assert_eq!(
        out.stdout.lines().collect::<Vec<_>>(),
        ["15", "0"],
        "{}",
        out.stderr
    );
}

#[test]
fn powershell_sees_the_class_nice_asked_for() {
    // The oracle that owes nothing to cash: Windows PowerShell's `Get-Process`.
    let out = at_normal(
        "nice -n 10 powershell -NoProfile -Command '(Get-Process -Id $PID).PriorityClass'; \
         nice -n -5 powershell -NoProfile -Command '(Get-Process -Id $PID).PriorityClass'; \
         nice -n 19 powershell -NoProfile -Command '(Get-Process -Id $PID).PriorityClass'; \
         nice --adjustment=-20 powershell -NoProfile -Command '(Get-Process -Id $PID).PriorityClass'",
    );
    assert_eq!(
        out.stdout.lines().map(str::trim).collect::<Vec<_>>(),
        ["BelowNormal", "AboveNormal", "Idle", "High"],
        "{}",
        out.stderr
    );
}

#[test]
fn a_builtin_and_a_pipeline_run_under_nice_too() {
    let out =
        at_normal("nice -n 10 echo hi; echo one | nice -n 5 cat; nice -n 3 false; echo \"rc=$?\"");
    assert_eq!(
        out.stdout.lines().collect::<Vec<_>>(),
        ["hi", "one", "rc=1"],
        "{}",
        out.stderr
    );
}

#[test]
fn nice_exit_statuses_and_messages_are_gnus() {
    let no_command = cash("nice -n 3");
    assert_eq!(no_command.code, 125);
    assert_eq!(
        no_command.stderr,
        "nice: a command must be given with an adjustment\nTry 'nice --help' for more information."
    );
    let bad = cash("nice -n abc true");
    assert_eq!(bad.code, 125);
    assert_eq!(bad.stderr, "nice: invalid adjustment 'abc'");
    let option = cash("nice -x true");
    assert_eq!(option.code, 125);
    assert_eq!(
        option.stderr,
        "nice: invalid option -- 'x'\nTry 'nice --help' for more information."
    );
    let missing = cash("nice no-such-command-here");
    assert_eq!(missing.code, 127, "{}", missing.stderr);
    assert!(
        missing
            .stderr
            .contains("nice: 'no-such-command-here': No such file or directory"),
        "{}",
        missing.stderr
    );
    let status = cash("nice -n 5 cash --no-config -c 'exit 7'");
    assert_eq!(status.code, 7);

    let help = cash("nice --help");
    assert_eq!(help.code, 0);
    assert!(
        help.stdout
            .starts_with("Usage: nice [OPTION] [COMMAND [ARG]...]"),
        "{}",
        help.stdout
    );
    assert!(help.stdout.contains("-n, --adjustment=N"));
    let version = cash("nice --version");
    assert_eq!(
        version.stdout,
        "nice (cash): GNU coreutils 9.11's options, on the Windows priority classes"
    );
}

// ---------------------------------------------------------------------------
// renice
// ---------------------------------------------------------------------------

/// A `ping` of the test's own to renice, in a console of its own, ended when dropped.
struct Target(Child);

impl Target {
    fn start() -> Self {
        let child = Command::new("ping.exe")
            .args(["-n", "60", "127.0.0.1"])
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start ping.exe");
        // A runner may start its jobs below normal, and the child inherits that; the
        // tests count from Normal, as util-linux's do from 0.
        set_process(child.id(), PriorityClass::Normal).expect("set the target to Normal");
        Self(child)
    }

    fn pid(&self) -> u32 {
        self.0.id()
    }

    fn class(&self) -> PriorityClass {
        of_process(self.pid()).expect("the target's class")
    }
}

impl Drop for Target {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn renice_sets_the_class_and_reports_old_and_new() {
    let target = Target::start();
    let pid = target.pid();
    assert_eq!(target.class(), PriorityClass::Normal);

    let out = cash(&format!("renice -n 5 -p {pid}"));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        format!("{pid} (process ID) old priority 0, new priority 5")
    );
    assert_eq!(target.class(), PriorityClass::BelowNormal);

    // The old syntax, a bare pid, --priority and --relative; a value out of range is
    // brought into it, and 7 reads back as 5, the middle of below normal.
    let out = cash(&format!(
        "renice +7 -p {pid}; renice 19 {pid}; renice --priority -3 {pid}; \
         renice --relative 5 {pid}; renice -n -30 -p {pid}; renice -n 0 {pid}"
    ));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(
        out.stdout.lines().collect::<Vec<_>>(),
        [
            format!("{pid} (process ID) old priority 5, new priority 5"),
            format!("{pid} (process ID) old priority 5, new priority 15"),
            format!("{pid} (process ID) old priority 15, new priority -5"),
            format!("{pid} (process ID) old priority -5, new priority 0"),
            format!("{pid} (process ID) old priority 0, new priority -15"),
            format!("{pid} (process ID) old priority -15, new priority 0"),
        ]
    );
    assert_eq!(target.class(), PriorityClass::Normal);
}

#[test]
fn renice_u_reaches_the_users_own_processes() {
    // `-u` reaches every process of the account, the developer's editor and the other
    // tests' children included, so this asks for a relative change of nothing: each
    // process is set to the class it has, and the ping of our own is among those listed.
    let target = Target::start();
    let pid = target.pid();
    let out = cash(&format!(
        "me=$(id -un); renice --relative 0 -u \"$me\" 2>/dev/null | grep '^{pid} '"
    ));
    assert_eq!(
        out.stdout,
        format!("{pid} (process ID) old priority 0, new priority 0"),
        "{}",
        out.stderr
    );
    assert_eq!(target.class(), PriorityClass::Normal);
    // The account as `id -un` prints it, `DOMAIN\user`, and the bare name both work.
    let bare = cash(&format!(
        "me=$(id -un); renice --relative 0 -u \"${{me##*\\\\}}\" 2>/dev/null | grep -c '^{pid} '"
    ));
    assert_eq!(bare.stdout, "1", "{}", bare.stderr);

    let unknown = cash("renice 5 -u no-such-user-here");
    assert_eq!(unknown.code, 1);
    assert_eq!(unknown.stderr, "renice: unknown user no-such-user-here");
}

#[test]
fn renice_refusals_are_util_linuxs() {
    let missing = cash("renice 5 -p 999999");
    assert_eq!(missing.code, 1);
    assert_eq!(
        missing.stderr,
        "renice: failed to get priority for 999999 (process ID): No such process"
    );
    assert_eq!(missing.stdout, "");

    // The System process: Windows protects it from everyone, elevated or not.
    let system = cash("renice -n 5 -p 4");
    assert_eq!(system.code, 1, "{}", system.stdout);
    assert!(
        system
            .stderr
            .ends_with("for 4 (process ID): Permission denied"),
        "{}",
        system.stderr
    );

    let group = cash("renice 5 -g 1");
    assert_eq!(group.code, 1);
    assert!(
        group
            .stderr
            .starts_with("renice: -g 1: Windows has no process groups"),
        "{}",
        group.stderr
    );

    let few = cash("renice -n 5");
    assert_eq!(few.code, 1);
    assert_eq!(
        few.stderr,
        "renice: not enough arguments\nTry 'renice --help' for more information."
    );
    let none = cash("renice");
    assert_eq!(none.code, 1);
    assert!(none.stderr.starts_with("renice: not enough arguments"));
    let priority = cash("renice abc 5");
    assert_eq!(priority.code, 1);
    assert_eq!(
        priority.stderr,
        "renice: invalid priority 'abc'\nTry 'renice --help' for more information."
    );
    let pid = cash("renice 5 abc");
    assert_eq!(pid.code, 1);
    assert_eq!(pid.stderr, "renice: bad process ID value: abc");

    // One failure among the ids fails the command, after the rest were done.
    let target = Target::start();
    let mixed = cash(&format!("renice 5 -p 999999 {}", target.pid()));
    assert_eq!(mixed.code, 1);
    assert!(
        mixed.stdout.ends_with("old priority 0, new priority 5"),
        "{}",
        mixed.stdout
    );
    assert_eq!(target.class(), PriorityClass::BelowNormal);
}

#[test]
fn renice_help_and_version() {
    let help = cash("renice --help");
    assert_eq!(help.code, 0);
    // util-linux's help starts with an empty line.
    assert!(
        help.stdout.starts_with(
            "\nUsage:\n renice [-n|--priority|--relative] <priority> [-p|--pid] <pid>..."
        ),
        "{}",
        help.stdout
    );
    let short = cash("renice -h");
    assert_eq!(short.stdout, help.stdout);
    let version = cash("renice -V");
    assert_eq!(
        version.stdout,
        "renice (cash): util-linux 2.42.3's options, on the Windows priority classes"
    );
}

#[test]
fn help_pages_cover_free_nice_and_renice() {
    for (name, needle) in [
        ("free", "procps' layout"),
        ("nice", "six priority classes"),
        ("renice", "six priority classes"),
    ] {
        let out = output_of(cash_command().args(["-c", &format!("help {name}")]));
        assert_eq!(out.code, 0, "{}", out.stderr);
        assert!(out.stdout.contains(needle), "help {name}:\n{}", out.stdout);
    }
}
