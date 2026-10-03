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
//! | `-w`, `a-w`, `u-w` | sets the read-only attribute: the file becomes read-only. Real, and visible to every Windows program |
//! | `+w`, `u+w` | clears read-only |
//! | `+x`, `+r` | D23: executability comes from the extension or a shebang, and a file is readable; silent, 0 |
//! | `-x` | D34: revoking execute needs a Deny ACE, which is blunt enough to lock you out of your own file. Warns and returns 0 |
//! | `-r` | Windows has no per-file read bit outside ACLs. Warns and returns 0 |
//! | `g…`, `o…` | Windows has no per-file group or other bits; silent, 0 |
//!
//! Numeric modes (`755`, `644`) are read for the owner's write bit, silently. `chmod 644 f`
//! does the one real thing it implies here — clearing read-only — rather than claiming to
//! have set nine mode bits it cannot.

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

/// Reduce a mode argument to the one bit Windows actually keeps per file: the owner's
/// write bit.
///
/// What else a mode asks for is said only where something is lost: taking read or
/// execute away from the owner (D34), setuid, setgid and sticky. Giving the owner read or
/// execute (D23; the user, 2026-10-02) and anything for group and other, which Windows
/// has no per-file bits for (the user, 2026-10-03), go without a word, as the execute bits
/// of a numeric mode do. A symbolic mode is a list of clauses (`u+rw,go-w`); only the first
/// was read, and as if it were the owner's, so `chmod go-w f` made `f` read-only (BI-05).
fn parse_mode(mode: &str) -> Option<Request> {
    // Numeric: only the owner's write bit has a Windows equivalent.
    if mode.chars().all(|c| c.is_ascii_digit()) {
        let value = u32::from_str_radix(mode, 8).ok()?;
        let owner = (value >> 6) & 0o7;
        return Some(Request {
            writable: Some(owner & 0o2 != 0),
            unsupported: Vec::new(),
        });
    }

    let mut writable = None;
    let mut unsupported = Vec::new();
    for clause in mode.split(',') {
        // [ugoa]* then one or more [+-=][rwxXst]*
        let who_end = clause.find(['+', '-', '='])?;
        let (who, mut rest) = clause.split_at(who_end);
        if !who.chars().all(|c| "ugoa".contains(c)) {
            return None;
        }
        let owner = who.is_empty() || who.contains(['u', 'a']);
        while let Some(op) = rest.chars().next() {
            let perms_end = rest
                .get(1..)?
                .find(['+', '-', '='])
                .map_or(rest.len(), |at| at + 1);
            let perms = rest.get(1..perms_end)?;
            rest = rest.get(perms_end..)?;
            if !perms.chars().all(|c| "rwxXst".contains(c)) {
                return None;
            }
            if !owner {
                continue;
            }
            let has = |bit: char| perms.contains(bit);
            match op {
                '+' => {
                    if has('w') {
                        writable = Some(true);
                    }
                }
                '-' => {
                    if has('w') {
                        writable = Some(false);
                    }
                    if has('r') {
                        unsupported.push("read");
                    }
                    if has('x') || has('X') {
                        unsupported.push("execute");
                    }
                }
                _ => {
                    writable = Some(has('w'));
                    if !has('r') {
                        unsupported.push("read");
                    }
                }
            }
            if op != '-' && has('s') {
                unsupported.push("setuid/setgid");
            }
            if op != '-' && has('t') {
                unsupported.push("sticky");
            }
        }
    }

    unsupported.sort_unstable();
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
