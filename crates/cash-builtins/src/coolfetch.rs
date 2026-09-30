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
//!
//! In Windows Terminal the logo is the real one, a sixel picture: the format DEC's
//! terminals drew images in, which Windows Terminal reads from 1.22 on. It is some ten
//! kilobytes, encoded once by `assets/make-sixel.ps1` and written out as it is, so drawing
//! it decodes nothing either.

use std::io::Write;

use cash_core::{ExecutionResult, builtins};
use clap::Parser;

/// Show what this machine is.
#[derive(Parser)]
pub(crate) struct CoolfetchCommand {
    /// Which logo to draw: `auto` is the picture in Windows Terminal and the text logo
    /// anywhere else.
    #[arg(long = "logo", value_enum, default_value_t = Logo::Auto)]
    logo: Logo,

    /// Leave the logo out and print only the facts, as `--logo=none` does.
    #[arg(long = "no-logo")]
    no_logo: bool,

    /// Never colour the output, even on a terminal.
    #[arg(long = "no-color", alias = "no-colour")]
    no_color: bool,
}

/// The logo beside the facts.
#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum Logo {
    /// The picture in a Windows Terminal window with room for it; the text logo in a
    /// smaller window or any other terminal, and when the output is not a terminal or is
    /// not to be coloured.
    Auto,
    /// The picture, wherever the output goes.
    Image,
    /// The four panes, in text.
    Ascii,
    /// Only the facts.
    None,
}

/// The logo as a picture: `assets/cash_logo.six`, which `assets/make-sixel.ps1` makes
/// from the PNG. A terminal lays each character cell over 10x20 of a sixel image's
/// pixels, whatever the font, so its 280x280 pixels cover 28 columns by 14 rows. It is
/// larger than the panes because a display scaled past 100% stretches those pixels over
/// bigger cells, and the softened edges are a smaller part of a larger picture.
const IMAGE: &[u8] = include_bytes!("../../../assets/cash_logo.six");
const IMAGE_ROWS: usize = 14;
const IMAGE_COLUMNS: usize = 28;
/// Where the picture starts, from zero: level with `OS`, below the title and its rule,
/// and one column in.
const IMAGE_TOP: usize = 2;
const IMAGE_LEFT: usize = 1;
/// The facts' column beside the picture, one clear of it.
const IMAGE_GUTTER: usize = IMAGE_LEFT + IMAGE_COLUMNS + 1;

/// The four panes, drawn small enough to sit beside the facts: from the `OS` line to the
/// first disk, split beside `Terminal`, the title and its rule standing clear above.
const LOGO: &[&str] = &[
    "",
    "",
    "  #######  #######  ",
    "  #######  #######  ",
    "  #######  #######  ",
    "  #######  #######  ",
    "",
    "  #######  #######  ",
    "  #######  #######  ",
    "  #######  #######  ",
    "  #######  #######  ",
];

impl builtins::Command for CoolfetchCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        // The command's own standard output, which a pipeline or a redirection replaces
        // even though the shell's is still the terminal.
        let terminal = context.try_fd(1).is_some_and(|f| f.is_terminal());
        let colour = !self.no_color && terminal;
        let facts = collect(&context);
        let logo = match self.logo {
            _ if self.no_logo => Logo::None,
            Logo::Auto if colour && in_windows_terminal(&context) && fits(&facts) => Logo::Image,
            Logo::Auto => Logo::Ascii,
            chosen => chosen,
        };

        let mut stdout = context.stdout();
        match logo {
            Logo::Image => beside_image(&mut stdout, &facts, colour)?,
            Logo::Ascii => beside_text(&mut stdout, &facts, LOGO, colour)?,
            Logo::None | Logo::Auto => beside_text(&mut stdout, &facts, &[], colour)?,
        }

        Ok(ExecutionResult::success())
    }
}

/// The facts, with the text logo's lines, or none, down their left.
fn beside_text(
    stdout: &mut impl Write,
    facts: &[(String, String)],
    logo: &[&str],
    colour: bool,
) -> std::io::Result<()> {
    let rows = logo.len().max(facts.len());
    // The facts outnumber the logo's lines, so the ones past its end still need its
    // width — otherwise the tail of the list jumps back to column zero.
    let gutter = logo.iter().map(|line| line.len()).max().unwrap_or(0);

    for row in 0..rows {
        let art = logo.get(row).copied().unwrap_or("");
        let fact = facts.get(row);
        if !logo.is_empty() {
            // Art alone on its row keeps no trailing spaces.
            let art = if fact.is_some() {
                std::format!("{art:gutter$}")
            } else {
                art.trim_end().to_owned()
            };
            write!(stdout, "{}", paint(&art, "34", colour))?;
        }

        if let Some(fact) = fact {
            write_fact(stdout, fact, colour)?;
        } else {
            writeln!(stdout)?;
        }
    }
    Ok(())
}

/// The facts, with the picture down their left.
///
/// Room is made before the picture is drawn, a line feed for each of its rows and back
/// up, so that drawing it at the bottom of the window scrolls nothing. The cursor is
/// saved around it (DECSC, DECRC), so the facts land in the same place wherever the
/// terminal leaves the cursor after an image, and a terminal that cannot draw one, which
/// throws the sequence away, shows a gap. Each fact goes straight to its column, since
/// spaces written over the picture would erase it.
fn beside_image(
    stdout: &mut impl Write,
    facts: &[(String, String)],
    colour: bool,
) -> std::io::Result<()> {
    let column = IMAGE_GUTTER + 1;
    for (row, fact) in facts.iter().enumerate() {
        if row == IMAGE_TOP {
            let room = "\n".repeat(IMAGE_ROWS);
            write!(
                stdout,
                "{room}\x1b[{IMAGE_ROWS}A\x1b7\x1b[{}G",
                IMAGE_LEFT + 1
            )?;
            stdout.write_all(IMAGE)?;
            write!(stdout, "\x1b8")?;
        }
        write!(stdout, "\x1b[{column}G")?;
        write_fact(stdout, fact, colour)?;
    }
    // Fewer facts than the picture is tall: finish below it.
    for _ in facts.len()..IMAGE_TOP + IMAGE_ROWS {
        writeln!(stdout)?;
    }
    Ok(())
}

fn write_fact(
    stdout: &mut impl Write,
    (label, value): &(String, String),
    colour: bool,
) -> std::io::Result<()> {
    if label.is_empty() {
        writeln!(stdout, "{}", paint(value, "36", colour))
    } else {
        writeln!(stdout, "{}: {value}", paint(label.as_str(), "36;1", colour))
    }
}

/// Whether this is Windows Terminal, which draws sixel pictures from 1.22 on; an older one
/// shows a gap. The `Terminal` line's test, `$TERM_PROGRAM` first: `$WT_SESSION` is
/// inherited, and is still set in VS Code's terminal when VS Code was started from a
/// Windows Terminal tab.
fn in_windows_terminal<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
) -> bool {
    terminal_name(context) == "Windows Terminal"
}

/// Whether the window has room for the picture with the facts beside it. Making room for
/// more rows than the window has would scroll the top of the picture away, and a fact too
/// long for its line wraps onto the picture and erases the cells it lands on.
fn fits(facts: &[(String, String)]) -> bool {
    let widest = facts
        .iter()
        .map(|(label, value)| {
            let label = label.chars().count();
            value.chars().count() + if label == 0 { 0 } else { label + 2 }
        })
        .max()
        .unwrap_or(0);
    crossterm::terminal::size().is_ok_and(|(columns, rows)| {
        usize::from(rows) > IMAGE_TOP + IMAGE_ROWS && usize::from(columns) >= IMAGE_GUTTER + widest
    })
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

    // What `uname -s` says, and the NT version `ver` prints.
    let kernel = cash_win32::sysinfo::nt_version().map_or_else(
        || String::from("Windows_NT"),
        |version| std::format!("Windows_NT {version}"),
    );
    facts.push((String::from("Kernel"), kernel));

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

#[cfg(test)]
mod tests {
    use super::{IMAGE, IMAGE_COLUMNS, IMAGE_ROWS};

    #[test]
    fn the_picture_is_the_size_the_layout_leaves_for_it() {
        // Its raster attributes, `"1;1;width;height` after the introducer, at 10x20
        // pixels a cell: a picture made at another size would overlap the facts or leave
        // the rows below it short.
        let sixel = std::str::from_utf8(IMAGE).unwrap();
        let size = sixel.strip_prefix("\x1bP9;1q\"1;1;").unwrap();
        let size: Vec<usize> = size
            .split(|c: char| !c.is_ascii_digit())
            .take(2)
            .map(|n| n.parse().unwrap())
            .collect();
        assert_eq!(size, [IMAGE_COLUMNS * 10, IMAGE_ROWS * 20]);
        assert!(sixel.ends_with("\x1b\\"), "the picture is never finished");
    }
}
