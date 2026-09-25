//! `coolfetch` — the banner, done the way a shell can do it.
//!
//! `neofetch` was archived in 2024 and is still what most people have; `fastfetch` is
//! what replaced it. Both are general-purpose tools that must identify *any* system, and
//! on Windows that costs: `neofetch` is a large shell script that spawns `wmic`, `reg`,
//! `uname`, `df` and more, and every one of those spawns is a `CreateProcessW`.
//!
//! cash is already holding every number they go looking for. The memory figures come from
//! the same `GlobalMemoryStatusEx` that `top` uses, the process count from the same
//! snapshot as `ps`, the account from the same token lookup, the machine's name from the
//! same place `$HOSTNAME` now comes from. What is left — the edition, the build, the
//! processor's name — is three registry reads.
//!
//! So this spawns nothing at all, and it does not try to identify anything but Windows.
//! That is the whole reason it can be instant.
//!
//! It is deliberately **not** called `neofetch`: printing different output under another
//! tool's name is the identity mismatch `cash doctor` exists to catch, and it would
//! shadow a real one on `PATH`.

use std::io::{IsTerminal, Write};

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Show what this machine is.
#[derive(Parser)]
pub(crate) struct CoolfetchCommand {
    /// Leave the logo out and print only the facts.
    #[arg(long = "no-logo")]
    no_logo: bool,

    /// Never colour the output, even on a terminal.
    #[arg(long = "no-color", alias = "no-colour")]
    no_color: bool,
}

/// The four panes, drawn small enough to sit beside the facts.
const LOGO: &[&str] = &[
    "  ####### #######  ",
    "  ####### #######  ",
    "  ####### #######  ",
    "                   ",
    "  ####### #######  ",
    "  ####### #######  ",
    "  ####### #######  ",
];

impl builtins::Command for CoolfetchCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let colour = !self.no_color && std::io::stdout().is_terminal();
        let facts = collect(&context);

        let mut stdout = context.stdout();
        let logo: &[&str] = if self.no_logo { &[] } else { LOGO };
        let rows = logo.len().max(facts.len());
        // The facts outnumber the logo's lines, so the ones past its end still need its
        // width — otherwise the tail of the list jumps back to column zero.
        let gutter = logo.iter().map(|line| line.len()).max().unwrap_or(0);

        for row in 0..rows {
            let art = logo.get(row).copied().unwrap_or("");
            if !self.no_logo {
                write!(
                    stdout,
                    "{}",
                    paint(&std::format!("{art:gutter$}"), "34", colour)
                )?;
            }

            if let Some((label, value)) = facts.get(row) {
                if label.is_empty() {
                    writeln!(stdout, "{}", paint(value, "36", colour))?;
                } else {
                    writeln!(stdout, "{}: {value}", paint(label.as_str(), "36;1", colour))?;
                }
            } else {
                writeln!(stdout)?;
            }
        }

        Ok(ExecutionResult::success())
    }
}

/// Wraps `value` in an ANSI colour, when there is a terminal to see it.
fn paint(value: &str, colour: &str, enabled: bool) -> String {
    if enabled {
        std::format!("\x1b[{colour}m{value}\x1b[0m")
    } else {
        value.to_string()
    }
}

/// Every line to the right of the logo, in order.
#[allow(
    clippy::too_many_lines,
    reason = "one block per fact, each a few lines"
)]
fn collect<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
) -> Vec<(String, String)> {
    let mut facts: Vec<(String, String)> = Vec::new();

    let user = cash_win32::process::details(std::process::id())
        .user
        .unwrap_or_else(|| String::from("?"));
    let host = cash_win32::process::computer_name().unwrap_or_else(|| String::from("?"));

    facts.push((String::new(), std::format!("{user}@{host}")));
    facts.push((String::new(), "-".repeat(user.len() + host.len() + 1)));

    let edition = cash_win32::sysinfo::os_name().unwrap_or_else(|| String::from("Windows"));
    let build = cash_win32::sysinfo::os_build();
    let release = cash_win32::sysinfo::os_release();
    let os = match (release, build) {
        (Some(release), Some(build)) => std::format!("{edition} {release} (build {build})"),
        (None, Some(build)) => std::format!("{edition} (build {build})"),
        _ => edition,
    };
    facts.push((String::from("OS"), os));

    facts.push((
        String::from("Uptime"),
        format_uptime(cash_win32::sysinfo::uptime()),
    ));

    // The shell reporting on itself: the name it was invoked by, so `sh` says `sh` (D7).
    let shell_name = context
        .shell
        .current_shell_name()
        .map_or_else(|| String::from("cash"), |name| trim_exe(name.as_ref()));
    // The product's version, which the shell was handed at startup and reports as
    // `$BRUSH_VERSION` — not this crate's, which is a different number that happens to be
    // nearby.
    let version = context.shell.version().unwrap_or("").to_string();
    facts.push((
        String::from("Shell"),
        if version.is_empty() {
            shell_name
        } else {
            std::format!("{shell_name} {version}")
        },
    ));

    facts.push((String::from("Terminal"), terminal_name(context)));

    let cpu = cash_win32::sysinfo::cpu_name().unwrap_or_else(|| String::from("?"));
    let cores = std::thread::available_parallelism().map_or(0, std::num::NonZero::get);
    facts.push((String::from("CPU"), std::format!("{cpu} ({cores})")));
    for gpu in cash_win32::sysinfo::gpu_names() {
        facts.push((String::from("GPU"), gpu));
    }

    if let (Some(total), Some(available)) = (
        cash_win32::process::total_physical_memory(),
        cash_win32::process::available_physical_memory(),
    ) {
        facts.push((String::from("Memory"), format_usage(total, available)));
    }

    for drive in cash_win32::sysinfo::logical_drives() {
        // Query the root, not `D:`, which means that drive's current directory.
        let root = std::path::PathBuf::from(std::format!("{drive}/"));
        if cash_win32::sysinfo::is_network_drive(&root) {
            facts.push((
                std::format!("Disk ({drive})"),
                String::from("Network drive"),
            ));
            continue;
        }
        let Some((total, free)) = cash_win32::sysinfo::disk_usage(&root) else {
            continue;
        };
        facts.push((std::format!("Disk ({drive})"), format_usage(total, free)));
    }

    let addresses = cash_win32::sysinfo::local_ipv4_addresses();
    if !addresses.is_empty() {
        facts.push((String::from("Local IP"), addresses.join(", ")));
    }

    facts.push((
        String::from("Processes"),
        cash_win32::process::list().len().to_string(),
    ));

    facts
}

/// Format existing counters without sampling or additional system queries.
fn format_usage(total: u64, free: u64) -> String {
    let used = total.saturating_sub(free);
    let percent = if total == 0 {
        0
    } else {
        (u128::from(used) * 100 + u128::from(total) / 2) / u128::from(total)
    };
    std::format!(
        "{} / {} ({percent}%)",
        format_bytes(used),
        format_bytes(total)
    )
}

fn format_bytes(bytes: u64) -> String {
    let (scale, unit) = if bytes >= 1 << 40 {
        (1u64 << 40, "TiB")
    } else if bytes >= 1 << 30 {
        (1 << 30, "GiB")
    } else if bytes >= 1 << 20 {
        (1 << 20, "MiB")
    } else if bytes >= 1 << 10 {
        (1 << 10, "KiB")
    } else {
        return std::format!("{bytes} B");
    };
    let hundredths = (u128::from(bytes) * 100 + u128::from(scale) / 2) / u128::from(scale);
    std::format!("{}.{:02} {unit}", hundredths / 100, hundredths % 100)
}

/// Windows Terminal, the classic console, or whatever said so.
///
/// cash: `$TERM_PROGRAM` is the modern convention and `$WT_SESSION` is what Windows
/// Terminal has always set. Nothing here guesses beyond what one of them says.
fn terminal_name<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
) -> String {
    let env = context.shell.env();

    if let Some(program) = env.get_str("TERM_PROGRAM", context.shell)
        && !program.is_empty()
    {
        return program.to_string();
    }

    if env.get_str("WT_SESSION", context.shell).is_some() {
        return String::from("Windows Terminal");
    }

    String::from("Windows Console")
}

/// The shell's name without the extension Windows adds, as `\s` reports it.
fn trim_exe(name: &str) -> String {
    let base = std::path::Path::new(name).file_name().map_or_else(
        || name.to_string(),
        |part| part.to_string_lossy().to_string(),
    );

    base.strip_suffix(".exe")
        .or_else(|| base.strip_suffix(".EXE"))
        .map_or_else(|| base.clone(), ToString::to_string)
}

/// `3 days, 4 hours, 12 mins`, the way these banners have always put it.
fn format_uptime(uptime: std::time::Duration) -> String {
    let seconds = uptime.as_secs();
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;

    let mut parts: Vec<String> = Vec::new();
    if days > 0 {
        parts.push(plural(days, "day"));
    }
    if hours > 0 {
        parts.push(plural(hours, "hour"));
    }
    if minutes > 0 || parts.is_empty() {
        parts.push(plural(minutes, "min"));
    }

    parts.join(", ")
}

fn plural(count: u64, unit: &str) -> String {
    if count == 1 {
        std::format!("{count} {unit}")
    } else {
        std::format!("{count} {unit}s")
    }
}
