//! Background jobs that run inside the shell: builtins, arithmetic, assignments.
//!
//! A background job is a task of the shell's own process. It ran on the runtime's
//! workers, so a loop of builtins held one for good, and as many such jobs as cores left
//! a foreground `$(…)` waiting for ever (EXE-03). `&` waited for the job to report a pid
//! or end a builtin, which a loop of arithmetic and assignments never does, so the `&`
//! itself hung; and `kill %1` had no process to signal, so such a loop could not be
//! ended (EXE-08). A job started inside a process substitution lost what it wrote after
//! the substitution's own command ended (EXE-06).
//!
//! Such a job had an empty `$!`. It now has a number of its own that no process has, and
//! `kill`, `wait` and `jobs -p` know it; and the limit on subshells running at once,
//! which pipeline stages count against too, is 256 and written down (D70, EXE-09).

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::{cash_command, output_of, run, run_in};

/// Runs each script and checks its standard output and status.
fn check(cases: &[(&str, &str)]) {
    for &(script, expected) in cases {
        let out = run(script);
        assert_eq!(
            (out.stdout.as_str(), out.code),
            (expected, 0),
            "{script}: {}",
            out.stderr
        );
    }
}

#[test]
fn a_loop_of_builtins_starts_in_the_background_and_ends_on_kill() {
    // Bash's statuses: 128 plus the signal.
    check(&[
        (
            r#"{ while ((1)); do x=1; done; } & echo started; kill %1; echo "killed $?"; wait %1; echo "waited $?""#,
            "started\nkilled 0\nwaited 143",
        ),
        (
            r#"while :; do :; done & kill -9 %1; wait %1; echo "waited $?""#,
            "waited 137",
        ),
        (
            r#"while :; do :; done & kill -0 %1; echo "zero $?"; kill %1; wait; echo end"#,
            "zero 0\nend",
        ),
        (
            r#"{ while :; do sleep 0.1; done; } & sleep 0.3; kill %1; wait %1; echo "waited $?""#,
            "waited 143",
        ),
        (
            r#"{ x=1; } & wait %1; echo "w $?"; { exit 3; } & wait %1; echo "w $?""#,
            "w 0\nw 3",
        ),
    ]);
}

#[test]
fn busy_background_jobs_leave_the_foreground_free() {
    // More busy jobs than any machine running the tests has cores.
    check(&[(
        r#"for i in $(seq 1 40); do { while :; do :; done; } & done
           echo "fg $(echo hi)"
           for j in $(seq 1 40); do kill %$j; done; wait; echo done"#,
        "fg hi\ndone",
    )]);
}

#[test]
fn a_job_started_in_a_process_substitution_writes_into_it() {
    check(&[(
        "cat <( { sleep 1; echo late; } & echo early )",
        "early\nlate",
    )]);
}

#[test]
fn a_job_that_starts_no_program_is_known_by_a_number_of_its_own() {
    check(&[
        (
            r#"while :; do :; done & p=$!; [ -n "$p" ] && echo has-pid; kill $p; wait $p; echo "waited $?""#,
            "has-pid\nwaited 143",
        ),
        (
            r#"while :; do :; done & p=$!; kill -0 $p; echo "zero $?"; kill $p; wait; kill -0 $p 2>/dev/null; echo "after $?""#,
            "zero 0\nafter 1",
        ),
        (
            r#"while :; do :; done & p=$!; [ "$(jobs -p)" = "$p" ] && echo match; kill %1; wait"#,
            "match",
        ),
        (r#"{ exit 3; } & p=$!; wait $p; echo "w $?""#, "w 3"),
        // Killed while it waits for a program, the job still ends with 143.
        (
            r#"{ :; sleep 5; } & p=$!; [ "$(jobs -p)" = "$p" ] && echo match; kill $p; wait $p; echo "w $?""#,
            "match\nw 143",
        ),
        // No Windows process id is 4n + 1.
        (r"{ x=1; } & echo $(( $! % 4 )); wait", "1"),
    ]);
}

#[test]
fn subshells_at_once_are_limited_to_256() {
    // 135 compound stages at once: past the old limit of 128, which failed them.
    let stages = "{ sleep 1; } | ".repeat(135);
    check(&[(
        &format!("{stages}{{ true; }}; echo \"stages $?\""),
        "stages 0",
    )]);

    // One more than `CASH_MAX_SUBSHELLS` fails as Bash's fork does.
    let mut command = cash_command();
    command.env("CASH_MAX_SUBSHELLS", "2");
    let out = output_of(command.args([
        "-c",
        "{ sleep 1; } & { sleep 1; } & { echo ran; } & wait; echo end",
    ]));
    assert_eq!(out.stdout, "end", "{}", out.stderr);
    assert!(
        out.stderr
            .contains("fork: retry: Resource temporarily unavailable"),
        "{}",
        out.stderr
    );
}

#[test]
fn a_job_is_written_as_bash_writes_it() {
    // A brace group was written over several lines, and its redirections against the
    // brace; Bash's `jobs` writes one line, and `declare -f` keeps the lines (11.6).
    check(&[
        (
            "{ sleep 1; sleep 0; } & { sleep 1 & sleep 1; } & { sleep 1; } > /dev/null & jobs; wait",
            "[1]   Running                    { sleep 1; sleep 0; } &\n\
             [2]-  Running                    { sleep 1 & sleep 1; } &\n\
             [3]+  Running                    { sleep 1; } > /dev/null &",
        ),
        (
            "f() { { echo a; }; g() { :; }; } > /dev/null; declare -f f",
            "f () \n{ \n    { \n        echo a\n    };\n    function g () \n    { \n        :\n    }\n} > /dev/null",
        ),
    ]);
}

#[test]
fn a_builtin_started_with_an_ampersand_lets_the_shell_go_on_at_once() {
    // `&` waited for the job to report a pid or end, and a builtin reported only when it
    // ended: `grep -r … &` held the shell for the whole search. One that starts no
    // command now says at once that no pid is coming. `read -t 2` waits its 2 s in the
    // background.
    let out = run("s=$EPOCHREALTIME; read -t 2 v < <(sleep 5) & \
         t=$EPOCHREALTIME; echo $(( (${t/./} - ${s/./}) < 1000000 )); wait; echo done");
    assert_eq!(
        (out.stdout.as_str(), out.code),
        ("1\ndone", 0),
        "{}",
        out.stderr
    );
}

#[test]
fn a_command_a_builtin_starts_in_the_background_keeps_its_pid() {
    // `nohup server & echo $! > pid`: `$!` is the program's pid, not the job's number,
    // so another shell can signal it. So is a pipeline's, its last command's.
    check(&[
        (
            "nohup sleep 5 >/dev/null 2>&1 & p=$!; (( p < 100000 )) && kill -0 $p && echo pid; kill $p; wait",
            "pid",
        ),
        (
            "echo x | sleep 5 & p=$!; (( p < 100000 )) && kill -0 $p && echo pid; kill $p; wait",
            "pid",
        ),
    ]);
}

#[test]
fn a_compressor_kill_stops_keeps_its_file_and_leaves_no_archive() {
    // A read a stop ended looked like the file's end: gzip and bzip2 wrote half an
    // archive and removed the file, zstd wrote half an archive. A stop now fails the
    // read (and xz's writes, whose threads read ahead), and the tool gives up as on any
    // failed read. The file is 64 GiB of zeros in no disk space, more than any of them
    // gets through in a second. xz runs on one thread: on several, a stop waits for the
    // blocks in flight (TODO.md), seconds under a loaded test run.
    let dir = tempfile::tempdir().unwrap();
    let file = std::fs::File::create(dir.path().join("big")).unwrap();
    cash_win32::pipe::make_sparse(&file).unwrap();
    file.set_len(64 << 30).unwrap();
    drop(file);
    for tool in ["gzip", "bzip2", "xz -T1", "zstd"] {
        let out = run_in(
            dir.path(),
            &format!(r#"{tool} -q big & sleep 1; kill %1; wait %1; echo "$?"; ls -A"#),
        );
        assert_eq!(
            (out.stdout.as_str(), out.stderr.as_str()),
            ("143\nbig", ""),
            "{tool}"
        );
    }
}

#[test]
fn a_compressor_kill_stops_on_standard_input_leaves_a_broken_archive() {
    // Standard input ended at a stop too, and the archive of what came before it was
    // whole, as if that were all; now it lacks its end, as a killed program's does.
    // bzip2 and zstd are not asked: they write a whole stream or frame for each batch of
    // input, as pbzip2 does, so what they wrote before a stop is whole and says so. xz
    // runs on one thread, as above.
    let dir = tempfile::tempdir().unwrap();
    for tool in ["gzip", "xz -T1"] {
        let out = run_in(
            dir.path(),
            &format!(
                r#"{tool} -c < /dev/zero > out & sleep 1; kill %1; wait %1; echo "$?"; {tool} -t out 2>/dev/null || echo broken"#
            ),
        );
        assert_eq!(
            (out.stdout.as_str(), out.stderr.as_str()),
            ("143\nbroken", ""),
            "{tool}"
        );
    }
}

#[test]
fn an_archiver_stopped_in_its_walk_writes_no_archive() {
    // On the shared walker a stop ended the walk, and the archiver went on to write an
    // archive of what it had walked, whole to look at. A stop now fails the run as each
    // tool fails at a Ctrl-C: zip in silence, rar with "User break", 7z with "Break
    // signaled", and none leaves an archive.
    let dir = tempfile::tempdir().unwrap();
    let big = dir.path().join("big");
    std::fs::create_dir(&big).unwrap();
    for n in 0..3000 {
        std::fs::write(big.join(format!("f{n}")), "x").unwrap();
    }
    for (tool, words) in [
        ("zip -rq z.zip big", ""),
        ("rar a -idq r.rar big", "User break"),
        ("7z a -bso0 -bsp0 s.7z big", "Break signaled"),
    ] {
        let out = run_in(
            dir.path(),
            &format!(r#"{tool} & kill %1; wait %1; echo "$?"; ls -A | grep -v '^big$'"#),
        );
        assert_eq!(out.stdout, "143", "{tool}: {}", out.stderr);
        assert!(out.stderr.contains(words), "{tool}: {}", out.stderr);
    }
}

#[test]
fn an_archiver_stopped_in_a_large_file_ends_at_once_and_leaves_no_archive() {
    // tar, zip, rar and 7z read a file they archived to its end before a `kill` or a
    // Ctrl-C was acted on: tar of the file below wrote 55 GB of zeros, until the disk was
    // full. They read it through the stop token now, and fail the run as each fails at a
    // Ctrl-C. The file is 64 GiB of zeros in no disk space; tar writes to standard output,
    // as its archive would be that large, and the rest compress the zeros to nothing.
    let dir = tempfile::tempdir().unwrap();
    let file = std::fs::File::create(dir.path().join("big")).unwrap();
    cash_win32::pipe::make_sparse(&file).unwrap();
    file.set_len(64 << 30).unwrap();
    drop(file);
    let tools = [
        ("tar -cf - big > /dev/null", ""),
        ("zip -q z.zip big", ""),
        ("rar a -idq r.rar big", "User break"),
        ("7z a -bso0 -bsp0 s.7z big", "Break signaled"),
        ("7z h -bso0 -bsp0 big > /dev/null", "Break signaled"),
    ];
    let script = stop_all_at_once(&tools) + "; ls -A | grep -v '^big$' | grep -v '^err'";
    let out = run_in(dir.path(), &script);
    assert_eq!(out.stdout, "143\n143\n143\n143\n143", "{}", out.stderr);
    for (n, (tool, words)) in tools.iter().enumerate() {
        let said = std::fs::read_to_string(dir.path().join(format!("err{n}"))).unwrap();
        assert!(said.contains(words), "{tool}: {said}");
    }
}

/// The script that starts each of `tools` in the background at once, its standard error
/// to `errN`, stops them all a second later, and says how each ended: one run for all,
/// as a run for each took longer than a test may in the CI's slow lane.
fn stop_all_at_once(tools: &[(&str, &str)]) -> String {
    use std::fmt::Write as _;
    let mut script = String::new();
    for (n, (tool, _)) in tools.iter().enumerate() {
        let _ = write!(script, "{tool} 2> err{n} & ");
    }
    script.push_str("sleep 1; kill");
    for n in 1..=tools.len() {
        let _ = write!(script, " %{n}");
    }
    for n in 1..=tools.len() {
        let _ = write!(script, r#"; wait %{n}; echo "$?""#);
    }
    script
}

#[test]
fn an_archiver_stopped_in_a_large_member_ends_at_once() {
    // Reading a member's data asked no stop token in unzip and 7z. The archives hold one
    // member of zeros in no disk space: a tar of 64 GiB, its size in GNU's base-256, and
    // a zip of 3 GiB, stored. Nothing is written: tar and unzip write to standard output,
    // the rest test.
    use std::io::{Seek, SeekFrom, Write};
    let dir = tempfile::tempdir().unwrap();
    let size: u64 = 64 << 30;
    let mut header = [0u8; 512];
    header[..3].copy_from_slice(b"big");
    header[100..108].copy_from_slice(b"0000644\0");
    header[108..116].copy_from_slice(b"0000000\0");
    header[116..124].copy_from_slice(b"0000000\0");
    header[124] = 0x80;
    header[128..136].copy_from_slice(&size.to_be_bytes());
    header[136..148].copy_from_slice(b"00000000000\0");
    header[148..156].copy_from_slice(b"        ");
    header[156] = b'0';
    header[257..265].copy_from_slice(b"ustar  \0");
    let sum: u32 = header.iter().map(|&b| u32::from(b)).sum();
    header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    let mut tar = std::fs::File::create(dir.path().join("big.tar")).unwrap();
    cash_win32::pipe::make_sparse(&tar).unwrap();
    // The zeros after the member's data are also the archive's end.
    tar.set_len(512 + size + 1024).unwrap();
    tar.write_all(&header).unwrap();
    drop(tar);

    let size: u32 = 3 << 30;
    let mut local = 0x0403_4b50_u32.to_le_bytes().to_vec();
    for half in [10_u16, 0, 0, 0, 0x21] {
        local.extend_from_slice(&half.to_le_bytes());
    }
    for word in [0_u32, size, size] {
        local.extend_from_slice(&word.to_le_bytes());
    }
    for half in [3_u16, 0] {
        local.extend_from_slice(&half.to_le_bytes());
    }
    local.extend_from_slice(b"big");
    let central_at = u32::try_from(local.len()).unwrap() + size;
    let mut central = Vec::new();
    central.extend_from_slice(&0x0201_4b50_u32.to_le_bytes());
    for half in [20_u16, 10, 0, 0, 0, 0x21] {
        central.extend_from_slice(&half.to_le_bytes());
    }
    for word in [0_u32, size, size] {
        central.extend_from_slice(&word.to_le_bytes());
    }
    for half in [3_u16, 0, 0, 0, 0] {
        central.extend_from_slice(&half.to_le_bytes());
    }
    for word in [0_u32, 0] {
        central.extend_from_slice(&word.to_le_bytes());
    }
    central.extend_from_slice(b"big");
    let mut end = Vec::new();
    end.extend_from_slice(&0x0605_4b50_u32.to_le_bytes());
    for half in [0_u16, 0, 1, 1] {
        end.extend_from_slice(&half.to_le_bytes());
    }
    for word in [u32::try_from(central.len()).unwrap(), central_at] {
        end.extend_from_slice(&word.to_le_bytes());
    }
    end.extend_from_slice(&0_u16.to_le_bytes());
    let mut zip = std::fs::File::create(dir.path().join("big.zip")).unwrap();
    cash_win32::pipe::make_sparse(&zip).unwrap();
    zip.set_len(u64::from(central_at) + (central.len() + end.len()) as u64)
        .unwrap();
    zip.write_all(&local).unwrap();
    zip.seek(SeekFrom::Start(u64::from(central_at))).unwrap();
    zip.write_all(&central).unwrap();
    zip.write_all(&end).unwrap();
    drop(zip);

    let tools = [
        ("tar -xOf big.tar > /dev/null", ""),
        ("7z t -bso0 -bsp0 big.tar", "Break signaled"),
        ("unzip -p big.zip > /dev/null", ""),
        ("unzip -tq big.zip", ""),
        ("7z t -bso0 -bsp0 big.zip", "Break signaled"),
    ];
    let out = run_in(dir.path(), &stop_all_at_once(&tools));
    assert_eq!(out.stdout, "143\n143\n143\n143\n143", "{}", out.stderr);
    for (n, (tool, words)) in tools.iter().enumerate() {
        let said = std::fs::read_to_string(dir.path().join(format!("err{n}"))).unwrap();
        assert!(said.contains(words), "{tool}: {said}");
    }
}
