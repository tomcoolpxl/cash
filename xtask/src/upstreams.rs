//! `cargo xtask upstreams`: the upstream code cash took in as crates of its own, against
//! what upstream has done since. Dependabot sees only crates.io dependencies, so nothing
//! else would say that reedline fixed a panic, or that `RustSec` lists a crate cash copied.
//!
//! The list is `upstreams.toml`. For each entry this shows upstream's newest release on
//! crates.io, or the commits made upstream since the revision it was taken at, and the
//! `RustSec` advisories that name its crate and the version it was taken from. The weekly
//! Upstreams workflow runs it and opens an issue when it exits with 1.
//!
//! It asks crates.io's sparse index through `curl`, and GitHub (the commits, and the
//! `RustSec` database) through `gh api`, which takes `GH_TOKEN` in CI.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Parser;
use serde::Deserialize;

/// Arguments for `cargo xtask upstreams`.
#[derive(Parser)]
pub struct UpstreamsArgs {
    /// The list of upstreams.
    #[clap(long, default_value = "upstreams.toml")]
    list: PathBuf,

    /// A Markdown file to write the report to, as well as printing it.
    #[clap(long)]
    report: Option<PathBuf>,
}

#[derive(Deserialize)]
struct List {
    upstream: Vec<Upstream>,
}

#[derive(Deserialize)]
struct Upstream {
    #[serde(rename = "crate")]
    folder: String,
    name: Option<String>,
    version: Option<String>,
    repository: Option<String>,
    revision: Option<String>,
    path: Option<String>,
    /// How far upstream was looked through with nothing to port: a release, or a commit
    /// for one taken from git.
    reviewed: Option<String>,
}

/// What one advisory says, from the TOML at the head of its Markdown file.
#[derive(Deserialize)]
struct Advisory {
    advisory: AdvisoryHead,
    #[serde(default)]
    versions: AdvisoryVersions,
}

#[derive(Deserialize)]
struct AdvisoryHead {
    id: String,
    #[serde(default)]
    title: String,
    informational: Option<String>,
    withdrawn: Option<String>,
}

#[derive(Deserialize, Default)]
struct AdvisoryVersions {
    #[serde(default)]
    patched: Vec<String>,
    #[serde(default)]
    unaffected: Vec<String>,
}

/// Runs `cargo xtask upstreams`: prints the report, and fails when an upstream moved on
/// or an advisory names what cash took.
pub fn run(args: &UpstreamsArgs, verbose: bool) -> Result<()> {
    let text = std::fs::read_to_string(&args.list)
        .with_context(|| format!("reading {}", args.list.display()))?;
    let list: List =
        toml::from_str(&text).with_context(|| format!("parsing {}", args.list.display()))?;

    let mut report = String::from("| Crate | Taken from | Upstream now |\n| --- | --- | --- |\n");
    let mut notes = String::new();
    let mut wanted = 0;
    for upstream in &list.upstream {
        let taken = taken_from(upstream);
        let (now, moved) = upstream_now(upstream, verbose)?;
        wanted += usize::from(moved);
        let mark = if moved { "**moved on**: " } else { "" };
        let _ = writeln!(report, "| `{}` | {taken} | {mark}{now} |", upstream.folder);

        if let (Some(name), Some(version)) = (&upstream.name, &upstream.version) {
            for advisory in advisories(name, verbose)? {
                if advisory.advisory.withdrawn.is_some() || !affects(&advisory, version)? {
                    continue;
                }
                let kind = advisory
                    .advisory
                    .informational
                    .as_deref()
                    .map_or_else(|| "vulnerability".to_owned(), |kind| kind.to_owned());
                wanted += 1;
                let _ = writeln!(
                    notes,
                    "- `{}` ({name} {version}): [{}](https://rustsec.org/advisories/{}.html), {kind}: {}",
                    upstream.folder,
                    advisory.advisory.id,
                    advisory.advisory.id,
                    advisory.advisory.title
                );
            }
        }
    }
    if !notes.is_empty() {
        report.push_str("\nRustSec advisories that name what cash took:\n\n");
        report.push_str(&notes);
    }

    println!("{report}");
    if let Some(path) = &args.report {
        std::fs::write(path, &report).with_context(|| format!("writing {}", path.display()))?;
    }
    if wanted > 0 {
        bail!("{wanted} upstream change(s) or advisories want looking at (upstreams.toml)");
    }
    Ok(())
}

fn taken_from(upstream: &Upstream) -> String {
    let mut taken = match (&upstream.name, &upstream.version) {
        (Some(name), Some(version)) => format!("{name} {version}"),
        _ => String::new(),
    };
    if let (Some(repository), Some(revision)) = (&upstream.repository, &upstream.revision) {
        let short: String = revision.chars().take(8).collect();
        let repo = repository.trim_start_matches("https://github.com/");
        let at = match &upstream.path {
            Some(path) => format!("{repo} `{short}` (`{path}/`)"),
            None => format!("{repo} `{short}`"),
        };
        taken = if taken.is_empty() {
            at
        } else {
            format!("{taken}, {at}")
        };
    }
    if let Some(reviewed) = &upstream.reviewed {
        let short: String = reviewed.chars().take(8).collect();
        let _ = write!(taken, "; reviewed to {short}");
    }
    taken
}

/// What upstream has now, and whether it moved on from what was taken: commits since the
/// revision for one taken from git, else a newer release.
fn upstream_now(upstream: &Upstream, verbose: bool) -> Result<(String, bool)> {
    if let (Some(repository), Some(revision)) = (&upstream.repository, &upstream.revision) {
        let repo = repository.trim_start_matches("https://github.com/");
        let from = upstream.reviewed.as_deref().unwrap_or(revision);
        let since = commits_since(repo, from, upstream.path.as_deref(), verbose)?;
        let release = match &upstream.name {
            Some(name) => newest_release(name, verbose)?
                .map(|release| format!(", newest release {release}"))
                .unwrap_or_default(),
            None => String::new(),
        };
        return Ok((format!("{since} commit(s) since{release}"), since > 0));
    }
    let (Some(name), Some(version)) = (&upstream.name, &upstream.version) else {
        bail!(
            "{}: a name and version, or a repository and revision",
            upstream.folder
        );
    };
    let seen = upstream.reviewed.as_deref().unwrap_or(version);
    let seen = semver::Version::parse(seen)
        .with_context(|| format!("{}: version {seen}", upstream.folder))?;
    match newest_release(name, verbose)? {
        Some(newest) => Ok((newest.to_string(), newest > seen)),
        None => Ok(("no release on crates.io".to_owned(), false)),
    }
}

/// The newest release of `name` on crates.io that is neither yanked nor a pre-release.
fn newest_release(name: &str, verbose: bool) -> Result<Option<semver::Version>> {
    let lower = name.to_ascii_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    let part = |range: std::ops::Range<usize>| chars[range].iter().collect::<String>();
    let prefix = match chars.len() {
        1 | 2 => format!("{}", chars.len()),
        3 => format!("3/{}", part(0..1)),
        _ => format!("{}/{}", part(0..2), part(2..4)),
    };
    let url = format!("https://index.crates.io/{prefix}/{lower}");
    let body = output(
        Command::new("curl").args(["-sSfL", "-A", "cash xtask upstreams", &url]),
        verbose,
    )
    .with_context(|| format!("asking crates.io for {name}"))?;
    let mut newest: Option<semver::Version> = None;
    for line in body.lines().filter(|line| !line.is_empty()) {
        let entry: serde_json::Value = serde_json::from_str(line)?;
        if entry["yanked"].as_bool() == Some(true) {
            continue;
        }
        let Some(Ok(version)) = entry["vers"].as_str().map(semver::Version::parse) else {
            continue;
        };
        if version.pre.is_empty() && newest.as_ref().is_none_or(|n| version > *n) {
            newest = Some(version);
        }
    }
    Ok(newest)
}

/// The commits made in `repo` (`owner/name`) after `revision`, those touching `path` when
/// one is given.
fn commits_since(repo: &str, revision: &str, path: Option<&str>, verbose: bool) -> Result<usize> {
    let date = output(
        Command::new("gh").args([
            "api",
            &format!("repos/{repo}/commits/{revision}"),
            "--jq",
            ".commit.committer.date",
        ]),
        verbose,
    )
    .with_context(|| format!("asking GitHub for {repo}'s {revision}"))?;
    let mut query = format!("repos/{repo}/commits?per_page=100&since={}", date.trim());
    if let Some(path) = path {
        let _ = write!(query, "&path={path}");
    }
    let shas = output(
        Command::new("gh").args(["api", "--paginate", &query, "--jq", ".[].sha"]),
        verbose,
    )
    .with_context(|| format!("asking GitHub for {repo}'s commits"))?;
    Ok(shas
        .lines()
        .filter(|sha| !sha.is_empty() && *sha != revision)
        .count())
}

/// The advisories in the `RustSec` database for `name`; none when it has no folder for it.
fn advisories(name: &str, verbose: bool) -> Result<Vec<Advisory>> {
    let listing = Command::new("gh")
        .args([
            "api",
            &format!("repos/rustsec/advisory-db/contents/crates/{name}"),
            "--jq",
            ".[].name",
        ])
        .output()
        .context("running gh")?;
    if !listing.status.success() {
        let stderr = String::from_utf8_lossy(&listing.stderr);
        if stderr.contains("Not Found") || stderr.contains("404") {
            return Ok(Vec::new());
        }
        bail!("asking RustSec for {name}: {stderr}");
    }
    let mut found = Vec::new();
    for file in String::from_utf8_lossy(&listing.stdout).lines() {
        if !std::path::Path::new(file)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        {
            continue;
        }
        let text = output(
            Command::new("gh").args([
                "api",
                "-H",
                "Accept: application/vnd.github.raw",
                &format!("repos/rustsec/advisory-db/contents/crates/{name}/{file}"),
            ]),
            verbose,
        )?;
        let front = text
            .split("```toml")
            .nth(1)
            .and_then(|rest| rest.split("```").next())
            .with_context(|| format!("{file}: no TOML at its head"))?;
        found.push(toml::from_str(front).with_context(|| format!("parsing {file}"))?);
    }
    Ok(found)
}

/// Whether `advisory` names `version`: neither patched in it nor unaffected.
fn affects(advisory: &Advisory, version: &str) -> Result<bool> {
    let version = semver::Version::parse(version)?;
    for requirement in advisory
        .versions
        .patched
        .iter()
        .chain(&advisory.versions.unaffected)
    {
        if semver::VersionReq::parse(requirement)?.matches(&version) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The standard output of `command`, which must succeed.
fn output(command: &mut Command, verbose: bool) -> Result<String> {
    if verbose {
        eprintln!("$ {command:?}");
    }
    let out = command
        .output()
        .with_context(|| format!("running {command:?}"))?;
    if !out.status.success() {
        bail!(
            "{command:?} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn advisory(patched: &[&str], unaffected: &[&str]) -> Advisory {
        Advisory {
            advisory: AdvisoryHead {
                id: "RUSTSEC-0000-0000".to_owned(),
                title: String::new(),
                informational: None,
                withdrawn: None,
            },
            versions: AdvisoryVersions {
                patched: patched.iter().map(|s| (*s).to_owned()).collect(),
                unaffected: unaffected.iter().map(|s| (*s).to_owned()).collect(),
            },
        }
    }

    #[test]
    fn an_advisory_names_a_version_neither_patched_nor_unaffected() {
        let bomb = advisory(&[">= 0.9.1"], &["< 0.8.0"]);
        assert!(affects(&bomb, "0.9.0").unwrap());
        assert!(!affects(&bomb, "0.9.1").unwrap());
        assert!(!affects(&bomb, "0.7.3").unwrap());
        assert!(affects(&advisory(&[], &[]), "1.0.0").unwrap());
    }

    #[test]
    fn the_list_in_the_repository_parses() {
        let text = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../upstreams.toml"),
        )
        .unwrap();
        let list: List = toml::from_str(&text).unwrap();
        assert!(list.upstream.len() >= 8);
        for upstream in &list.upstream {
            let named = upstream.name.is_some() && upstream.version.is_some();
            let from_git = upstream.repository.is_some() && upstream.revision.is_some();
            assert!(named || from_git, "{}", upstream.folder);
            assert!(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("..")
                    .join(&upstream.folder)
                    .exists(),
                "{} is not a folder of the repository",
                upstream.folder
            );
        }
    }
}
