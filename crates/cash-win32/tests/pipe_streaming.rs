//! Tests for Win32 Named Pipe streaming process substitution.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::uninlined_format_args,
    reason = "integration tests"
)]

use std::io::{Read, Write};

#[test]
fn test_metadata_probe_then_read() {
    let sub = cash_win32::pipe::create_read_substitution().expect("create read substitution");
    let mut writer = sub.writer;
    let pipe_path = sub.path;

    let subshell = std::thread::spawn(move || {
        writeln!(writer, "hello dual instance").unwrap();
    });

    // 1. Metadata probe (simulating uutils/coreutils is_dir() check)
    let md = std::fs::metadata(&pipe_path);
    assert!(md.is_ok(), "metadata probe on named pipe should succeed");

    // 2. Open and read stream
    let mut client = std::fs::File::open(&pipe_path).expect("open client");
    let mut content = String::new();
    client.read_to_string(&mut content).expect("read content");

    subshell.join().unwrap();

    assert_eq!(content, "hello dual instance\n");
}

#[test]
fn test_direct_read_without_probe() {
    let sub = cash_win32::pipe::create_read_substitution().expect("create read substitution");
    let mut writer = sub.writer;
    let pipe_path = sub.path;

    let subshell = std::thread::spawn(move || {
        writeln!(writer, "hello direct").unwrap();
    });

    // Direct open without metadata probe
    let mut client = std::fs::File::open(&pipe_path).expect("open client");
    let mut content = String::new();
    client.read_to_string(&mut content).expect("read content");

    subshell.join().unwrap();

    assert_eq!(content, "hello direct\n");
}

#[test]
fn test_write_substitution_streaming() {
    let sub = cash_win32::pipe::create_write_substitution().expect("create write substitution");
    let mut reader = sub.reader;
    let pipe_path = sub.path;

    let subshell = std::thread::spawn(move || {
        let mut content = String::new();
        reader.read_to_string(&mut content).unwrap();
        content
    });

    // Producer writes to the named pipe
    {
        let mut client = std::fs::OpenOptions::new()
            .write(true)
            .open(&pipe_path)
            .expect("open write client");
        writeln!(client, "stream to write substitution").unwrap();
    }

    let received = subshell.join().unwrap();
    assert_eq!(received, "stream to write substitution\n");
}
