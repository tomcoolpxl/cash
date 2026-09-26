//! Passing arguments to MSYS2 and Cygwin programs intact.
//!
//! A Windows process receives one command line, and each program splits it into words
//! itself. Programs built with the Microsoft C runtime split by the rules
//! `CommandLineToArgvW` documents, which are the rules Rust's `std::process::Command`
//! encodes for. Programs linked against `msys-2.0.dll` or `cygwin1.dll`, which is all of
//! Git for Windows' `usr/bin` (`grep`, `sed`, `awk`, `echo`, `sh`), split by Cygwin's own
//! rules when their parent is not a Cygwin process. Those rules differ enough to change an
//! argument on the way: `'` quotes as well as `"`, a backslash is a glob escape, and an
//! argument that contains a glob character is globbed against the working directory.
//! `"[^[:cntrl:]"\\]*"|[[:space:]]+`, a pattern `JSON.sh` passes to `egrep`, arrived at
//! Git's `grep` as `\[^[:cntrl:]"\]*"|[[:space:]]+`.
//!
//! [`quote_arg`] encodes one argument the way Cygwin decodes it (`build_argv`, `quoted`
//! and `globify` in `winsup/cygwin/dcrt0.cc`), and [`is_msys_program`] tells which
//! executables need it, by reading their import table.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::io::{Read as _, Seek as _, SeekFrom};
use std::os::windows::ffi::{OsStrExt as _, OsStringExt as _};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::SystemTime;

/// The runtime DLLs that make a program split its command line the Cygwin way.
const RUNTIMES: [&str; 2] = ["msys-2.0.dll", "cygwin1.dll"];

/// Answers already worked out, keyed by path, with the modification time each was
/// worked out for.
type Answers = HashMap<PathBuf, (Option<SystemTime>, bool)>;

static CACHE: Mutex<Option<Answers>> = Mutex::new(None);

/// Whether the executable at `path` links against the MSYS2 or Cygwin runtime.
///
/// Anything unreadable or malformed counts as an ordinary Windows program, which is
/// what every program was taken to be before this check existed.
pub fn is_msys_program(path: &Path) -> bool {
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let known = CACHE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .and_then(|cache| cache.get(path).copied());
    if let Some((when, answer)) = known {
        if when == modified {
            return answer;
        }
    }
    let answer = imported_dlls(path).is_some_and(|dlls| {
        dlls.iter()
            .any(|dll| RUNTIMES.iter().any(|rt| dll.eq_ignore_ascii_case(rt)))
    });
    CACHE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_with(HashMap::new)
        .insert(path.to_path_buf(), (modified, answer));
    answer
}

/// Adds `args` to `command` in the encoding the program at `target` decodes.
///
/// That is Cygwin's for an MSYS2 or Cygwin program, and the Microsoft C runtime's (what
/// `Command::args` writes) for anything else, including when `target` is unknown.
pub fn add_args<S: AsRef<OsStr>>(
    command: &mut std::process::Command,
    target: Option<&Path>,
    args: &[S],
) {
    use std::os::windows::process::CommandExt as _;

    if target.is_some_and(is_msys_program) {
        for arg in args {
            command.raw_arg(quote_arg(arg.as_ref()));
        }
    } else {
        command.args(args);
    }
}

/// The native executable `program` names, or `None` if there is none.
///
/// It is searched for the way `Command::new` will search: as a path from `cwd` if it has
/// a separator, otherwise along the directories in `path` with `PATHEXT`. The caller
/// splits PATH, because the shell's is spelled with `:` and the process's with `;`.
pub fn locate(program: &OsStr, path: &[PathBuf], cwd: &Path) -> Option<PathBuf> {
    let pathext = std::env::var("PATHEXT").map_or_else(
        |_| {
            crate::resolve::DEFAULT_PATHEXT
                .iter()
                .map(|s| (*s).to_owned())
                .collect()
        },
        |value| crate::resolve::parse_pathext(&value),
    );
    match crate::resolve::resolve(&program.to_string_lossy(), path, &pathext, cwd)? {
        crate::resolve::Dispatch::Native(target) => Some(target),
        _ => None,
    }
}

/// Runs `program` with `args` on behalf of `tool`, a bundled utility that spawns its
/// command itself, and returns the program's exit status.
///
/// uutils' `env` and `timeout` build their child's command line with
/// `std::process::Command`, which always writes the Microsoft encoding. When their
/// command is an MSYS2 program, cash names itself as the command instead; this is what
/// then runs. Its own arguments arrived intact, because cash decodes the Microsoft way,
/// and it encodes them again for whatever `program` turns out to be in the environment
/// and directory the tool set up.
///
/// The program goes in a job that dies with this process, so a tool that kills its
/// child, as `timeout` does, still kills the program. Console events are left to the
/// program: it shares this console group, and its exit status is what gets reported.
pub fn relay(tool: &str, program: &OsStr, args: &[OsString]) -> i32 {
    use windows_sys::Win32::Foundation::TRUE;
    use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;
    use windows_sys::core::BOOL;

    /// Swallows every console event. A handler, not `SetConsoleCtrlHandler(NULL, …)`,
    /// because the null form is inherited and would make the program ignore Ctrl-C too.
    const unsafe extern "system" fn ignore(_event: u32) -> BOOL {
        TRUE
    }

    let path: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default();
    let cwd = std::env::current_dir().unwrap_or_default();
    let target = locate(program, &path, &cwd);
    let mut command =
        std::process::Command::new(target.as_deref().map_or(program, Path::as_os_str));
    add_args(&mut command, target.as_deref(), args);

    // SAFETY: `ignore` is a valid handler for the life of the process and touches nothing.
    unsafe { SetConsoleCtrlHandler(Some(ignore), TRUE) };
    let job = crate::job::JobObject::for_pipeline().ok();
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            eprintln!("{tool}: {}: {err}", program.to_string_lossy());
            return if err.kind() == std::io::ErrorKind::NotFound {
                127
            } else {
                126
            };
        }
    };
    if let Some(job) = &job {
        let _ = job.assign_child(&child);
    }
    match child.wait() {
        Ok(status) => status.code().unwrap_or(1),
        Err(err) => {
            eprintln!("{tool}: {}: {err}", program.to_string_lossy());
            125
        }
    }
}

/// The names of the DLLs in a PE file's import table, or `None` if it is not one.
fn imported_dlls(path: &Path) -> Option<Vec<String>> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut read_at = |offset: u64, len: usize| -> Option<Vec<u8>> {
        let mut buf = vec![0u8; len];
        file.seek(SeekFrom::Start(offset)).ok()?;
        file.read_exact(&mut buf).ok()?;
        Some(buf)
    };
    let u16_at =
        |b: &[u8], at: usize| Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?));
    let u32_at =
        |b: &[u8], at: usize| Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?));

    let dos = read_at(0, 64)?;
    if dos.get(..2)? != b"MZ" {
        return None;
    }
    let pe = u64::from(u32_at(&dos, 0x3C)?);
    let coff = read_at(pe, 24)?;
    if coff.get(..4)? != b"PE\0\0" {
        return None;
    }
    let sections = usize::from(u16_at(&coff, 6)?);
    let optional_len = usize::from(u16_at(&coff, 20)?);
    let optional = read_at(pe + 24, optional_len)?;
    // PE32 and PE32+ differ in where the data directories start.
    let directories = match u16_at(&optional, 0)? {
        0x10b => 96,
        0x20b => 112,
        _ => return None,
    };
    let import_rva = u32_at(&optional, directories + 8)?;
    if import_rva == 0 {
        return Some(Vec::new());
    }

    let table_at = pe + 24 + u64::try_from(optional_len).ok()?;
    let table = read_at(table_at, sections.checked_mul(40)?)?;
    let offset_of = |rva: u32| -> Option<u64> {
        table.as_chunks::<40>().0.iter().find_map(|section| {
            let size = u32_at(section, 8)?.max(u32_at(section, 16)?);
            let start = u32_at(section, 12)?;
            let raw = u32_at(section, 20)?;
            (rva >= start && rva - start < size).then(|| u64::from(raw) + u64::from(rva - start))
        })
    };

    let mut names = Vec::new();
    let mut descriptor = offset_of(import_rva)?;
    // A descriptor is 20 bytes and the table ends with an all-zero one; the bound only
    // stops a corrupt file from being read forever.
    for _ in 0..1024 {
        let entry = read_at(descriptor, 20)?;
        let name_rva = u32_at(&entry, 12)?;
        if name_rva == 0 {
            break;
        }
        if let Some(name) = offset_of(name_rva).and_then(|at| read_at(at, 64)) {
            let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
            names.push(String::from_utf8_lossy(name.get(..end)?).into_owned());
        }
        descriptor += 20;
    }
    Some(names)
}

/// Characters that make Cygwin treat an argument as a glob pattern or a quoted string.
fn is_special(unit: u16) -> bool {
    matches!(
        char::from_u32(u32::from(unit)),
        Some('?' | '*' | '[' | ']' | '"' | '\'' | '(' | ')' | '{' | '}')
    )
}

fn is_blank(unit: u16) -> bool {
    char::from_u32(u32::from(unit)).is_some_and(char::is_whitespace)
}

fn is_ascii(unit: u16, c: u8) -> bool {
    unit == u16::from(c)
}

/// Encodes one argument for an MSYS2 or Cygwin program's command line, so that the
/// program's runtime decodes exactly `arg`.
///
/// * An argument with nothing Cygwin would act on is passed as it is, backslashes and
///   all: a word without quotes or glob characters is never unescaped.
/// * Otherwise the whole argument goes in double quotes, with `"` and `\` escaped, which
///   inside quotes both the word splitter and the globber undo, and which leaves every
///   other character literal.
/// * A drive-letter path (`C:…`) is the exception: Cygwin keeps its backslashes literal
///   and does not undo escapes inside its quotes, so only the characters that need it are
///   quoted, one at a time, and the backslashes stay outside.
pub fn quote_arg(arg: &OsStr) -> OsString {
    let units: Vec<u16> = arg.encode_wide().collect();
    let plain = !units.is_empty()
        && !units.iter().any(|&u| is_special(u) || is_blank(u))
        && !units
            .first()
            .is_some_and(|&u| is_ascii(u, b'~') || is_ascii(u, b'@'));
    if plain {
        return arg.to_owned();
    }

    let quote = u16::from(b'"');
    let apostrophe = u16::from(b'\'');
    let backslash = u16::from(b'\\');
    let drive = units
        .first()
        .is_some_and(|&u| u < 0x80 && u8::try_from(u).is_ok_and(|b| b.is_ascii_alphabetic()))
        && units.get(1).is_some_and(|&u| is_ascii(u, b':'));

    let mut out = Vec::with_capacity(units.len() * 2 + 2);
    if drive {
        for &unit in &units {
            if unit == quote {
                out.extend([apostrophe, quote, apostrophe]);
            } else if is_special(unit) || is_blank(unit) {
                out.extend([quote, unit, quote]);
            } else {
                out.push(unit);
            }
        }
    } else {
        out.push(quote);
        for &unit in &units {
            if unit == quote || unit == backslash {
                out.push(backslash);
            }
            out.push(unit);
        }
        out.push(quote);
    }
    OsString::from_wide(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(arg: &str) -> String {
        quote_arg(OsStr::new(arg)).to_string_lossy().into_owned()
    }

    #[test]
    fn a_plain_word_passes_unchanged() {
        assert_eq!(q("-Eo"), "-Eo");
        assert_eq!(q(r"a\b\c"), r"a\b\c");
        assert_eq!(q(r"C:\Windows\System32"), r"C:\Windows\System32");
    }

    #[test]
    fn quotes_and_backslashes_are_escaped_inside_double_quotes() {
        assert_eq!(q(r#""[^"\\]*""#), r#""\"[^\"\\\\]*\"""#);
        assert_eq!(q("[[:space:]]+"), r#""[[:space:]]+""#);
        assert_eq!(q(""), r#""""#);
        assert_eq!(q("~/x"), r#""~/x""#);
    }

    #[test]
    fn a_drive_path_keeps_its_backslashes_outside_the_quotes() {
        assert_eq!(q(r"C:\Program Files\x\"), r#"C:\Program" "Files\x\"#);
        assert_eq!(q(r"C:\a*"), r#"C:\a"*""#);
    }

    #[test]
    fn git_grep_is_recognised_and_cmd_is_not() {
        let grep = Path::new(r"C:\Program Files\Git\usr\bin\grep.exe");
        if grep.is_file() {
            assert!(is_msys_program(grep));
        }
        let system = std::env::var_os("SystemRoot").map(PathBuf::from);
        if let Some(cmd) = system.map(|root| root.join(r"System32\cmd.exe")) {
            assert!(!is_msys_program(&cmd));
        }
    }
}
