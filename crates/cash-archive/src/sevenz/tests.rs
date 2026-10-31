//! The 7z module against 7-Zip 26.03's own archives (tests/fixtures/7z, made by
//! Scoop's 7-Zip from the same small tree), and round trips of what it writes.

use std::io::{Cursor, Read, Write};

use super::{
    Archive, ArchiveEntry, ArchiveReader, ArchiveWriter, EncoderConfiguration, EncoderMethod,
    Error, Password, Problem,
    archive::{
        K_CODERS_UNPACK_SIZE, K_END, K_FILES_INFO, K_FOLDER, K_HEADER, K_MAIN_STREAMS_INFO,
        K_NUM_UNPACK_STREAM, K_PACK_INFO, K_SIZE, K_SUB_STREAMS_INFO, K_UNPACK_INFO,
    },
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
    let mut directory = ArchiveEntry::new_directory("dir");
    directory.has_windows_attributes = true;
    directory.windows_attributes = 0x10;
    writer.push_empty(directory);
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

/// An empty entry standing between the files of a solid block (upstream sevenz-rust2
/// 0.24.2 lost the block's last file over it): the block covers all of its files.
#[test]
fn an_empty_entry_between_the_files_of_a_solid_block_hides_none() {
    let bytes = fixture!("interleaved_empty_entry.7z");
    let read = contents(bytes, "");
    let names: Vec<&str> = read.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["d.txt", "a.txt", "b.txt", "c.txt"]);
    let sizes: Vec<usize> = read
        .iter()
        .map(|(_, data)| data.as_ref().unwrap().len())
        .collect();
    assert_eq!(sizes, [1000, 0, 1000, 1000]);
    let archive = Archive::read(&mut Cursor::new(bytes), &Password::empty()).unwrap();
    assert_eq!(archive.stream_map.block_files, [vec![0, 2, 3]]);
}

/// A reader that leaves an entry of a solid block unread, or half read, still gets the
/// next entry's own data.
#[test]
fn an_unread_entry_of_a_solid_block_leaves_the_next_one_whole() {
    let bytes = fixture!("solid.7z");
    let archive = Archive::read(&mut Cursor::new(bytes), &Password::empty()).unwrap();
    let block_of = |name: &str| {
        let index = archive.files.iter().position(|f| f.name == name).unwrap();
        archive.stream_map.file_block_index[index]
    };
    assert_eq!(block_of("d/a.txt"), block_of("text.txt"));
    for read_len in [0, 3] {
        let mut reader =
            ArchiveReader::new(Cursor::new(bytes.as_slice()), Password::empty()).unwrap();
        let mut got = None;
        reader
            .for_each_entries(&|_| true, |_, entry, data| {
                if entry.name == "d/a.txt" {
                    let mut buf = vec![0; read_len];
                    data.read_exact(&mut buf).unwrap();
                } else if entry.name == "text.txt" {
                    let mut buf = Vec::new();
                    data.read_to_end(&mut buf).unwrap();
                    got = Some(buf);
                }
                Ok::<_, ()>(true)
            })
            .unwrap();
        assert_eq!(got, Some(text()), "after {read_len} bytes of d/a.txt");
    }
}

/// Writes `first` and then 20 directories, and reads the archive back: the bit vectors
/// with one bit for every entry without a stream span 3 bytes (upstream 0.24.2 wrote them
/// short after the last empty file or anti item).
fn written_with_directories_after(first: ArchiveEntry) -> Archive {
    let mut writer = ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
    writer.push_empty(first);
    for i in 0..20 {
        writer.push_empty(ArchiveEntry::new_directory(&format!("dir{i:02}")));
    }
    let bytes = writer.finish().unwrap().into_inner();
    Archive::read(&mut Cursor::new(&bytes), &Password::empty()).unwrap()
}

#[test]
fn directories_after_the_last_empty_file_or_anti_item_keep_their_names() {
    let archive = written_with_directories_after(ArchiveEntry::new_file("empty.txt"));
    assert_eq!(archive.files.len(), 21);
    assert_eq!(archive.files[0].name, "empty.txt");
    assert!(!archive.files[0].is_directory && !archive.files[0].has_stream);
    assert!(archive.files[1..].iter().all(|f| f.is_directory));
    assert_eq!(archive.files[20].name, "dir19");

    let mut anti = ArchiveEntry::new_directory("removed");
    anti.is_anti_item = true;
    let archive = written_with_directories_after(anti);
    assert_eq!(archive.files.len(), 21);
    assert_eq!(archive.files[0].name, "removed");
    assert!(archive.files[0].is_anti_item && archive.files[0].is_directory);
    assert!(
        archive.files[1..]
            .iter()
            .all(|f| f.is_directory && !f.is_anti_item)
    );
    assert_eq!(archive.files[20].name, "dir19");
}

/// Writes `value` as the format's NUMBER.
fn number(out: &mut Vec<u8>, value: u64) {
    let mut first: u8 = 0;
    let mut mask: u8 = 0x80;
    let mut low = Vec::new();
    for i in 0..8u32 {
        if value < (1u64 << (7 * (i + 1))) {
            first |= (value >> (8 * i)) as u8;
            break;
        }
        first |= mask;
        mask >>= 1;
        low.push((value >> (8 * i)) as u8);
    }
    out.push(first);
    out.extend_from_slice(&low);
}

/// A 7z file: the signature header, `packed` and then `header` as the next header.
fn raw_7z(packed: &[u8], header: &[u8]) -> Vec<u8> {
    let mut start = Vec::with_capacity(20);
    start.extend_from_slice(&(packed.len() as u64).to_le_bytes());
    start.extend_from_slice(&(header.len() as u64).to_le_bytes());
    start.extend_from_slice(&crc32fast::hash(header).to_le_bytes());
    let mut out = vec![0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0x00, 0x04];
    out.extend_from_slice(&crc32fast::hash(&start).to_le_bytes());
    out.extend_from_slice(&start);
    out.extend_from_slice(packed);
    out.extend_from_slice(header);
    out
}

fn opens(bytes: &[u8]) -> Result<ArchiveReader<Cursor<&[u8]>>, Error> {
    ArchiveReader::new(Cursor::new(bytes), Password::empty())
}

/// Opens the archive and reads every entry: `Ok` when all of it decodes.
fn decodes(bytes: &[u8]) -> Result<(), String> {
    let mut reader = opens(bytes).map_err(|e| e.to_string())?;
    let mut problems = Vec::new();
    reader
        .for_each_entries(&|_| true, |_, entry, data| {
            let _ = std::io::copy(data, &mut std::io::sink());
            if let Err(problem) = data.finish() {
                problems.push(format!("{}: {problem:?}", entry.name));
            }
            Ok::<_, ()>(true)
        })
        .map_err(|()| String::from("stopped early"))?;
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join(", "))
    }
}

/// A header with streams info but no files info is rejected, as 7-Zip rejects it
/// (upstream 0.24.2 panicked on it), plain or in an encoded header.
#[test]
fn streams_without_files_info_are_rejected() {
    let headerless: [&[u8]; 2] = [
        fixture!("streams_without_files_info.bin"),
        fixture!("encoded_streams_without_files_info.bin"),
    ];
    for bytes in headerless {
        assert!(opens(bytes).is_err());
    }
}

/// A coder with no input stream gives a block without packed streams, whose first pack
/// stream index is past the pack sizes; it is rejected rather than indexed.
#[test]
fn a_block_without_packed_streams_is_rejected() {
    let lone: &[u8] = &[
        K_HEADER,
        K_MAIN_STREAMS_INFO,
        K_UNPACK_INFO,
        K_FOLDER,
        0x01, // num_blocks
        0x00, // external
        0x01, // num_coders
        0x11, // coder flags: id_size=1, complex, no attributes
        0x00, // coder id = COPY
        0x00, // num_in_streams = 0
        0x01, // num_out_streams = 1
        K_CODERS_UNPACK_SIZE,
        0x00,  // unpack_sizes[0] = 0
        K_END, // unpack_info end
        K_END, // streams_info end
        K_FILES_INFO,
        0x00,  // num_files = 0
        K_END, // files_info props end
        K_END, // header end
    ];
    assert!(opens(&raw_7z(&[], lone)).is_err());

    let after_copy_block: &[u8] = &[
        K_HEADER,
        K_MAIN_STREAMS_INFO,
        K_PACK_INFO,
        0x00, // pack_pos
        0x01, // num_pack_streams
        K_SIZE,
        0x05, // pack_sizes[0] = 5
        K_END,
        K_UNPACK_INFO,
        K_FOLDER,
        0x02, // num_blocks
        0x00, // external
        0x01, // block 0: num_coders
        0x01, // coder flags: id_size=1, simple, no attributes
        0x00, // coder id = COPY
        0x01, // block 1: num_coders
        0x11, // coder flags: id_size=1, complex, no attributes
        0x00, // coder id = COPY
        0x00, // num_in_streams = 0
        0x01, // num_out_streams = 1
        K_CODERS_UNPACK_SIZE,
        0x05, // unpack_sizes[0] = 5
        0x00, // unpack_sizes[1] = 0
        K_END,
        K_SUB_STREAMS_INFO,
        K_NUM_UNPACK_STREAM,
        0x01,  // block 0: one sub-stream
        0x00,  // block 1: no sub-streams
        K_END, // sub_streams_info end
        K_END, // streams_info end
        K_FILES_INFO,
        0x01,  // num_files = 1
        K_END, // files_info props end
        K_END, // header end
    ];
    assert!(opens(&raw_7z(&[0xAA; 5], after_copy_block)).is_err());
}

/// Every coder has one output stream; the block keeps one unpack size for each coder, so
/// a coder declaring none (two coders, one unpack size) is rejected rather than indexed.
#[test]
fn a_coder_without_an_output_stream_is_rejected() {
    let header: &[u8] = &[
        K_HEADER,
        K_MAIN_STREAMS_INFO,
        K_PACK_INFO,
        0x00, // pack_pos
        0x02, // num_pack_streams
        K_SIZE,
        0x01, // pack_sizes[0] = 1
        0x01, // pack_sizes[1] = 1
        K_END,
        K_UNPACK_INFO,
        K_FOLDER,
        0x01, // num_blocks
        0x00, // external
        0x02, // num_coders
        0x01, // coder 0 flags: id_size=1, simple, no attributes
        0x00, // coder 0 id = COPY
        0x11, // coder 1 flags: id_size=1, complex, no attributes
        0x00, // coder 1 id = COPY
        0x01, // num_in_streams = 1
        0x00, // num_out_streams = 0
        0x00, // packed_streams[0] = 0
        0x01, // packed_streams[1] = 1
        K_CODERS_UNPACK_SIZE,
        0x01,  // unpack_sizes[0] = 1
        K_END, // unpack_info end
        K_SUB_STREAMS_INFO,
        K_END, // sub_streams_info end
        K_END, // streams_info end
        K_FILES_INFO,
        0x01,  // num_files = 1
        K_END, // files_info props end
        K_END, // header end
    ];
    assert!(opens(&raw_7z(&[0xAA, 0xBB], header)).is_err());
}

/// A header with one block of `num_coders` COPY coders in a chain over five packed bytes,
/// and one file.
fn copy_chain_header(num_coders: u64) -> Vec<u8> {
    let mut header = vec![
        K_HEADER,
        K_MAIN_STREAMS_INFO,
        K_PACK_INFO,
        0x00, // pack_pos
        0x01, // num_pack_streams
        K_SIZE,
        0x05, // pack_sizes[0] = 5
        K_END,
        K_UNPACK_INFO,
        K_FOLDER,
        0x01, // num_blocks
        0x00, // external
    ];
    number(&mut header, num_coders);
    for _ in 0..num_coders {
        header.extend_from_slice(&[0x01, 0x00]); // simple COPY coder
    }
    // Coder `i` reads the output of coder `i + 1`; the packed stream feeds the last coder.
    for i in 0..num_coders - 1 {
        number(&mut header, i); // in_index
        number(&mut header, i + 1); // out_index
    }
    header.push(K_CODERS_UNPACK_SIZE);
    header.resize(header.len() + num_coders as usize, 0x05); // unpack size of every coder
    header.extend_from_slice(&[
        K_END, // unpack_info end
        K_SUB_STREAMS_INFO,
        K_END, // sub_streams_info end
        K_END, // streams_info end
        K_FILES_INFO,
        0x01,  // num_files = 1
        K_END, // files_info props end
        K_END, // header end
    ]);
    header
}

/// The decoder nests one reader for each coder of a block, and a long chain of coders
/// exhausts the stack. 7-Zip accepts at most 64 coders in a block.
#[test]
fn a_block_of_more_than_64_coders_is_rejected() {
    assert_eq!(decodes(&raw_7z(&[0xAA; 5], &copy_chain_header(64))), Ok(()));
    assert!(opens(&raw_7z(&[0xAA; 5], &copy_chain_header(65))).is_err());
}

/// A header with one block of COPY coders and one file. Coder `k` declares
/// `streams_per_coder[k]` input streams. Each bind pair is `(in_index, out_index)`, and
/// every packed stream index gets a one-byte pack stream.
fn coder_graph_header(
    streams_per_coder: &[u64],
    bind_pairs: &[(u64, u64)],
    packed_streams: &[u64],
) -> Vec<u8> {
    let mut header = vec![K_HEADER, K_MAIN_STREAMS_INFO, K_PACK_INFO, 0x00]; // pack_pos
    number(&mut header, packed_streams.len() as u64); // num_pack_streams
    header.push(K_SIZE);
    header.resize(header.len() + packed_streams.len(), 0x01); // pack_sizes = 1 each
    header.extend_from_slice(&[
        K_END,
        K_UNPACK_INFO,
        K_FOLDER,
        0x01, // num_blocks
        0x00, // external
    ]);
    number(&mut header, streams_per_coder.len() as u64); // num_coders
    for &streams in streams_per_coder {
        header.extend_from_slice(&[0x11, 0x00]); // complex COPY coder
        number(&mut header, streams); // num_in_streams
        header.push(0x01); // num_out_streams = 1
    }
    for &(in_index, out_index) in bind_pairs {
        number(&mut header, in_index);
        number(&mut header, out_index);
    }
    if packed_streams.len() > 1 {
        for &in_index in packed_streams {
            number(&mut header, in_index);
        }
    }
    header.push(K_CODERS_UNPACK_SIZE);
    header.resize(header.len() + streams_per_coder.len(), 0x01); // unpack size of every coder
    header.extend_from_slice(&[
        K_END, // unpack_info end
        K_SUB_STREAMS_INFO,
        K_END, // sub_streams_info end
        K_END, // streams_info end
        K_FILES_INFO,
        0x01,  // num_files = 1
        K_END, // files_info props end
        K_END, // header end
    ]);
    header
}

/// A header from `coder_graph_header` and its number of packed streams. The first input
/// of coder `k` reads the output of coder `k + 1`; every other input reads a packed stream.
fn input_streams_header(streams_per_coder: &[u64]) -> (Vec<u8>, usize) {
    let total: u64 = streams_per_coder.iter().sum();
    let mut bind_pairs = Vec::new();
    let mut first = 0;
    for (k, &streams) in streams_per_coder[..streams_per_coder.len() - 1]
        .iter()
        .enumerate()
    {
        bind_pairs.push((first, k as u64 + 1));
        first += streams;
    }
    let packed: Vec<u64> = (0..total)
        .filter(|i| !bind_pairs.iter().any(|&(in_index, _)| in_index == *i))
        .collect();
    let header = coder_graph_header(streams_per_coder, &bind_pairs, &packed);
    (header, packed.len())
}

/// 7-Zip accepts at most 64 input streams in a block: one coder declaring 65, or two
/// declaring 33 each, is rejected.
#[test]
fn a_block_of_more_than_64_input_streams_is_rejected() {
    for streams_per_coder in [&[65][..], &[33, 33]] {
        let (header, num_packed) = input_streams_header(streams_per_coder);
        let bytes = raw_7z(&vec![0xAA; num_packed], &header);
        assert!(opens(&bytes).is_err(), "{streams_per_coder:?}");
    }
}

/// A name, the input streams of each coder, the bind pairs and the packed streams of a block.
type CoderGraph<'a> = (&'a str, &'a [u64], &'a [(u64, u64)], &'a [u64]);

/// Each input stream of a block is bound or packed once, and each coder output bound
/// once: a stream used twice makes the decoder build the coders behind it once for every
/// use (a chain of BCJ2 coders with three inputs bound to one output builds 3^n readers).
#[test]
fn a_block_using_a_stream_twice_is_rejected() {
    let valid = coder_graph_header(&[2, 1], &[(0, 1)], &[1, 2]);
    assert!(opens(&raw_7z(&[0xAA; 2], &valid)).is_ok());

    let cases: [CoderGraph<'_>; 5] = [
        (
            "an output bound twice",
            &[2, 1, 1],
            &[(0, 1), (1, 1)],
            &[2, 3],
        ),
        (
            "an input bound twice",
            &[2, 1, 1],
            &[(0, 1), (0, 2)],
            &[1, 2],
        ),
        ("an input packed twice", &[2, 1], &[(0, 1)], &[1, 1]),
        ("an input bound and packed", &[2, 1], &[(0, 1)], &[0, 1]),
        ("a packed input out of range", &[2, 1], &[(0, 1)], &[1, 5]),
    ];
    for (name, streams_per_coder, bind_pairs, packed_streams) in cases {
        let header = coder_graph_header(streams_per_coder, bind_pairs, packed_streams);
        let bytes = raw_7z(&vec![0xAA; packed_streams.len()], &header);
        assert!(opens(&bytes).is_err(), "{name}");
    }
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
