//! `help`: the builtins by kind, a page for each, the topics, and a search over them all.
//!
//! What it says comes from the catalogue in `helpdocs`; the options of a builtin come
//! from the builtin itself, so they are never copied. `cash help ...`, outside the
//! shell, runs this builtin (`crates/cash/src/main.rs`).

use cash_core::{ExecutionResult, builtins};
use clap::Parser;
use itertools::Itertools;
use std::fmt::Write as _;
use std::io::Write;

use crate::helpdocs::{self, render, render::Style};

/// Display command help.
#[derive(Parser)]
pub(crate) struct HelpCommand {
    /// Display a short description for the commands.
    #[arg(short = 'd')]
    short_description: bool,

    /// Display a man-style page of documentation for the commands.
    #[arg(short = 'm')]
    man_page_style: bool,

    /// Display a short usage summary for the commands.
    #[arg(short = 's')]
    short_usage: bool,

    /// Patterns of builtins, or names of topics, to display help for; `topics` lists
    /// the topics, and `search WORD` searches every page.
    topic_patterns: Vec<String>,
}

/// The widest `help` wraps prose to, however wide the terminal: past this, lines are
/// hard to read.
const MAX_WIDTH: usize = 100;

/// Where the decisions a page cites are written down.
const SPEC_URL: &str = "https://github.com/tomcoolpxl/cash/blob/main/spec.md";

impl builtins::Command for HelpCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        let style = style(&context);
        match self.topic_patterns.split_first() {
            None => {
                let text = general_help(&context, style);
                write_out(&context, &text)?;
                Ok(ExecutionResult::success())
            }
            Some((first, [])) if first == "topics" => {
                write_out(&context, &topic_list(style))?;
                Ok(ExecutionResult::success())
            }
            Some((first, words)) if first == "search" => search(&context, words, style),
            Some(_) => self.show_patterns(&context, style),
        }
    }
}

impl HelpCommand {
    /// Shows each requested builtin or topic; fails only when none of them matched, as
    /// Bash does.
    fn show_patterns<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
        style: Style,
    ) -> Result<ExecutionResult, cash_core::Error> {
        let mut any_matched = false;
        let mut shown = 0;
        for pattern in &self.topic_patterns {
            let mut texts = Vec::new();
            let matcher = cash_core::patterns::Pattern::from(pattern.as_str())
                .set_extended_globbing(context.shell.options().extended_globbing)
                .set_case_insensitive(context.shell.options().case_insensitive_pathname_expansion);
            for (name, registration) in builtins_by_name(context) {
                if matcher.exactly_matches(name.as_str())? {
                    texts.push(self.builtin_text(name, registration, style)?);
                }
            }
            if texts.is_empty()
                && let Some(topic) = helpdocs::catalogue().topic(pattern)
            {
                texts.push(self.topic_text(topic, style));
            }
            if texts.is_empty() {
                no_match(context, pattern)?;
                continue;
            }
            any_matched = true;
            for text in texts {
                // Whole pages are set apart by a blank line; one-line answers are not.
                let page = !(self.short_description || self.short_usage);
                if page && shown > 0 {
                    write_out(context, "\n")?;
                }
                write_out(context, &text)?;
                shown += 1;
            }
        }
        Ok(if any_matched {
            ExecutionResult::success()
        } else {
            ExecutionResult::general_error()
        })
    }

    /// What `help` says about the builtin `name`, in the form the options ask for.
    fn builtin_text<SE: cash_core::ShellExtensions>(
        &self,
        name: &str,
        registration: &builtins::Registration<SE>,
        style: Style,
    ) -> Result<String, cash_core::Error> {
        if self.short_description {
            return Ok(format!("{name} - {}\n", summary_of(name, registration)?));
        }
        if self.short_usage {
            return short_usage(name, registration);
        }
        builtin_page(name, registration, style)
    }

    fn topic_text(&self, topic: &helpdocs::Topic, style: Style) -> String {
        if self.short_description || self.short_usage {
            format!("{} - {}\n", topic.name, render::plain(topic.summary))
        } else {
            topic_page(topic, style)
        }
    }
}

/// How this run of `help` draws: wrapped to the terminal, coloured when it writes to
/// one and `NO_COLOR` is not set.
fn style(context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>) -> Style {
    let terminal = context
        .try_fd(cash_core::openfiles::OpenFiles::STDOUT_FD)
        .is_some_and(|fd| fd.is_terminal());
    let no_colour = context
        .shell
        .env_str("NO_COLOR")
        .is_some_and(|value| !value.is_empty());
    let columns = if terminal {
        crossterm::terminal::size()
            .ok()
            .map(|(columns, _)| usize::from(columns))
    } else {
        None
    };
    let columns = columns
        .or_else(|| {
            context
                .shell
                .env_str("COLUMNS")
                .and_then(|value| value.trim().parse().ok())
        })
        .unwrap_or(80);
    Style {
        width: columns.saturating_sub(1).clamp(40, MAX_WIDTH),
        colour: terminal && !no_colour,
    }
}

fn write_out(
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
    text: &str,
) -> Result<(), cash_core::Error> {
    let mut stdout = context.stdout();
    stdout.write_all(text.as_bytes())?;
    stdout.flush()?;
    Ok(())
}

fn builtins_by_name<'a, SE: cash_core::ShellExtensions>(
    context: &'a cash_core::ExecutionContext<'_, SE>,
) -> Vec<(&'a String, &'a builtins::Registration<SE>)> {
    context
        .shell
        .builtins()
        .iter()
        .sorted_by_key(|(name, _)| *name)
        .collect()
}

/// The builtin's one-line summary: the catalogue's, or failing that, its own.
fn summary_of<SE: cash_core::ShellExtensions>(
    name: &str,
    registration: &builtins::Registration<SE>,
) -> Result<String, cash_core::Error> {
    if let Some(entry) = helpdocs::catalogue().entry(name) {
        return Ok(render::plain(entry.summary));
    }
    let own = (registration.content_func)(
        name,
        builtins::ContentType::ShortDescription,
        &builtins::ContentOptions::default(),
    )?;
    let own = own.trim();
    Ok(own
        .strip_prefix(name)
        .and_then(|rest| rest.trim_start().strip_prefix('-'))
        .unwrap_or(own)
        .trim()
        .to_owned())
}

/// `help -s NAME`: the builtin's usage line, `name: name [OPTION]...`.
fn short_usage<SE: cash_core::ShellExtensions>(
    name: &str,
    registration: &builtins::Registration<SE>,
) -> Result<String, cash_core::Error> {
    let options = builtins::ContentOptions::default();
    let own = (registration.content_func)(name, builtins::ContentType::ShortUsage, &options)?;
    if !own.trim().is_empty() {
        return Ok(if own.ends_with('\n') {
            own
        } else {
            format!("{own}\n")
        });
    }
    // A builtin with no usage line of its own, a bundled tool's, has one in its help.
    let help = (registration.content_func)(name, builtins::ContentType::DetailedHelp, &options)?;
    // `Usage: cat [OPTION]...`, or `Usage` with the line on the next (`ping`).
    let mut lines = help.lines().map(str::trim);
    let usage = lines
        .by_ref()
        .find_map(|line| line.strip_prefix("Usage"))
        .map(|rest| rest.trim_start_matches(':').trim())
        .and_then(|rest| {
            if rest.is_empty() {
                lines.find(|line| !line.is_empty())
            } else {
                Some(rest)
            }
        })
        .map_or_else(|| name.to_owned(), str::to_owned);
    Ok(format!("{name}: {usage}\n"))
}

/// The path of the Windows program `name` hides inside cash, if System32 has one:
/// `find` is cash's builtin, and `find.exe` there searches inside files.
fn hidden_windows_program(name: &str) -> Option<std::path::PathBuf> {
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    [".exe", ".com"]
        .iter()
        .map(|ext| cash_win32::fs::system_program(&format!("{name}{ext}")))
        .find(|path| path.is_file())
}

/// The sentences that say a builtin hides a Windows program, and how to reach it.
fn hiding_note(name: &str, program: &std::path::Path) -> String {
    let file = program
        .file_name()
        .map(|file| file.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!(
        "Windows has a program of the same name, `{}`. Inside cash, `{name}` runs this \
         builtin; to run the Windows one, give its path: `\"$SYSTEMROOT/System32/{file}\"`.",
        program.display()
    )
}

/// `help NAME`: the builtin's page, man-style.
fn builtin_page<SE: cash_core::ShellExtensions>(
    name: &str,
    registration: &builtins::Registration<SE>,
    style: Style,
) -> Result<String, cash_core::Error> {
    let catalogue = helpdocs::catalogue();
    let indent = " ".repeat(render::INDENT);
    let mut out = render::heading("Name", style);
    let summary = summary_of(name, registration)?;
    out.push_str(&render::wrap(
        &render::inline(&format!("`{name}` - {summary}"), style.colour),
        style.width,
        &indent,
        render::INDENT,
    ));
    if let Some(entry) = catalogue.entry(name) {
        let _ = writeln!(out, "{indent}({})", entry.kind);
    }
    if let Some(program) = hidden_windows_program(name) {
        out.push('\n');
        out.push_str(&render::wrap(
            &render::inline(&hiding_note(name, &program), style.colour),
            style.width,
            &indent,
            render::INDENT,
        ));
    }
    if registration.disabled {
        out.push('\n');
        out.push_str(&render::wrap(
            &render::inline(
                "It is disabled (`enable -n`): the name runs a command found on `PATH` \
                 instead, until `enable` turns it back on.",
                style.colour,
            ),
            style.width,
            &indent,
            render::INDENT,
        ));
    }

    let page = catalogue.page(name);
    if let Some(page) = page {
        out.push('\n');
        out.push_str(&render::markdown(page.body, style));
    }

    let options = builtins::ContentOptions {
        colorized: style.colour,
    };
    let own = (registration.content_func)(name, builtins::ContentType::DetailedHelp, &options)?;
    if !own.trim().is_empty() {
        out.push('\n');
        out.push_str(&render::heading("Usage and options", style));
        out.push_str(&render::indented(&own, render::INDENT));
    }

    if let Some(page) = page {
        see_also_and_spec(&mut out, &page.see, &page.spec, style);
    }
    Ok(out)
}

/// `help NAME` for a topic.
fn topic_page(topic: &helpdocs::Topic, style: Style) -> String {
    let indent = " ".repeat(render::INDENT);
    let mut out = render::heading(topic.title, style);
    out.push_str(&render::wrap(
        &render::inline(topic.summary, style.colour),
        style.width,
        &indent,
        render::INDENT,
    ));
    out.push('\n');
    out.push_str(&render::markdown(topic.body, style));
    see_also_and_spec(&mut out, &topic.see, &topic.spec, style);
    out
}

fn see_also_and_spec(out: &mut String, see: &[&str], spec: &[&str], style: Style) {
    let indent = " ".repeat(render::INDENT);
    if !see.is_empty() {
        out.push('\n');
        out.push_str(&render::heading("See also", style));
        let list = see.iter().map(|name| format!("`help {name}`")).join(", ");
        out.push_str(&render::wrap(
            &render::inline(&list, style.colour),
            style.width,
            &indent,
            render::INDENT,
        ));
    }
    if !spec.is_empty() {
        out.push('\n');
        out.push_str(&render::heading("Why", style));
        let text = format!(
            "The decisions {} in cash's specification, {SPEC_URL}",
            spec.join(", ")
        );
        out.push_str(&render::wrap(&text, style.width, &indent, render::INDENT));
    }
}

/// `help` alone: every builtin, grouped by kind, each with its summary.
fn general_help<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    style: Style,
) -> String {
    let catalogue = helpdocs::catalogue();
    let builtins = builtins_by_name(context);
    let mut out = String::new();
    if let Some(display) = context.shell.product_display_str() {
        let _ = writeln!(out, "{display}\n");
    }
    out.push_str(&render::wrap(
        &render::inline(
            "The builtins, by kind. `help NAME` shows a builtin's page, `help topics` lists \
             the topics (paths, line endings, keys, ...), and `help search WORD` searches \
             them all.",
            style.colour,
        ),
        style.width,
        "",
        0,
    ));

    let name_width = builtins
        .iter()
        .map(|(name, _)| render::visible_width(name))
        .max()
        .unwrap_or_default();
    let mut hidden = Vec::new();
    let mut any_disabled = false;
    let mut line = |out: &mut String, name: &str, disabled: bool, summary: &str| {
        let hides = hidden_windows_program(name);
        let marker = if disabled {
            any_disabled = true;
            '*'
        } else if let Some(program) = hides {
            hidden.push(
                program
                    .file_name()
                    .map(|file| file.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            );
            '+'
        } else {
            ' '
        };
        let first = format!(" {marker}{name:<name_width$}  ");
        out.push_str(&render::wrap(
            &render::inline(summary, style.colour),
            style.width,
            &first,
            name_width + 4,
        ));
    };

    for kind in &catalogue.kinds {
        let members: Vec<_> = catalogue
            .entries
            .iter()
            .filter(|entry| entry.kind == *kind)
            .filter_map(|entry| {
                builtins
                    .iter()
                    .find(|(name, _)| name.as_str() == entry.name)
                    .map(|(_, registration)| (entry, registration.disabled))
            })
            .collect();
        if members.is_empty() {
            continue;
        }
        out.push('\n');
        out.push_str(&kind_heading(kind, style));
        for (entry, disabled) in members {
            line(&mut out, entry.name, disabled, entry.summary);
        }
    }

    // A builtin the catalogue does not know yet still gets listed, with its own words.
    let unknown: Vec<_> = builtins
        .iter()
        .filter(|(name, _)| catalogue.entry(name).is_none())
        .collect();
    if !unknown.is_empty() {
        out.push('\n');
        out.push_str(&kind_heading("Other", style));
        for (name, registration) in unknown {
            let summary = summary_of(name, registration).unwrap_or_default();
            line(&mut out, name, registration.disabled, &summary);
        }
    }

    out.push_str(&markers_note(&hidden, any_disabled, style));
    out
}

/// What the list's `+` and `*` mean, for the marks it used.
fn markers_note(hidden: &[String], any_disabled: bool, style: Style) -> String {
    let mut out = String::new();
    if !hidden.is_empty() {
        out.push('\n');
        let text = format!(
            "+ Windows has a program of the same name in System32 ({}). Inside cash the \
             builtin wins; to run the Windows one, give its path, as \
             `\"$SYSTEMROOT/System32/{}\"`.",
            hidden.join(", "),
            hidden.first().map_or("find.exe", String::as_str)
        );
        out.push_str(&render::wrap(
            &render::inline(&text, style.colour),
            style.width,
            "",
            2,
        ));
    }
    if any_disabled {
        out.push_str(&render::wrap(
            &render::inline("* Disabled with `enable -n`.", style.colour),
            style.width,
            "",
            2,
        ));
    }
    out
}

fn kind_heading(kind: &str, style: Style) -> String {
    if style.colour {
        format!("\x1b[1m{kind}\x1b[0m\n")
    } else {
        format!("{kind}\n")
    }
}

/// `help topics`.
fn topic_list(style: Style) -> String {
    let catalogue = helpdocs::catalogue();
    let width = catalogue
        .topics
        .iter()
        .map(|topic| topic.name.len())
        .max()
        .unwrap_or_default();
    let mut out = render::wrap(
        &render::inline(
            "Topics: `help NAME` shows one. `help` alone lists the builtins.",
            style.colour,
        ),
        style.width,
        "",
        0,
    );
    out.push('\n');
    for topic in &catalogue.topics {
        let first = format!("  {:<width$}  ", topic.name);
        out.push_str(&render::wrap(
            &render::inline(topic.summary, style.colour),
            style.width,
            &first,
            width + 4,
        ));
    }
    out
}

/// How many matching lines of one page a search shows.
const LINES_PER_HIT: usize = 3;

/// `help search WORD`.
fn search(
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
    words: &[String],
    style: Style,
) -> Result<ExecutionResult, cash_core::Error> {
    let word = words.join(" ");
    if word.trim().is_empty() {
        writeln!(
            context.error_stream(),
            "{}: search: say what to search for: `help search WORD'",
            context.command_name
        )?;
        return Ok(ExecutionResult::general_error());
    }
    let hits = helpdocs::search(&word);
    if hits.is_empty() {
        writeln!(
            context.error_stream(),
            "{}: nothing mentions `{word}'. `help' lists the builtins, `help topics' the \
             topics.",
            context.command_name
        )?;
        return Ok(ExecutionResult::general_error());
    }
    let mut out = String::new();
    for hit in hits {
        let what = if hit.topic { "topic" } else { "builtin" };
        out.push_str(&render::wrap(
            &render::inline(
                &format!("`{}` ({what}) - {}", hit.name, hit.summary),
                style.colour,
            ),
            style.width,
            "",
            4,
        ));
        for line in hit.lines.iter().take(LINES_PER_HIT) {
            out.push_str(&render::wrap(line, style.width, "    ", 6));
        }
        if hit.lines.len() > LINES_PER_HIT {
            let more = hit.lines.len() - LINES_PER_HIT;
            let text = format!("    ... and {more} more: `help {}`", hit.name);
            let _ = writeln!(out, "{}", render::inline(&text, style.colour));
        }
    }
    write_out(context, &out)?;
    Ok(ExecutionResult::success())
}

/// Says that `pattern` matched nothing, and what was meant, if a name is close.
fn no_match(
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
    pattern: &str,
) -> Result<(), cash_core::Error> {
    writeln!(
        context.error_stream(),
        "{}: no help topics match `{pattern}'.",
        context.command_name
    )?;
    // The advice follows on lines of its own, without the script and line the error
    // above names.
    let mut stderr = context.stderr();
    let builtins: Vec<&str> = context
        .shell
        .builtins()
        .keys()
        .map(String::as_str)
        .collect();
    let close = helpdocs::suggestions(pattern, &builtins);
    if !close.is_empty() {
        writeln!(stderr, "Did you mean: {}?", close.join(", "))?;
    }
    writeln!(
        stderr,
        "Try `help search {pattern}', `help topics', or `help' for every builtin."
    )?;
    Ok(())
}
