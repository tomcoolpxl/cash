use cash_core::{ExecutionResult, builtins};
#[cfg(unix)]
use cash_core::ErrorKind;
#[cfg(unix)]
use cfg_if::cfg_if;
use clap::Parser;
#[cfg(all(unix, not(any(target_os = "linux", target_os = "android"))))]
use nix::sys::stat::Mode;
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
            if mode.starts_with(|c: char| c.is_digit(8)) {
                let parsed = cash_core::int_utils::parse(mode.as_str(), 8)?;
                set_umask(parsed)?;
            } else {
                return cash_core::error::unimp("umask setting mode from symbolic value");
            }
        } else {
            let umask = get_umask()?;

            let formatted = if self.symbolic_output {
                let u = symbolic_mask_from_bits((!umask & 0o700) >> 6);
                let g = symbolic_mask_from_bits((!umask & 0o070) >> 3);
                let o = symbolic_mask_from_bits(!umask & 0o007);
                std::format!("u={u},g={g},o={o}")
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

/// cash: Windows has no umask.
///
/// File permissions come from ACLs inherited from the parent directory (D23), and there
/// is no process-wide mask to consult or set. The value is nonetheless *remembered*, so
/// that `umask 022` and a later `umask` round-trip as a script expects.
///
/// Absent entirely would be worse. `umask 022` is a commonplace line, and a shell that
/// answers `command not found` kills any script running under `set -e` — a direct hit on
/// D2. Reporting the stored value is the least-surprising behaviour available on a
/// platform with no such concept.
#[cfg(windows)]
static REMEMBERED_UMASK: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0o022);

#[cfg(windows)]
#[expect(clippy::unnecessary_wraps)]
fn get_umask() -> Result<u32, cash_core::Error> {
    Ok(REMEMBERED_UMASK.load(std::sync::atomic::Ordering::Relaxed))
}

#[cfg(windows)]
#[expect(clippy::unnecessary_wraps)]
fn set_umask(value: u32) -> Result<(), cash_core::Error> {
    REMEMBERED_UMASK.store(value, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

#[cfg(unix)]
cfg_if! {
    if #[cfg(any(target_os = "linux", target_os = "android"))] {
        fn get_umask() -> Result<u32, cash_core::Error> {
            let umask = procfs::process::Process::myself().ok().and_then(|me| me.status().ok()).and_then(|status| status.umask);
            umask.ok_or_else(|| cash_core::ErrorKind::InvalidUmask.into())
        }
    } else {
        #[expect(clippy::unnecessary_wraps)]
        fn get_umask() -> Result<u32, cash_core::Error> {
            let u = nix::sys::stat::umask(Mode::empty());
            nix::sys::stat::umask(u);
            Ok(u32::from(u.bits()))
        }
    }
}

#[cfg(unix)]
fn set_umask(value: nix::sys::stat::mode_t) -> Result<(), cash_core::Error> {
    // value of mode_t can be platform dependent
    let mode = nix::sys::stat::Mode::from_bits(value).ok_or_else(|| ErrorKind::InvalidUmask)?;
    nix::sys::stat::umask(mode);
    Ok(())
}

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
