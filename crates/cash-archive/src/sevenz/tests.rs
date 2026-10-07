//! The 7z module against 7-Zip 26.03's own archives (tests/fixtures/7z, made by
//! Scoop's 7-Zip from the same small tree), and round trips of what it writes.

use std::io::{Cursor, Read, Write};

use super::{
    Archive, ArchiveEntry, ArchiveReader, ArchiveWriter, EncoderConfiguration, EncoderMethod,
    Error, Password, Problem,
    options::{AesEncoderOptions, EncoderOptions, LzmaParams},
};

macro_rules! fixture {
    ($name:literal) => {
        include_bytes!(concat!("../../tests/fixtures/7z/", $name))
    };
}

const FIXTURES: [(&str, &[u8]); 11] = [
    ("stored", fixture!("stored.7z")),
    ("solid", fixture!("solid.7z")),
    ("aes", fixture!("aes.7z")),
    ("ppmd", fixture!("ppmd.7z")),
    ("bzip2", fixture!("bzip2.7z")),
    ("deflate", fixture!("deflate.7z")),
    ("deflate64", fixture!("deflate64.7z")),
    ("lzma", fixture!("lzma.7z")),
    ("delta", fixture!("delta.7z")),
    ("arm64", fixture!("arm64.7z")),
    ("bcj2", fixture!("bcj2.7z")),
];

/// The fixtures' `text.txt`.
fn text() -> Vec<u8> {
    use std::fmt::Write as _;
    let mut text = String::new();
    for i in 0..400 {
        let _ = writeln!(
            text,
            "line {i:05}: the quick brown fox jumps over the lazy dog {}",
            i * 7919 % 1000
        );
    }
    text.into_bytes()
}

fn expected(name: &str) -> Option<Vec<u8>> {
    match name {
        "d/a.txt" => Some(b"hello\n".to_vec()),
        "d/sub/b.txt" => Some(b"world wide\n".to_vec()),
        "d/empty" => Some(Vec::new()),
        "text.txt" => Some(text()),
        _ => None,
    }
}

/// Every entry's data, or its problem.
fn contents(bytes: &[u8], password: &str) -> Vec<(String, Result<Vec<u8>, Problem>)> {
    let mut reader = ArchiveReader::new(Cursor::new(bytes), Password::from(password)).unwrap();
    let mut out = Vec::new();
    reader
        .for_each_entries(&|_| true, |_, entry, data| {
            let mut bytes = Vec::new();
            let _ = data.read_to_end(&mut bytes);
            out.push((entry.name.clone(), data.finish().map(|()| bytes)));
            Ok::<_, ()>(true)
        })
        .unwrap();
    out
}

#[test]
fn every_7_zip_fixture_reads_back_its_files() {
    for (name, bytes) in FIXTURES {
        let password = if name == "aes" { "secret" } else { "" };
        let entries = contents(bytes, password);
        assert!(!entries.is_empty(), "{name}");
        for (entry, data) in entries {
            if let Some(want) = expected(&entry) {
                assert_eq!(data.as_deref(), Ok(want.as_slice()), "{name}: {entry}");
            }
        }
    }
}

#[test]
fn the_facts_7_zip_lists() {
    let archive =
        Archive::read(&mut Cursor::new(fixture!("stored.7z")), &Password::empty()).unwrap();
    let names: Vec<&str> = archive.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["d", "d/sub", "d/empty", "d/a.txt", "d/sub/b.txt"]);
    assert_eq!(archive.physical_size, fixture!("stored.7z").len() as u64);
    assert_eq!(archive.packed_size(), 17);
    assert_eq!(archive.header_size, archive.physical_size - 17);
    assert!(archive.header_blocks.is_empty());
    assert_eq!(archive.version, (0, 4));
    assert!(archive.files[0].is_directory);
    assert!(!archive.files[2].is_directory && !archive.files[2].has_stream);
}

#[test]
fn an_encrypted_header_wants_its_password() {
    let bytes = fixture!("aes.7z");
    assert!(matches!(
        Archive::read(&mut Cursor::new(bytes), &Password::empty()),
        Err(Error::PasswordRequired)
    ));
    assert!(Archive::read(&mut Cursor::new(bytes), &Password::from("wrong")).is_err());
    let archive = Archive::read(&mut Cursor::new(bytes), &Password::from("secret")).unwrap();
    assert!(archive.header_encrypted());
    assert!(archive.blocks.iter().all(super::Block::is_encrypted));
}

/// The stored fixture, entry for entry: 7-Zip's `-mx0 -mhc=off` archive, which the
/// writer must make byte for byte from the same entries.
#[test]
fn a_stored_archive_is_7_zips_byte_for_byte() {
    let bytes = fixture!("stored.7z");
    let archive = Archive::read(&mut Cursor::new(bytes), &Password::empty()).unwrap();
    let mut writer = ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
    writer.set_content_methods(vec![EncoderConfiguration::new(EncoderMethod::COPY)]);
    writer.set_compress_header(false);
    for entry in &archive.files {
        let mut entry = entry.clone();
        entry.compressed_size = 0;
        if entry.has_stream {
            let data = expected(&entry.name).unwrap();
            writer
                .push_block(vec![entry], vec![data.as_slice()])
                .unwrap();
        } else {
            writer.push_empty(entry);
        }
    }
    let written = writer.finish().unwrap().into_inner();
    assert_eq!(written, bytes);
}

fn entry(name: &str) -> ArchiveEntry {
    let mut entry = ArchiveEntry::new_file(name);
    entry.has_last_modified_date = true;
    entry.last_modified_date = super::NtTime::new(134_000_000_000_000_000);
    entry.has_windows_attributes = true;
    entry.windows_attributes = 0x20;
    entry
}

fn aes(password: &str) -> EncoderConfiguration {
    let options = AesEncoderOptions::new(Password::from(password)).unwrap();
    EncoderConfiguration::new(EncoderMethod::AES256_SHA256)
        .with_options(EncoderOptions::Aes(options))
}

#[test]
fn solid_threaded_lzma2_with_aes_and_an_encrypted_header_round_trips() {
    let big: Vec<u8> = text().repeat(40);
    let mut params = LzmaParams::with_preset(5);
    params.dict_size = 64 << 10;
    let lzma2 =
        EncoderConfiguration::new(EncoderMethod::LZMA2).with_options(EncoderOptions::Lzma2 {
            params,
            threads: 4,
            chunk_size: 128 << 10,
        });
    let mut writer = ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
    writer.set_content_methods(vec![
        aes("pw"),
        lzma2,
        EncoderConfiguration::new(EncoderMethod::BCJ_X86_FILTER),
    ]);
    writer.set_header_encryption(Some(aes("pw")));
    let mut folder = ArchiveEntry::new_directory("dir");
    folder.has_windows_attributes = true;
    folder.windows_attributes = 0x10;
    writer.push_empty(folder);
    writer
        .push_block(
            vec![entry("dir/big.txt"), entry("dir/small.txt")],
            vec![big.as_slice(), b"small\n".as_slice()],
        )
        .unwrap();
    let bytes = writer.finish().unwrap().into_inner();

    assert!(Archive::read(&mut Cursor::new(&bytes), &Password::empty()).is_err());
    let read = contents(&bytes, "pw");
    assert_eq!(read.len(), 3);
    assert_eq!(read[1], ("dir/big.txt".to_string(), Ok(big)));
    assert_eq!(
        read[2],
        ("dir/small.txt".to_string(), Ok(b"small\n".to_vec()))
    );
}

#[test]
fn copied_blocks_keep_their_data() {
    let bytes = fixture!("solid.7z");
    let mut reader = ArchiveReader::new(Cursor::new(bytes.as_slice()), Password::empty()).unwrap();
    let archive = reader.archive().clone();
    let mut writer = ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
    for (index, file) in archive.files.iter().enumerate() {
        if !file.has_stream {
            writer.push_empty(file.clone());
        } else if archive.stream_map.block_files[0].first() == Some(&index) {
            let mut entries: Vec<ArchiveEntry> = archive.stream_map.block_files[0]
                .iter()
                .map(|&i| archive.files[i].clone())
                .collect();
            entries[0].name = "renamed.txt".to_string();
            writer
                .push_copied_block(entries, &archive.blocks[0], |out| {
                    reader.copy_packed(0, out)
                })
                .unwrap();
        }
    }
    let copied = writer.finish().unwrap().into_inner();
    let before = contents(bytes, "");
    let after = contents(&copied, "");
    assert_eq!(before.len(), after.len());
    for ((name, data), (new_name, new_data)) in before.iter().zip(&after) {
        assert_eq!(data, new_data, "{name}");
        assert!(name == new_name || new_name == "renamed.txt");
    }
}

#[test]
fn damaged_data_is_an_entrys_problem() {
    let mut bytes = fixture!("stored.7z").to_vec();
    // The stored data starts after the start header: "hello\n" first.
    bytes[32] ^= 1;
    let read = contents(&bytes, "");
    let a = read.iter().find(|(name, _)| name == "d/a.txt").unwrap();
    assert_eq!(a.1, Err(Problem::Crc));
    let b = read.iter().find(|(name, _)| name == "d/sub/b.txt").unwrap();
    assert_eq!(b.1.as_deref(), Ok(b"world wide\n".as_slice()));
}

#[test]
fn a_wrong_password_is_a_data_problem() {
    let mut writer = ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
    writer.set_content_methods(vec![
        aes("right"),
        EncoderConfiguration::new(EncoderMethod::LZMA2),
    ]);
    writer
        .push_block(vec![entry("a.txt")], vec![text().as_slice()])
        .unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    let read = contents(&bytes, "wrong");
    assert!(matches!(
        read[0].1,
        Err(Problem::Data | Problem::Crc | Problem::UnexpectedEnd)
    ));
    let none = contents(&bytes, "");
    assert_eq!(none[0].1, Err(Problem::PasswordNeeded));
}

#[test]
fn truncated_and_flipped_archives_never_panic() {
    for (name, bytes) in FIXTURES {
        let password = if name == "aes" { "secret" } else { "" };
        for cut in (0..bytes.len()).step_by(7) {
            if let Ok(mut reader) =
                ArchiveReader::new(Cursor::new(&bytes[..cut]), Password::from(password))
            {
                let _ = reader.for_each_entries(&|_| true, |_, _, data| {
                    let _ = std::io::copy(data, &mut std::io::sink());
                    Ok::<_, ()>(true)
                });
            }
        }
        for at in (0..bytes.len()).step_by(3) {
            let mut flipped = bytes.to_vec();
            flipped[at] ^= 0x55;
            if let Ok(mut reader) =
                ArchiveReader::new(Cursor::new(&flipped), Password::from(password))
            {
                let _ = reader.for_each_entries(&|_| true, |_, _, data| {
                    let _ = std::io::copy(data, &mut std::io::sink());
                    Ok::<_, ()>(true)
                });
            }
        }
    }
}

#[test]
fn an_empty_archive_is_its_start_header() {
    let writer = ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    assert_eq!(bytes.len(), 32);
    let archive = Archive::read(&mut Cursor::new(&bytes), &Password::empty()).unwrap();
    assert!(archive.files.is_empty());
}

#[test]
fn chains_finish_into_any_writer() {
    // `Write` for the cursor is all the writer needs besides `Seek`.
    let mut out = Cursor::new(Vec::new());
    out.write_all(b"prefix").unwrap();
    let mut writer = ArchiveWriter::new(out).unwrap();
    writer
        .push_block(vec![entry("x")], vec![b"x".as_slice()])
        .unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    assert_eq!(&bytes[..6], b"prefix");
    let archive = Archive::read(&mut Cursor::new(&bytes[6..]), &Password::empty()).unwrap();
    assert_eq!(archive.files.len(), 1);
}

#[test]
#[ignore = "writes a file for 7-Zip to look at"]
fn dump_for_7_zip() {
    let dir = std::env::var("DUMP_7Z").unwrap();
    let mut writer = ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
    writer
        .push_block(vec![entry("x")], vec![b"x".as_slice()])
        .unwrap();
    std::fs::write(
        format!("{dir}/lzma2.7z"),
        writer.finish().unwrap().into_inner(),
    )
    .unwrap();
    let mut writer = ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
    writer.set_compress_header(false);
    writer
        .push_block(vec![entry("x")], vec![b"x".as_slice()])
        .unwrap();
    std::fs::write(
        format!("{dir}/lzma2plain.7z"),
        writer.finish().unwrap().into_inner(),
    )
    .unwrap();

    let big: Vec<u8> = text().repeat(40);
    let mut params = LzmaParams::with_preset(5);
    params.dict_size = 64 << 10;
    let lzma2 =
        EncoderConfiguration::new(EncoderMethod::LZMA2).with_options(EncoderOptions::Lzma2 {
            params,
            threads: 4,
            chunk_size: 128 << 10,
        });
    let mut writer = ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
    writer.set_content_methods(vec![
        aes("pw"),
        lzma2,
        EncoderConfiguration::new(EncoderMethod::BCJ_X86_FILTER),
    ]);
    writer.set_header_encryption(Some(aes("pw")));
    writer.push_empty(ArchiveEntry::new_directory("dir"));
    writer
        .push_block(
            vec![entry("dir/big.txt"), entry("dir/small.txt")],
            vec![big.as_slice(), b"small\n".as_slice()],
        )
        .unwrap();
    std::fs::write(
        format!("{dir}/aes.7z"),
        writer.finish().unwrap().into_inner(),
    )
    .unwrap();

    for (name, methods) in [
        ("ppmd", vec![EncoderConfiguration::new(EncoderMethod::PPMD)]),
        (
            "bzip2",
            vec![EncoderConfiguration::new(EncoderMethod::BZIP2)],
        ),
        (
            "deflate",
            vec![EncoderConfiguration::new(EncoderMethod::DEFLATE)],
        ),
        ("lzma", vec![EncoderConfiguration::new(EncoderMethod::LZMA)]),
        (
            "delta",
            vec![
                EncoderConfiguration::new(EncoderMethod::LZMA2),
                EncoderConfiguration::new(EncoderMethod::DELTA_FILTER)
                    .with_options(EncoderOptions::Delta { distance: 4 }),
            ],
        ),
    ] {
        let mut writer = ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
        writer.set_content_methods(methods);
        writer
            .push_block(vec![entry("text.txt")], vec![big.as_slice()])
            .unwrap();
        std::fs::write(
            format!("{dir}/{name}.7z"),
            writer.finish().unwrap().into_inner(),
        )
        .unwrap();
    }
}
