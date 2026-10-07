//! `l`: 7-Zip's `ListArchives`, the columns or, with `-slt`, every property of every item.

use super::archive::{Item, Prop};
use super::cmdline::Options;
use super::extract::{Found, archive_props, forced_kind, open_asking, open_error};
use super::{Console, Env, Stop, code, text};
use std::fmt::Write as _;

/// Sums over the items listed: `CListStat`.
#[derive(Default, Clone, Copy)]
struct Stat {
    size: Option<u64>,
    packed: Option<u64>,
    modified: Option<u64>,
    files: u64,
    dirs: u64,
}

impl Stat {
    fn add(&mut self, other: &Self) {
        if let Some(size) = other.size {
            self.size = Some(self.size.unwrap_or(0) + size);
        }
        if let Some(packed) = other.packed {
            self.packed = Some(self.packed.unwrap_or(0) + packed);
        }
        if other.modified > self.modified {
            self.modified = other.modified;
        }
        self.files += other.files;
        self.dirs += other.dirs;
    }
}

const LINES: &str = "------------------- ----- ------------ ------------  ------------------------";
const TITLE: &str = "   Date      Time    Attr         Size   Compressed  Name";

fn right(text: &str, width: usize) -> String {
    format!("{text:>width$}")
}

fn number(value: Option<u64>, width: usize) -> String {
    right(&value.map(|v| v.to_string()).unwrap_or_default(), width)
}

#[expect(
    clippy::too_many_lines,
    reason = "7-Zip's ListArchives, archive by archive"
)]
pub(super) fn run<SE: cash_core::ShellExtensions>(
    options: &Options,
    env: &Env<'_, SE>,
    console: &Console<'_, SE>,
    found: &[Found],
) -> Result<u8, Stop> {
    let mut errors = 0u64;
    let mut warnings = 0u64;
    let mut total = Stat::default();
    let mut archives = 0u64;
    let mut total_size = 0u64;
    let mut password = options.password.clone();
    let censor = &options.censor;
    let all = censor.all_allowed();
    for archive in found {
        if archive.size == u64::MAX {
            console.flush_stdout();
            console.se(&format!("\nERROR: {} is not a file\n\n", archive.name));
            errors += 1;
            continue;
        }
        total_size += archive.size;
        if options.headers {
            console.stdout(&format!(
                "\nListing archive: {}\n\n",
                archive.name.replace('\\', "/")
            ));
        }
        let mut asked = false;
        let opened = open_asking(
            archive,
            forced_kind(options),
            env,
            &mut password,
            &|t| console.stdout(t),
            &mut asked,
        )?;
        let opened = match opened {
            Ok(opened) => opened,
            Err(failure) => {
                console.flush_stdout();
                if let super::archive::OpenFailure::Io(error) = &failure {
                    console.se(&format!(
                        "\nERROR: {} : opening : {}\n",
                        archive.name.replace('\\', "/"),
                        text::system_message(error)
                    ));
                } else {
                    console.se(&format!(
                        "\nERROR: {} : {}\n",
                        archive.name.replace('\\', "/"),
                        open_error(archive, &failure, asked)
                    ));
                }
                errors += 1;
                continue;
            }
        };
        if !opened.warnings().is_empty() || opened.type_warning.is_some() {
            warnings += 1;
        }
        if !opened.error_flags.is_empty() {
            errors += 1;
        }
        archives += 1;
        if options.headers {
            console.stdout(&archive_props(archive, &opened));
            console.stdout("\n");
            if options.tech {
                console.stdout("----------\n");
            } else {
                console.stdout(&format!("{TITLE}\n{LINES}\n"));
            }
        }
        let mut stat = Stat::default();
        for item in &opened.items {
            if item.is_dir && options.exclude_dirs || !item.is_dir && options.exclude_files {
                continue;
            }
            if !all && !censor.wants(&item.path, item.is_dir) {
                continue;
            }
            let one = Stat {
                size: item.size,
                packed: item.packed,
                modified: item.modified,
                files: u64::from(!item.is_dir),
                dirs: u64::from(item.is_dir),
            };
            stat.add(&one);
            if options.tech {
                console.stdout(&technical(env, &opened.item_props, item));
            } else {
                console.stdout(&line(env, item));
            }
        }
        if stat.packed.is_none() {
            stat.packed = Some(if stat.files + stat.dirs == 0 {
                0
            } else {
                archive.size
            });
        }
        if stat.files == 0 {
            stat.size.get_or_insert(0);
        }
        if options.headers && !options.tech {
            console.stdout(&format!("{LINES}\n"));
            console.stdout(&sum(env, &stat));
        }
        total.add(&stat);
        console.flush_stdout();
    }
    if options.headers && !options.tech && found.len() > 1 {
        if total.files == 0 {
            total.size.get_or_insert(0);
        }
        console.stdout(&format!("\n{LINES}\n"));
        console.stdout(&sum(env, &total));
        console.stdout(&format!(
            "\nArchives: {archives}\nVolumes: {archives}\nTotal archives size: {total_size}\n"
        ));
    }
    if options.headers && warnings > 0 {
        console.stdout(&format!("\nWarnings: {warnings}\n"));
    }
    if errors > 0 {
        if options.headers {
            console.stdout(&format!("\nErrors: {errors}\n"));
        }
        return Ok(code::FATAL);
    }
    Ok(0)
}

/// One item's line in the columns.
fn line<SE: cash_core::ShellExtensions>(env: &Env<'_, SE>, item: &Item) -> String {
    let time = item
        .modified
        .map_or_else(String::new, |t| text::time(&env.zone, t, 0));
    format!(
        "{time:<19} {} {} {}  {}\n",
        text::attributes_short(item.attrib.unwrap_or(0), item.is_dir),
        number(item.size, 12),
        number(item.packed, 12),
        item.path.replace('\\', "/")
    )
}

/// The totals' line: the newest time, the sizes, "N files, M folders".
fn sum<SE: cash_core::ShellExtensions>(env: &Env<'_, SE>, stat: &Stat) -> String {
    let time = stat
        .modified
        .map_or_else(String::new, |t| text::time(&env.zone, t, 0));
    let mut names = format!("{} files", stat.files);
    if stat.dirs != 0 {
        let _ = write!(names, ", {} folders", stat.dirs);
    }
    format!(
        "{time:<19} {:5} {} {}  {names}\n",
        "",
        number(stat.size, 12),
        number(stat.packed, 12)
    )
}

/// `-slt`: each property on its line, an empty line after the item.
fn technical<SE: cash_core::ShellExtensions>(
    env: &Env<'_, SE>,
    props: &[Prop],
    item: &Item,
) -> String {
    let mut out = String::new();
    for prop in props {
        let value = match prop {
            Prop::Path => item.path.replace('\\', "/"),
            Prop::Size => item.size.map(|v| v.to_string()).unwrap_or_default(),
            Prop::PackedSize => item.packed.map(|v| v.to_string()).unwrap_or_default(),
            Prop::Modified => item.modified.map_or_else(String::new, |t| {
                text::time_ns(&env.zone, t, item.time_extra[0], item.time_digits[0])
            }),
            Prop::Created => item.created.map_or_else(String::new, |t| {
                text::time_ns(&env.zone, t, item.time_extra[1], item.time_digits[1])
            }),
            Prop::Accessed => item.accessed.map_or_else(String::new, |t| {
                text::time_ns(&env.zone, t, item.time_extra[2], item.time_digits[2])
            }),
            Prop::Anti => if item.anti { "+" } else { "-" }.to_owned(),
            Prop::Attributes => text::attributes_long(item.attrib.unwrap_or(0), item.is_dir),
            Prop::Crc => item.crc.map(|c| format!("{c:08X}")).unwrap_or_default(),
            Prop::Encrypted => if item.encrypted { "+" } else { "-" }.to_owned(),
            Prop::Method => item.method.clone().unwrap_or_default(),
            Prop::Block => item.block.map(|b| b.to_string()).unwrap_or_default(),
            Prop::HostOs => item.host_os.clone().unwrap_or_default(),
            other => item
                .extra
                .iter()
                .find(|(prop, _)| *prop == *other)
                .map(|(_, value)| value.clone())
                .unwrap_or_default(),
        };
        let _ = writeln!(out, "{} = {value}", prop.name());
    }
    out.push('\n');
    out
}
