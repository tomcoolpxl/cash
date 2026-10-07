//! `free`, procps-ng's, on Windows' memory counters.
//!
//! Options, layout and number formatting are procps-ng 4.0.7's, read off its output
//! (`free`, `free -h`, `free -w -t`, `free -L`) and its `scale_size`. The columns come
//! from Windows like this:
//!
//! * `total` is the installed physical memory and `available` what Windows can hand out
//!   at once (`PhysicalAvailable`: free, zeroed and standby pages), Linux's
//!   `MemAvailable`; `used` is `total - available`, as procps 4 computes it;
//! * `buff/cache` is the system cache (`SystemCache`: the standby list, which holds
//!   file pages that can be dropped, plus the system's working set), capped at what is
//!   available; `free` is what is available beyond it, so the three add up to `total`
//!   as they do on Linux; `shared` is 0, which Windows does not count;
//! * `Swap:` is the page files together, their sizes and the part in use; zeros without
//!   a page file; `Comm:` (`-v`) is the commit limit and the commit charge.
//!
//! `-s` repeats until Ctrl-C, which the shell then acts on as it does between commands.

use std::fmt::Write as _;
use std::io::Write as _;
use std::time::Duration;

use cash_core::{ExecutionResult, builtins};
use cash_getopt::{Arg, Getopt, Item, Long, Short};
use cash_win32::memory::{MemoryCounts, PageFiles};
use clap::Parser;

/// Display the amount of free and used memory.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct FreeCommand {
    /// Options, parsed here.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// procps-ng 4.0.7's `free --help`.
const USAGE: &str = "\
Usage:
 free [options]

Options:
 -b, --bytes         show output in bytes
     --kilo          show output in kilobytes
     --mega          show output in megabytes
     --giga          show output in gigabytes
     --tera          show output in terabytes
     --peta          show output in petabytes
 -k, --kibi          show output in kibibytes
 -m, --mebi          show output in mebibytes
 -g, --gibi          show output in gibibytes
     --tebi          show output in tebibytes
     --pebi          show output in pebibytes
 -h, --human         show human-readable output
     --si            use powers of 1000 not 1024
 -l, --lohi          show detailed low and high memory statistics
 -L, --line          show output on a single line
 -t, --total         show total for RAM + swap
 -v, --committed     show committed memory and commit limit
 -s N, --seconds N   repeat printing every N seconds
 -c N, --count N     repeat printing N times, then exit
 -w, --wide          wide output

     --help     display this help and exit
 -V, --version  output version information and exit

For more details see free(1).
";

/// How numbers are scaled: procps's `args.exponent`, 1 for bytes, 2 for the first unit,
/// and so on; 0 is the default, the first unit as well.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Scale {
    /// 0 for bytes, 1 for K, 2 for M, 3 for G, 4 for T, 5 for P.
    power: u32,
    /// Powers of 1000 rather than 1024.
    si: bool,
    /// `-h`: a unit chosen per number, with a suffix.
    human: bool,
}

impl Scale {
    const fn base(self) -> u64 {
        if self.si { 1000 } else { 1024 }
    }

    /// `bytes` as procps's `scale_size` prints it.
    fn format(self, bytes: u64) -> String {
        if !self.human {
            let divisor = self.base().saturating_pow(self.power);
            return (bytes / divisor.max(1)).to_string();
        }
        let plain = std::format!("{bytes}B");
        if plain.len() <= 4 {
            return plain;
        }
        // A unit at a time, the first whose number fits: one decimal if that fits, the
        // whole number otherwise. The binary suffixes carry an `i`, so they get a
        // character more.
        let width = if self.si { 4 } else { 5 };
        let suffix = if self.si { "" } else { "i" };
        let base = self.base();
        #[expect(
            clippy::cast_precision_loss,
            reason = "memory sizes are far below f64's integer limit"
        )]
        let bytes_f = bytes as f64;
        for (power, unit) in ['K', 'M', 'G', 'T', 'P'].into_iter().enumerate() {
            let divisor = base.saturating_pow(u32::try_from(power + 1).unwrap_or(5));
            #[expect(
                clippy::cast_precision_loss,
                reason = "powers of 1000 or 1024 up to the fifth are exact in f64"
            )]
            let scaled = bytes_f / divisor as f64;
            let decimal = std::format!("{scaled:.1}{unit}{suffix}");
            if decimal.len() <= width {
                return decimal;
            }
            let whole = std::format!("{}{unit}{suffix}", scaled.trunc());
            if whole.len() <= width {
                return whole;
            }
        }
        // Past petabytes: procps falls back to the plain byte count.
        plain
    }
}

/// What the command line asked for.
#[derive(Debug, PartialEq)]
struct Options {
    scale: Scale,
    lohi: bool,
    line: bool,
    total: bool,
    committed: bool,
    wide: bool,
    /// `-s`: the pause between repeats.
    seconds: Option<Duration>,
    /// `-c`: how many times to print.
    count: Option<u64>,
}

/// Why the command line was refused: the message, and whether the usage follows it.
struct Refused {
    message: String,
    usage: bool,
}

impl Refused {
    fn with_usage(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            usage: true,
        }
    }

    fn plain(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            usage: false,
        }
    }
}

/// What one option asks for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Flag {
    Help,
    Version,
    Unit { power: u32, si: bool },
    Human,
    Si,
    Lohi,
    Line,
    Total,
    Committed,
    Wide,
    Seconds,
    Count,
}

/// A unit option's flag.
const fn unit(power: u32, si: bool) -> Flag {
    Flag::Unit { power, si }
}

/// procps 4.0's short options, each known by its flag.
const SHORT_OPTIONS: &[Short<Flag>] = &[
    Short::new('b', Arg::No, unit(0, false)),
    Short::new('k', Arg::No, unit(1, false)),
    Short::new('m', Arg::No, unit(2, false)),
    Short::new('g', Arg::No, unit(3, false)),
    Short::new('h', Arg::No, Flag::Human),
    Short::new('l', Arg::No, Flag::Lohi),
    Short::new('L', Arg::No, Flag::Line),
    Short::new('t', Arg::No, Flag::Total),
    Short::new('v', Arg::No, Flag::Committed),
    Short::new('w', Arg::No, Flag::Wide),
    Short::new('s', Arg::Required, Flag::Seconds),
    Short::new('c', Arg::Required, Flag::Count),
    Short::new('V', Arg::No, Flag::Version),
];

/// procps 4.0's long options, in its table's order, which an ambiguity message follows.
const LONG_OPTIONS: &[Long<'static, Flag>] = &[
    Long::new("bytes", Arg::No, unit(0, false)),
    Long::new("kilo", Arg::No, unit(1, true)),
    Long::new("mega", Arg::No, unit(2, true)),
    Long::new("giga", Arg::No, unit(3, true)),
    Long::new("tera", Arg::No, unit(4, true)),
    Long::new("peta", Arg::No, unit(5, true)),
    Long::new("kibi", Arg::No, unit(1, false)),
    Long::new("mebi", Arg::No, unit(2, false)),
    Long::new("gibi", Arg::No, unit(3, false)),
    Long::new("tebi", Arg::No, unit(4, false)),
    Long::new("pebi", Arg::No, unit(5, false)),
    Long::new("human", Arg::No, Flag::Human),
    Long::new("si", Arg::No, Flag::Si),
    Long::new("lohi", Arg::No, Flag::Lohi),
    Long::new("line", Arg::No, Flag::Line),
    Long::new("total", Arg::No, Flag::Total),
    Long::new("committed", Arg::No, Flag::Committed),
    Long::new("seconds", Arg::Required, Flag::Seconds),
    Long::new("count", Arg::Required, Flag::Count),
    Long::new("wide", Arg::No, Flag::Wide),
    Long::new("help", Arg::No, Flag::Help),
    Long::new("version", Arg::No, Flag::Version),
];

/// What a parse ends in.
enum Parsed {
    Run(Options),
    Help,
    Version,
}

/// procps's `-s` argument: a positive number of seconds.
fn parse_seconds(text: &str) -> Result<Duration, Refused> {
    let seconds: f64 = text.trim().parse().map_err(|_| {
        Refused::plain(std::format!(
            "seconds argument failed: '{text}': Invalid argument"
        ))
    })?;
    if seconds.is_nan() || seconds <= 0.0 || seconds.is_infinite() {
        return Err(Refused::plain(std::format!(
            "seconds argument `{text}' is not positive number"
        )));
    }
    Ok(Duration::from_secs_f64(seconds))
}

/// procps's `-c` argument: a count of one or more.
fn parse_count(text: &str) -> Result<u64, Refused> {
    match text.trim().parse::<i64>() {
        Ok(count) if count >= 1 => Ok(count.unsigned_abs()),
        Ok(_) => Err(Refused::plain(std::format!(
            "failed to parse count argument: '{text}': Numerical result out of range"
        ))),
        Err(_) => Err(Refused::plain(std::format!(
            "failed to parse count argument: '{text}'"
        ))),
    }
}

/// Reads the command line as procps's `getopt_long` loop does (`cash-getopt`): each
/// option acted on as it comes, so `free -Vz` prints the version.
fn parse(args: &[String]) -> Result<Parsed, Refused> {
    let mut scale = Scale {
        power: 1,
        si: false,
        human: false,
    };
    let mut unit_set = false;
    let mut options = Options {
        scale,
        lohi: false,
        line: false,
        total: false,
        committed: false,
        wide: false,
        seconds: None,
        count: None,
    };
    for next in Getopt::new(SHORT_OPTIONS, LONG_OPTIONS).read(args) {
        let (flag, value) = match next {
            Ok(Item::Option { id, value, .. }) => (id, value),
            // `free` takes no operands, before `--` or after it.
            Ok(Item::Operand { .. }) => return Err(Refused::with_usage(String::new())),
            Err(problem) => return Err(Refused::with_usage(problem.to_string())),
        };
        match flag {
            Flag::Help => return Ok(Parsed::Help),
            Flag::Version => return Ok(Parsed::Version),
            Flag::Unit { power, si } => {
                if unit_set {
                    return Err(Refused::plain("Multiple unit options don't make sense."));
                }
                unit_set = true;
                scale.power = power;
                scale.si |= si;
            }
            Flag::Human => scale.human = true,
            Flag::Si => scale.si = true,
            Flag::Lohi => options.lohi = true,
            Flag::Line => options.line = true,
            Flag::Total => options.total = true,
            Flag::Committed => options.committed = true,
            Flag::Wide => options.wide = true,
            Flag::Seconds => options.seconds = Some(parse_seconds(&value.unwrap_or_default())?),
            Flag::Count => options.count = Some(parse_count(&value.unwrap_or_default())?),
        }
    }
    options.scale = scale;
    Ok(Parsed::Run(options))
}

/// The figures of one report, in bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Figures {
    total: u64,
    used: u64,
    free: u64,
    shared: u64,
    buffers: u64,
    cache: u64,
    available: u64,
    swap_total: u64,
    swap_used: u64,
    swap_free: u64,
    commit_limit: u64,
    commit_used: u64,
}

impl Figures {
    /// What Windows' counters mean in `free`'s columns (see the module's notes).
    fn from_windows(memory: MemoryCounts, swap: PageFiles) -> Self {
        let total = memory.physical_total;
        let available = memory.physical_available.min(total);
        let cache = memory.system_cache.min(available);
        Self {
            total,
            used: total - available,
            free: available - cache,
            shared: 0,
            buffers: 0,
            cache,
            available,
            swap_total: swap.total,
            swap_used: swap.used.min(swap.total),
            swap_free: swap.total - swap.used.min(swap.total),
            commit_limit: memory.commit_limit,
            commit_used: memory.commit_total.min(memory.commit_limit),
        }
    }

    fn from_machine() -> Option<Self> {
        let memory = cash_win32::memory::counts()?;
        let swap = cash_win32::memory::page_files(memory.page_size)?;
        Some(Self::from_windows(memory, swap))
    }
}

/// One report, as procps lays it out: the header, `Mem:`, `Swap:` and the optional rows,
/// or the `-L` line.
fn report(figures: &Figures, options: &Options) -> String {
    let scale = options.scale;
    let n = |bytes: u64| scale.format(bytes);
    let mut out = String::new();
    if options.line {
        for (label, value) in [
            ("SwapUse", figures.swap_used),
            ("CachUse", figures.buffers + figures.cache),
            ("MemUse", figures.used),
            ("MemFree", figures.free),
        ] {
            // Writing to a String cannot fail.
            let _ = write!(out, "{label:>7} {:>11} ", n(value));
        }
        out.push('\n');
        return out;
    }
    let row = |out: &mut String, label: &str, values: &[u64]| {
        let _ = write!(out, "{label:<9}");
        for (index, value) in values.iter().enumerate() {
            if index > 0 {
                out.push(' ');
            }
            let _ = write!(out, "{:>11}", n(*value));
        }
        out.push('\n');
    };
    if options.wide {
        out.push_str(
            "               total        used        free      shared     buffers       cache   available\n",
        );
        row(
            &mut out,
            "Mem:",
            &[
                figures.total,
                figures.used,
                figures.free,
                figures.shared,
                figures.buffers,
                figures.cache,
                figures.available,
            ],
        );
    } else {
        out.push_str(
            "               total        used        free      shared  buff/cache   available\n",
        );
        row(
            &mut out,
            "Mem:",
            &[
                figures.total,
                figures.used,
                figures.free,
                figures.shared,
                figures.buffers + figures.cache,
                figures.available,
            ],
        );
    }
    if options.lohi {
        // Linux's low memory is all of it on 64-bit machines, and `used` there is counted
        // without the cache, as procps prints it; high memory is none.
        row(
            &mut out,
            "Low:",
            &[figures.total, figures.total - figures.free, figures.free],
        );
        row(&mut out, "High:", &[0, 0, 0]);
    }
    row(
        &mut out,
        "Swap:",
        &[figures.swap_total, figures.swap_used, figures.swap_free],
    );
    if options.committed {
        row(
            &mut out,
            "Comm:",
            &[
                figures.commit_limit,
                figures.commit_used,
                figures.commit_limit - figures.commit_used,
            ],
        );
    }
    if options.total {
        row(
            &mut out,
            "Total:",
            &[
                figures.total + figures.swap_total,
                figures.used + figures.swap_used,
                figures.free + figures.swap_free,
            ],
        );
    }
    out
}

impl builtins::Command for FreeCommand {
    type Error = cash_core::Error;

    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        Ok(Self {
            args: args.into_iter().skip(1).collect(),
        })
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let options = match parse(&self.args) {
            Ok(Parsed::Run(options)) => options,
            Ok(Parsed::Help) => {
                write!(context.stdout(), "{USAGE}")?;
                return Ok(ExecutionResult::success());
            }
            Ok(Parsed::Version) => {
                writeln!(context.stdout(), "free (cash): procps-ng 4.0.7's options")?;
                return Ok(ExecutionResult::success());
            }
            Err(refused) => {
                let mut stderr = context.stderr();
                if !refused.message.is_empty() {
                    writeln!(stderr, "free: {}", refused.message)?;
                }
                if refused.usage {
                    write!(stderr, "\n{USAGE}")?;
                }
                return Ok(ExecutionResult::general_error());
            }
        };

        // `-c` without `-s` repeats every second, as procps does.
        let pause = match (options.seconds, options.count) {
            (Some(pause), _) => Some(pause),
            (None, Some(_)) => Some(Duration::from_secs(1)),
            (None, None) => None,
        };
        let mut stdout = context.stdout();
        let mut printed = 0u64;
        loop {
            let Some(figures) = Figures::from_machine() else {
                writeln!(
                    context.stderr(),
                    "free: Windows would not report the machine's memory"
                )?;
                return Ok(ExecutionResult::general_error());
            };
            if printed > 0 {
                writeln!(stdout)?;
            }
            write!(stdout, "{}", report(&figures, &options))?;
            stdout.flush()?;
            printed += 1;
            let Some(pause) = pause else { break };
            if options.count.is_some_and(|count| printed >= count) {
                break;
            }
            // In slices, so that a Ctrl-C ends the loop within a moment rather than after
            // the pause; the shell then acts on it as it does between commands (D13). A
            // `free -s N &` leaves the interrupt to the foreground, as the interpreter does.
            let mut left = pause;
            while !left.is_zero() {
                let slice = left.min(Duration::from_millis(50));
                tokio::time::sleep(slice).await;
                left -= slice;
                if !context.params.is_asynchronous() && cash_win32::console::take_interrupt() {
                    return context.shell.interrupt(&context.params).await;
                }
            }
        }
        Ok(ExecutionResult::success())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "tests assert loudly on failure"
)]
mod tests {
    use super::*;

    const KIB: u64 = 1024;
    const MIB: u64 = KIB * KIB;
    const GIB: u64 = MIB * KIB;

    fn scale(power: u32, si: bool, human: bool) -> Scale {
        Scale { power, si, human }
    }

    #[test]
    fn human_numbers_are_procps_scale_size() {
        let h = scale(1, false, true);
        // Read off `free -h` on Linux: 16334928 KiB, 643744 KiB, 3568 KiB, 4194304 KiB, 0.
        assert_eq!(h.format(16_334_928 * KIB), "15Gi");
        assert_eq!(h.format(643_744 * KIB), "628Mi");
        assert_eq!(h.format(3568 * KIB), "3.5Mi");
        assert_eq!(h.format(4_194_304 * KIB), "4.0Gi");
        assert_eq!(h.format(0), "0B");
        assert_eq!(h.format(999), "999B");
        // Five characters: too wide for the byte form, so the next unit.
        assert_eq!(h.format(1000), "1.0Ki");
        assert_eq!(h.format(10_000), "9.8Ki");
        assert_eq!(h.format(1023 * KIB), "1.0Mi");
        assert_eq!(h.format(100 * GIB), "100Gi");
        assert_eq!(h.format(1024 * GIB), "1.0Ti");
    }

    #[test]
    fn si_human_numbers_use_1000_and_one_letter() {
        let si = scale(1, true, true);
        // `free --si -h` on the same machine: 16726966272, 660221952, 3653632, 4294967296.
        assert_eq!(si.format(16_726_966_272), "16G");
        assert_eq!(si.format(660_221_952), "660M");
        assert_eq!(si.format(3_653_632), "3.7M");
        assert_eq!(si.format(4_294_967_296), "4.3G");
        assert_eq!(si.format(0), "0B");
    }

    #[test]
    fn plain_numbers_divide_and_truncate() {
        let total = 16_726_966_272u64;
        assert_eq!(scale(0, false, false).format(total), "16726966272");
        assert_eq!(scale(1, false, false).format(total), "16334928");
        assert_eq!(scale(2, false, false).format(total), "15952");
        assert_eq!(scale(3, false, false).format(total), "15");
        assert_eq!(scale(4, false, false).format(total), "0");
        assert_eq!(scale(1, true, false).format(total), "16726966");
        assert_eq!(scale(3, true, false).format(total), "16");
    }

    fn figures() -> Figures {
        Figures {
            total: 16 * GIB,
            used: 4 * GIB,
            free: 10 * GIB,
            shared: 0,
            buffers: 0,
            cache: 2 * GIB,
            available: 12 * GIB,
            swap_total: 4 * GIB,
            swap_used: GIB,
            swap_free: 3 * GIB,
            commit_limit: 20 * GIB,
            commit_used: 6 * GIB,
        }
    }

    fn options(args: &[&str]) -> Options {
        let args: Vec<String> = args.iter().map(|&a| a.to_owned()).collect();
        match parse(&args) {
            Ok(Parsed::Run(options)) => options,
            Ok(_) => panic!("help or version for {args:?}"),
            Err(refused) => panic!("{} for {args:?}", refused.message),
        }
    }

    #[test]
    fn the_layout_is_procps() {
        let text = report(&figures(), &options(&["-t"]));
        assert_eq!(
            text,
            "               total        used        free      shared  buff/cache   available\n\
             Mem:        16777216     4194304    10485760           0     2097152    12582912\n\
             Swap:        4194304     1048576     3145728\n\
             Total:      20971520     5242880    13631488\n"
        );
        let wide = report(&figures(), &options(&["-w", "-h"]));
        assert_eq!(
            wide,
            "               total        used        free      shared     buffers       cache   available\n\
             Mem:            16Gi       4.0Gi        10Gi          0B          0B       2.0Gi        12Gi\n\
             Swap:          4.0Gi       1.0Gi       3.0Gi\n"
        );
        let lohi = report(&figures(), &options(&["-l", "-v", "-m"]));
        assert_eq!(
            lohi,
            "               total        used        free      shared  buff/cache   available\n\
             Mem:           16384        4096       10240           0        2048       12288\n\
             Low:           16384        6144       10240\n\
             High:              0           0           0\n\
             Swap:           4096        1024        3072\n\
             Comm:          20480        6144       14336\n"
        );
        let line = report(&figures(), &options(&["-L"]));
        assert_eq!(
            line,
            "SwapUse     1048576 CachUse     2097152  MemUse     4194304 MemFree    10485760 \n"
        );
    }

    #[test]
    fn windows_counters_become_columns_that_add_up() {
        let memory = MemoryCounts {
            page_size: 4096,
            physical_total: 16 * GIB,
            physical_available: 12 * GIB,
            system_cache: 2 * GIB,
            commit_total: 6 * GIB,
            commit_limit: 20 * GIB,
            kernel_paged: 0,
            kernel_nonpaged: 0,
        };
        let swap = PageFiles {
            total: 4 * GIB,
            used: GIB,
        };
        assert_eq!(Figures::from_windows(memory, swap), figures());
        // A cache larger than what is available is capped, and nothing goes negative.
        let crowded = MemoryCounts {
            physical_available: GIB,
            system_cache: 3 * GIB,
            ..memory
        };
        let f = Figures::from_windows(crowded, PageFiles::default());
        assert_eq!(f.used + f.free + f.cache, f.total);
        assert_eq!(f.free, 0);
        assert_eq!((f.swap_total, f.swap_used, f.swap_free), (0, 0, 0));
    }

    #[test]
    fn options_are_read_as_getopt_reads_them() {
        let o = options(&["-htw"]);
        assert!(o.scale.human && o.total && o.wide);
        let o = options(&["-s", "0.5", "-c2"]);
        assert_eq!(o.seconds, Some(Duration::from_millis(500)));
        assert_eq!(o.count, Some(2));
        let o = options(&["--seconds=1.5", "--count", "3", "--si", "--mebi"]);
        assert_eq!(o.seconds, Some(Duration::from_millis(1500)));
        assert_eq!(o.count, Some(3));
        assert_eq!(o.scale, scale(2, true, false));
        assert_eq!(options(&["--kilo"]).scale, scale(1, true, false));
        assert_eq!(options(&["-b"]).scale, scale(0, false, false));
        assert_eq!(options(&[]).scale, scale(1, false, false));
        assert!(matches!(parse(&["--help".to_owned()]), Ok(Parsed::Help)));
        assert!(matches!(parse(&["-V".to_owned()]), Ok(Parsed::Version)));
    }

    fn refused(args: &[&str]) -> Refused {
        let args: Vec<String> = args.iter().map(|&a| a.to_owned()).collect();
        match parse(&args) {
            Err(refused) => refused,
            Ok(_) => panic!("accepted {args:?}"),
        }
    }

    #[test]
    fn mistakes_are_procps_messages() {
        let r = refused(&["-x"]);
        assert_eq!(r.message, "invalid option -- 'x'");
        assert!(r.usage);
        let r = refused(&["--bogus"]);
        assert_eq!(r.message, "unrecognized option '--bogus'");
        assert!(r.usage);
        let r = refused(&["-b", "-k"]);
        assert_eq!(r.message, "Multiple unit options don't make sense.");
        assert!(!r.usage);
        assert_eq!(
            refused(&["-c", "0"]).message,
            "failed to parse count argument: '0': Numerical result out of range"
        );
        assert_eq!(
            refused(&["-c", "abc"]).message,
            "failed to parse count argument: 'abc'"
        );
        assert_eq!(
            refused(&["-s", "abc"]).message,
            "seconds argument failed: 'abc': Invalid argument"
        );
        assert_eq!(
            refused(&["-s", "0"]).message,
            "seconds argument `0' is not positive number"
        );
        let r = refused(&["-s"]);
        assert_eq!(r.message, "option requires an argument -- 's'");
        assert!(r.usage);
        let r = refused(&["extra"]);
        assert_eq!(r.message, "");
        assert!(r.usage);
    }
}
