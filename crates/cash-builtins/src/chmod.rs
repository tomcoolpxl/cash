//! `chmod` — **D34**, **D23**.
//!
//! uutils' `chmod` is Unix-only: on Windows the crate compiles to nothing, so D48's
//! bundle cannot supply one. What filled the gap instead was the MSYS `chmod` from Git
//! for Windows, and that one is a fiction: it writes POSIX mode bits into MSYS's own
//! emulation layer, which nothing outside MSYS reads. So `chmod +x f` reported success
//! and `test -x f` — which follows D23 and reads ACLs and extensions — still said no.
//! Two tools in the same shell disagreeing about the same file, neither of them wrong on
//! its own terms.
//!
//! This one does only what Windows can honestly do, and says so when it cannot:
//!
//! | Request | What happens |
//! |---|---|
//! | `-w`, `a-w`, `u-w` | clears the read-only attribute's inverse: the file becomes read-only. Real, and visible to every Windows program |
//! | `+w` | clears read-only |
//! | `+x` | D23: executability comes from the extension or a shebang, so this warns and returns 0 |
//! | `-x` | D34: revoking execute needs a Deny ACE, which is blunt enough to lock you out of your own file. Warns and returns 0 |
//! | `+r`, `-r` | Windows has no per-file read bit outside ACLs. Warns and returns 0 |
//!
//! Numeric modes (`755`, `644`) are read for their write bit and otherwise treated the
//! same way. `chmod 644 f` does the one real thing it implies here — clearing read-only —
//! rather than claiming to have set nine mode bits it cannot.

use std::io::Write;
use std::path::Path;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Change what a file permits, as far as Windows can express it.
#[derive(Parser)]
pub(crate) struct ChmodCommand {
    /// Suppress the diagnostics about permissions Windows cannot express.
    #[arg(short = 'f', long = "silent", alias = "quiet")]
    silent: bool,

    /// Describe what is being changed.
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,

    /// Operate on directories and their contents.
    #[arg(short = 'R', long = "recursive")]
    recursive: bool,

    /// The mode, then the files: `chmod +x script.sh`.
    #[arg(allow_hyphen_values = true)]
    args: Vec<String>,
}

/// What a mode argument asks for, reduced to what Windows can answer.
struct Request {
    /// `Some(true)` means make writable, `Some(false)` means read-only.
    writable: Option<bool>,
    /// Bits that were asked for and cannot be honoured, for the diagnostic.
    unsupported: Vec<&'static str>,
}

impl builtins::Command for ChmodCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let Some((mode, files)) = self.args.split_first() else {
            writeln!(
                context.stderr(),
                "{}: missing operand\nusage: chmod MODE FILE...",
                context.command_name
            )?;
            return Ok(ExecutionResult::from(
                cash_core::ExecutionExitCode::InvalidUsage,
            ));
        };

        if files.is_empty() {
            writeln!(
                context.stderr(),
                "{}: missing file operand after '{mode}'",
                context.command_name
            )?;
            return Ok(ExecutionResult::from(
                cash_core::ExecutionExitCode::InvalidUsage,
            ));
        }

        let Some(request) = parse_mode(mode) else {
            writeln!(
                context.stderr(),
                "{}: invalid mode: '{mode}'",
                context.command_name
            )?;
            return Ok(ExecutionResult::general_error());
        };

        // Said once for the whole invocation, not once per file: the reason is about the
        // platform, and repeating it for a hundred files would bury the real errors.
        if !self.silent && !request.unsupported.is_empty() {
            writeln!(
                context.stderr(),
                "{}: {}: not represented on Windows; ignored",
                context.command_name,
                request.unsupported.join(", ")
            )?;
        }

        let mut failed = false;
        for file in files {
            let path = context.shell.absolute_path(Path::new(file));
            let mut targets = vec![path.clone()];
            if self.recursive {
                collect(&path, &mut targets);
            }

            for target in targets {
                if let Err(e) = apply(&target, &request) {
                    if !self.silent {
                        writeln!(
                            context.stderr(),
                            "{}: {}: {e}",
                            context.command_name,
                            cash_win32::path::render(&target)
                        )?;
                    }
                    failed = true;
                } else if self.verbose {
                    writeln!(
                        context.stdout(),
                        "mode of '{}' retained as {}",
                        cash_win32::path::render(&target),
                        describe(&target)
                    )?;
                }
            }
        }

        if failed {
            return Ok(ExecutionResult::general_error());
        }
        Ok(ExecutionResult::success())
    }
}

/// Reduce a mode argument to the one bit Windows actually keeps per file.
fn parse_mode(mode: &str) -> Option<Request> {
    let mut unsupported = Vec::new();

    // Numeric: only the owner's write bit has a Windows equivalent.
    if mode.chars().all(|c| c.is_ascii_digit()) {
        let value = u32::from_str_radix(mode, 8).ok()?;
        let owner = (value >> 6) & 0o7;
        if owner & 0o1 != 0 {
            unsupported.push("execute");
        }
        return Some(Request {
            writable: Some(owner & 0o2 != 0),
            unsupported,
        });
    }

    // Symbolic: [ugoa]*[+-=][rwx]+
    let operator = mode.find(['+', '-', '='])?;
    let (_who, rest) = mode.split_at(operator);
    let mut chars = rest.chars();
    let op = chars.next()?;
    let bits: String = chars.collect();
    if bits.is_empty() || !bits.chars().all(|c| "rwxXst".contains(c)) {
        return None;
    }

    let mut writable = None;
    for bit in bits.chars() {
        match bit {
            'w' => writable = Some(op != '-'),
            'x' | 'X' => unsupported.push("execute"),
            'r' => unsupported.push("read"),
            's' => unsupported.push("setuid/setgid"),
            't' => unsupported.push("sticky"),
            _ => (),
        }
    }

    // `=` without `w` means write is being taken away.
    if op == '=' && writable.is_none() {
        writable = Some(false);
    }

    unsupported.dedup();
    Some(Request {
        writable,
        unsupported,
    })
}

/// Apply the one change Windows can keep.
fn apply(path: &Path, request: &Request) -> std::io::Result<()> {
    let Some(writable) = request.writable else {
        // Nothing representable was asked for. Confirm the file exists so a typo in the
        // filename is still an error, then do nothing — D34's "warn and return 0".
        return path.metadata().map(|_| ());
    };

    let mut permissions = path.metadata()?.permissions();
    permissions.set_readonly(!writable);
    std::fs::set_permissions(path, permissions)
}

/// How the file stands now, for `-v`.
fn describe(path: &Path) -> &'static str {
    match path.metadata() {
        Ok(meta) if meta.permissions().readonly() => "read-only",
        Ok(_) => "writable",
        Err(_) => "unknown",
    }
}

/// Gather a directory's contents for `-R`.
fn collect(root: &Path, into: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, into);
        }
        into.push(path);
    }
}
