use cash_core::{ExecutionResult, builtins};
use clap::Parser;
use std::io::Write;

/// Manage the process umask.
#[derive(Parser)]
pub(crate) struct UmaskCommand {
    /// If MODE is omitted, output in a form that may be reused as input.
    #[arg(short = 'p')]
    print_roundtrippable: bool,

    /// Makes the output symbolic; otherwise an octal number is given.
    #[arg(short = 'S')]
    symbolic_output: bool,

    /// Mode mask.
    mode: Option<String>,
}

impl builtins::Command for UmaskCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        if let Some(mode) = &self.mode {
            if mode.starts_with(|c: char| c.is_ascii_digit()) {
                let parsed = cash_core::int_utils::parse(mode.as_str(), 8).map_err(|_| {
                    cash_core::ErrorKind::InvalidUmask(format!("{mode}: octal number out of range"))
                })?;
                context.shell.set_umask(parsed);
            } else {
                let parsed = parse_symbolic_umask(mode, context.shell.umask())?;
                context.shell.set_umask(parsed);
            }

            if self.symbolic_output {
                writeln!(
                    context.stdout(),
                    "{}",
                    format_symbolic_umask(context.shell.umask())
                )?;
            }
        } else {
            let umask = context.shell.umask();

            let formatted = if self.symbolic_output {
                format_symbolic_umask(umask)
            } else {
                std::format!("{umask:04o}")
            };

            if self.print_roundtrippable {
                writeln!(context.stdout(), "umask {formatted}")?;
            } else {
                writeln!(context.stdout(), "{formatted}")?;
            }
        }

        Ok(ExecutionResult::success())
    }
}

fn format_symbolic_umask(umask: u32) -> String {
    let u = symbolic_mask_from_bits((!umask & 0o700) >> 6);
    let g = symbolic_mask_from_bits((!umask & 0o070) >> 3);
    let o = symbolic_mask_from_bits(!umask & 0o007);
    std::format!("u={u},g={g},o={o}")
}

/// Parses the chmod-style symbolic language Bash accepts for `umask`. Work with the
/// complement of the mask (the permissions that are allowed), exactly as Bash does.
fn parse_symbolic_umask(mode: &str, current_mask: u32) -> Result<u32, cash_core::Error> {
    let bytes = mode.as_bytes();
    let initial = !current_mask & 0o777;
    let mut allowed = initial;
    let mut index = 0;

    while index < bytes.len() {
        let mut who = 0;
        while let Some(byte) = bytes.get(index) {
            let bits = match byte {
                b'u' => 0o700,
                b'g' => 0o070,
                b'o' => 0o007,
                b'a' => 0o777,
                _ => break,
            };
            who |= bits;
            index += 1;
        }
        if who == 0 {
            who = 0o777;
        }

        loop {
            let operation = bytes.get(index).copied().unwrap_or(0);
            if !matches!(operation, b'+' | b'-' | b'=') {
                return Err(invalid_mode(operation, "operator"));
            }
            index += 1;

            let mut permissions = 0;
            while let Some(byte) = bytes.get(index) {
                let (bits, copy) = match byte {
                    b'r' => (0o444, false),
                    b'w' => (0o222, false),
                    b'x' => (0o111, false),
                    b'X' => {
                        if initial & 0o111 == 0 {
                            (0, false)
                        } else {
                            (0o111, false)
                        }
                    }
                    b'u' => (copy_permission_class(initial, 6), true),
                    b'g' => (copy_permission_class(initial, 3), true),
                    b'o' => (copy_permission_class(initial, 0), true),
                    // Bash accepts these in the shared chmod grammar. They do not
                    // contribute to the nine file-creation permission bits.
                    b's' | b't' => (0, false),
                    _ => break,
                };
                // GNU Bash's parser assigns for a copy specification. Thus `g=ru`
                // differs from `g=ur`: the `u` in the former replaces the earlier `r`.
                if copy {
                    permissions = bits;
                } else {
                    permissions |= bits;
                }
                index += 1;
            }

            let permissions = permissions & who;
            match operation {
                b'+' => allowed |= permissions,
                b'-' => allowed &= !permissions,
                b'=' => allowed = (allowed & !who) | permissions,
                _ => unreachable!(),
            }

            match bytes.get(index) {
                None => return Ok(!allowed & 0o777),
                Some(b',') => {
                    index += 1;
                    break;
                }
                Some(b'+' | b'-' | b'=') => {}
                Some(&other) => return Err(invalid_mode(other, "character")),
            }
        }
    }

    Err(invalid_mode(0, "operator"))
}

/// Bash's complaint about a symbolic mode, naming the character it stopped at in quotes
/// (`q` in `u=q`). The end of the mode is a NUL between them, as Bash prints it.
fn invalid_mode(byte: u8, what: &str) -> cash_core::Error {
    cash_core::ErrorKind::InvalidUmask(format!(
        "`{}': invalid symbolic mode {what}",
        char::from(byte)
    ))
    .into()
}

const fn copy_permission_class(bits: u32, shift: u32) -> u32 {
    let class = (bits >> shift) & 0o7;
    let read = if class & 0o4 != 0 { 0o444 } else { 0 };
    let write = if class & 0o2 != 0 { 0o222 } else { 0 };
    let execute = if class & 0o1 != 0 { 0o111 } else { 0 };
    read | write | execute
}

// cash: Windows has no umask.
//
// File permissions come from ACLs inherited from the parent directory (D23), and there
// is no process-wide mask to consult or set. The value is nonetheless *remembered*, in the
// shell (`Shell::umask`), so that `umask 022` and a later `umask` round-trip as a script
// expects, and a subshell's `umask` stays its own.
//
// Absent entirely would be worse. `umask 022` is a commonplace line, and a shell that
// answers `command not found` kills any script running under `set -e` — a direct hit on
// D2. Reporting the stored value is the least-surprising behaviour available on a
// platform with no such concept.

fn symbolic_mask_from_bits(bits: u32) -> String {
    let mut result = String::new();

    if (bits & 0b100) != 0 {
        result.push('r');
    }
    if (bits & 0b010) != 0 {
        result.push('w');
    }
    if (bits & 0b001) != 0 {
        result.push('x');
    }

    result
}
