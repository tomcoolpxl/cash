//! `cargo xtask perf`: what a build of cash costs, its size and how long it takes to
//! start, against the limits in `perf-budget.toml`. The release workflow runs it on the
//! dist build before it publishes, and the figures go in the job's summary.
//!
//! The start is cash's own: the median of `cash -c true` less the median of a bare
//! `cmd /c exit`, so that what Windows takes to start any process, which differs from
//! machine to machine, is not counted.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use clap::Parser;

/// Arguments for `cargo xtask perf`.
#[derive(Parser)]
pub struct PerfArgs {
    /// The cash.exe to measure.
    #[clap(long, default_value = "target/dist/cash.exe")]
    exe: PathBuf,

    /// How many starts each median is taken over.
    #[clap(long, default_value_t = 30)]
    runs: usize,

    /// The file of limits.
    #[clap(long, default_value = "perf-budget.toml")]
    budget: PathBuf,

    /// A Markdown file to add the figures to; `GITHUB_STEP_SUMMARY` when not given.
    #[clap(long)]
    summary: Option<PathBuf>,
}

/// The limits `perf-budget.toml` sets.
struct Budget {
    max_bytes: u64,
    max_own_start_ms: f64,
}

impl Budget {
    /// Reads the budget at `path`.
    fn read(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading the budget {}", path.display()))?;
        Self::parse(&text).with_context(|| path.display().to_string())
    }

    /// The `key = number` lines of `text`; comments and blank lines are skipped.
    fn parse(text: &str) -> Result<Self> {
        let mut max_bytes = None;
        let mut max_own_start_ms = None;
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                bail!("not `key = number`: {line}");
            };
            let value = value.trim().replace('_', "");
            match key.trim() {
                "max_bytes" => max_bytes = Some(value.parse().context("max_bytes")?),
                "max_own_start_ms" => {
                    max_own_start_ms = Some(value.parse().context("max_own_start_ms")?);
                }
                other => bail!("unknown limit `{other}`"),
            }
        }
        Ok(Self {
            max_bytes: max_bytes.context("the budget sets no max_bytes")?,
            max_own_start_ms: max_own_start_ms.context("the budget sets no max_own_start_ms")?,
        })
    }
}

/// Run `cargo xtask perf`.
pub fn run(args: &PerfArgs, verbose: bool) -> Result<()> {
    let budget = Budget::read(&args.budget)?;
    let bytes = std::fs::metadata(&args.exe)
        .with_context(|| {
            format!(
                "{} is not there; build it first: cargo build --profile dist --bin cash",
                args.exe.display()
            )
        })?
        .len();

    let cash = median_ms(args.runs, verbose, || {
        let mut command = Command::new(&args.exe);
        command.args(["--no-config", "-c", "true"]);
        command
    })?;
    let bare = median_ms(args.runs, verbose, || {
        let mut command = Command::new("cmd.exe");
        command.args(["/d", "/c", "exit"]);
        command
    })?;
    let own = (cash - bare).max(0.0);

    let size_ok = bytes <= budget.max_bytes;
    let start_ok = own <= budget.max_own_start_ms;
    let mark = |ok: bool| if ok { "ok" } else { "OVER" };
    println!(
        "cash.exe          {:>8}  budget {:>8}  {}",
        megabytes(bytes),
        megabytes(budget.max_bytes),
        mark(size_ok)
    );
    println!(
        "cash -c true      {cash:>6.1} ms  (median of {})",
        args.runs
    );
    println!("cmd /c exit       {bare:>6.1} ms");
    println!(
        "cash's own start  {own:>6.1} ms  budget {:>5.0} ms  {}",
        budget.max_own_start_ms,
        mark(start_ok)
    );

    let summary = args
        .summary
        .clone()
        .or_else(|| std::env::var_os("GITHUB_STEP_SUMMARY").map(PathBuf::from));
    if let Some(summary) = summary {
        let mut text = String::new();
        let _ = writeln!(text, "### Size and start\n");
        let _ = writeln!(text, "| | measured | budget | |\n|---|---:|---:|---|");
        let _ = writeln!(
            text,
            "| `cash.exe` | {} | {} | {} |",
            megabytes(bytes),
            megabytes(budget.max_bytes),
            mark(size_ok)
        );
        let _ = writeln!(
            text,
            "| cash's own start (`cash -c true` {cash:.1} ms less `cmd /c exit` {bare:.1} ms, \
             medians of {}) | {own:.1} ms | {:.0} ms | {} |\n",
            args.runs,
            budget.max_own_start_ms,
            mark(start_ok)
        );
        append(&summary, &text)?;
    }

    if !(size_ok && start_ok) {
        bail!(
            "over the budget in {}: raise it there, in a commit of its own that says why, \
             or make the build smaller or faster",
            args.budget.display()
        );
    }
    Ok(())
}

/// The median, in milliseconds, of `runs` runs of the command `make` builds, after one
/// that is not counted (the first start reads the program from disk).
fn median_ms(runs: usize, verbose: bool, make: impl Fn() -> Command) -> Result<f64> {
    let mut times = Vec::with_capacity(runs);
    for run in 0..=runs {
        let mut command = make();
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let start = Instant::now();
        let program = Path::new(command.get_program()).display().to_string();
        let status = command
            .status()
            .with_context(|| format!("starting {program}"))?;
        let took = start.elapsed().as_secs_f64() * 1000.0;
        if !status.success() {
            bail!("{program} failed: {status}");
        }
        if verbose {
            eprintln!("{program} run {run}: {took:.1} ms");
        }
        if run > 0 {
            times.push(took);
        }
    }
    times.sort_by(f64::total_cmp);
    Ok(times.get(times.len() / 2).copied().unwrap_or_default())
}

/// `bytes` in megabytes, to one decimal.
#[expect(clippy::cast_precision_loss, reason = "a size to one decimal place")]
fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

/// Adds `text` to the end of the file at `path`.
fn append(path: &Path, text: &str) -> Result<()> {
    use std::io::Write as _;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    file.write_all(text.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_budget_reads_numbers_with_underscores_and_skips_comments() {
        let budget = Budget::parse(
            "# size\nmax_bytes = 50_935_808  # 1.11.0 + 2 MiB\n\nmax_own_start_ms = 60\n",
        )
        .unwrap();
        assert_eq!(budget.max_bytes, 50_935_808);
        assert!((budget.max_own_start_ms - 60.0).abs() < f64::EPSILON);
        assert!(
            Budget::parse("max_bytes = 1\n").is_err(),
            "a limit is missing"
        );
        assert!(Budget::parse("max_bytes = 1\nmax_own_start_ms = 2\nspeed = 3\n").is_err());
        assert!(Budget::parse("max_bytes = big\nmax_own_start_ms = 2\n").is_err());
    }

    #[test]
    fn the_repositorys_budget_reads() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../perf-budget.toml");
        assert!(Budget::read(&path).is_ok());
    }
}
