//! `help`: the builtins by kind, a page for each, the topics, and a search over them all.
//!
//! What it says comes from the manual's pages (`cash-man`); a page not yet written in
//! full ends with the builtin's own `--help`, so its options are never copied. `cash
//! help ...`, outside the shell, runs this builtin (`crates/cash/src/main.rs`).

use cash_core::{ExecutionResult, builtins};
use clap::Parser;
use itertools::Itertools;
use std::fmt::Write as _;
use std::io::Write;

use cash_man::page::{Page, Subject};

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

    /// Colour the text: always, never, or auto (only for a terminal, the default).
    #[arg(
        long = "color",
        alias = "colour",
        value_name = "WHEN",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "always"
    )]
    colour: Option<When>,

    /// Speak as `cash help`, the subcommand run from outside the shell: its errors name
    /// `cash help` rather than the `-c` script and line that runs it.
    #[arg(long = helpdocs::CASH_SUBCOMMAND_OPTION, hide = true)]
    cash_subcommand: bool,

    /// An option `cash help` was given and `help` has not: reported by `help` itself,
    /// since the shell's report would name the `-c` script and its line.
    #[arg(skip)]
    invalid_option: Option<String>,

    /// Patterns of builtins, or names of topics, to display help for; `topics` lists
    /// the topics, and `search WORD` searches every page.
    topic_patterns: Vec<String>,
}

/// When `help` colours its text (`--color=WHEN`, as `ls` and `grep` take it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
enum When {
    Always,
    Never,
    Auto,
}

/// The widest `help` wraps prose to, however wide the terminal: past this, lines are
/// hard to read.
const MAX_WIDTH: usize = 100;

impl builtins::Command for HelpCommand {
    type Error = cash_core::Error;

    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        let args: Vec<String> = args.into_iter().collect();
        let marker = format!("--{}", helpdocs::CASH_SUBCOMMAND_OPTION);
        let cash_subcommand = args.contains(&marker);
        match builtins::parse_args::<Self, _>(&args) {
            Err(error)
                if cash_subcommand && error.kind() == clap::error::ErrorKind::UnknownArgument =>
            {
                let option = match error.get(clap::error::ContextKind::InvalidArg) {
                    Some(clap::error::ContextValue::String(argument)) => argument
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_owned(),
                    _ => String::new(),
                };
                Ok(Self {
                    short_description: false,
                    man_page_style: false,
                    short_usage: false,
                    colour: None,
                    cash_subcommand,
                    invalid_option: Some(option),
                    topic_patterns: Vec::new(),
                })
            }
            parsed => parsed,
        }
    }

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<cash_core::ExecutionResult, Self::Error> {
        if let Some(option) = &self.invalid_option {
            self.complain(&context, &format!("{option}: invalid option"))?;
            self.complain(&context, "usage: cash help [-dms] [pattern ...]")?;
            return Ok(cash_core::ExecutionExitCode::InvalidUsage.into());
        }
        let style = style(&context, self.colour);
        // Gathered whole, then shown through the pager: paged in a terminal when taller
        // than it, colours and all.
        let mut out = String::new();
        let result = match self.topic_patterns.split_first() {
            None => {
                out = general_help(&context, style);
                ExecutionResult::success()
            }
            Some((first, [])) if first == "topics" => {
                out = topic_list(style);
                ExecutionResult::success()
            }
            Some((first, words)) if first == "search" => {
                self.search(&context, words, style, &mut out)?
            }
            Some(_) => self.show_patterns(&context, style, &mut out)?,
        };
        crate::pager::show(&context, &out)?;
        Ok(result)
    }
}

impl HelpCommand {
    /// How the user runs `help`, for the advice in a message: `help`, or `cash help`
    /// outside the shell.
    const fn invocation(&self) -> &'static str {
        if self.cash_subcommand {
            "cash help"
        } else {
            "help"
        }
    }

    /// Reports an error: as Bash reports a builtin's inside the shell (`script: line 3:
    /// help: ...`), and as `cash help: ...` outside it, where the `-c` script and its
    /// line are cash's own doing and mean nothing to the user.
    fn complain(
        &self,
        context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
        message: &str,
    ) -> Result<(), cash_core::Error> {
        if self.cash_subcommand {
            writeln!(context.stderr(), "cash help: {message}")?;
        } else {
            writeln!(
                context.error_stream(),
                "{}: {message}",
                context.command_name
            )?;
        }
        Ok(())
    }

    /// Shows each requested builtin or topic; fails only when none of them matched, as
    /// Bash does.
    fn show_patterns<SE: cash_core::ShellExtensions>(
        &self,
        context: &cash_core::ExecutionContext<'_, SE>,
        style: Style,
        out: &mut String,
    ) -> Result<ExecutionResult, cash_core::Error> {
        let mut any_matched = false;
        let mut shown = 0;
        for pattern in &self.topic_patterns {
            let mut texts = Vec::new();
            let matcher = cash_core::patterns::Pattern::from(pattern.as_str())
                .set_extended_globbing(context.shell.options().extended_globbing)
                .set_case_insensitive(context.shell.options().case_insensitive_pathname_expansion);
            // A word with no wildcard in it names every builtin it begins, as in Bash:
            // `help -s rea` is read, readarray and readonly.
            let prefix = !pattern.contains(['*', '?', '[']);
            let matches =
                |name: &str| -> Result<bool, cash_core::Error> {
                    Ok(matcher.exactly_matches(name)?
                        || (prefix && name.starts_with(pattern.as_str())))
                };
            for (name, registration) in builtins_by_name(context) {
                if matches(name)? {
                    texts.push(self.builtin_text(name, registration, style)?);
                }
            }
            // The reserved words and constructs have pages of their own, as Bash has
            // help for them: `help case`, `help [[`.
            for page in keyword_pages() {
                for name in &page.names {
                    if matches(name)? {
                        texts.push(self.keyword_text(name, page, style));
                    }
                }
            }
            // `help variables` is Bash's name for the topic of the shell's variables.
            let topic_name = if pattern == "variables" {
                "vars"
            } else {
                pattern.as_str()
            };
            if texts.is_empty()
                && let Some(topic) = cash_man::manual().topic(topic_name)
            {
                texts.push(self.topic_text(topic, style));
            }
            if texts.is_empty() {
                self.no_match(context, pattern)?;
                continue;
            }
            any_matched = true;
            for text in texts {
                // Whole pages are set apart by a blank line; one-line answers are not.
                let page = !(self.short_description || self.short_usage);
                if page && shown > 0 {
                    out.push('\n');
                }
                out.push_str(&text);
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
        builtin_page(name, registration, style, self.man_page_style)
    }

    /// What `help` says about the reserved word or construct `name`, from its page.
    fn keyword_text(&self, name: &str, page: &Page, style: Style) -> String {
        if self.short_description {
            format!("{name} - {}\n", render::plain(page.summary_of(name)))
        } else if self.short_usage {
            format!("{name}: {}\n", keyword_synopsis(name, page))
        } else {
            drawn(page, style)
        }
    }

    fn topic_text(&self, topic: &Page, style: Style) -> String {
        if self.short_description || self.short_usage {
            format!("{} - {}\n", topic.file, render::plain(&topic.summary))
        } else {
            drawn(topic, style)
        }
    }
}

/// How this run of `help` draws: wrapped to the terminal, and coloured as `--color`
/// says; without it, when `NO_COLOR` is not set and it writes to a terminal or
/// `CLICOLOR_FORCE` is set, which keeps the colours through a pipe into `less`.
fn style(
    context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
    when: Option<When>,
) -> Style {
    let terminal = context
        .try_fd(cash_core::openfiles::OpenFiles::STDOUT_FD)
        .is_some_and(|fd| fd.is_terminal());
    let set = |name: &str| {
        context
            .shell
            .env_str(name)
            .is_some_and(|value| !value.is_empty() && value != "0")
    };
    let colour = match when {
        Some(When::Always) => true,
        Some(When::Never) => false,
        Some(When::Auto) | None => {
            context
                .shell
                .env_str("NO_COLOR")
                .is_none_or(|value| value.is_empty())
                && (terminal || set("CLICOLOR_FORCE"))
        }
    };
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
        colour,
    }
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

/// The names `help` lists under `kind`, each with whether it is disabled and its summary.
/// The reserved words are listed from their pages: no builtin is one.
fn kind_members<'a, SE: cash_core::ShellExtensions>(
    kind: &str,
    builtins: &[(&'a String, &'a builtins::Registration<SE>)],
) -> Vec<(&'a str, bool, &'static str)> {
    let manual = cash_man::manual();
    if kind == cash_man::KEYWORDS {
        keyword_pages()
            .flat_map(|page| {
                page.names
                    .iter()
                    .map(move |name| (name.as_str(), false, page.summary_of(name)))
            })
            .sorted_by_key(|(name, _, _)| *name)
            .collect()
    } else {
        builtins
            .iter()
            .filter_map(|(name, registration)| {
                let page = manual.page(name).filter(|page| page.kind == kind)?;
                Some((name.as_str(), registration.disabled, page.summary_of(name)))
            })
            .collect()
    }
}

/// The pages of the reserved words and constructs.
fn keyword_pages() -> impl Iterator<Item = &'static Page> {
    cash_man::manual()
        .pages
        .iter()
        .filter(|page| page.kind == cash_man::KEYWORDS)
}

/// The usage line of the reserved word `name`: the first paragraph of its page's syntax,
/// on one line, or of that paragraph the lines of `name` when each line is one of the
/// page's words (`while` and `until`).
fn keyword_synopsis(name: &str, page: &Page) -> String {
    let lines: Vec<&str> = page
        .section(cash_man::page::Heading::Syntax)
        .map(|section| {
            section
                .body
                .lines()
                .skip_while(|line| line.trim().is_empty())
                .take_while(|line| !line.trim().is_empty())
                .map(str::trim)
                .collect()
        })
        .unwrap_or_default();
    let starts_with = |line: &str, word: &str| {
        line.strip_prefix(word)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
    };
    let each_its_own = lines.len() > 1
        && lines
            .iter()
            .all(|line| page.names.iter().any(|word| starts_with(line, word)));
    let chosen: Vec<&str> = if each_its_own {
        lines
            .into_iter()
            .filter(|line| starts_with(line, name))
            .collect()
    } else {
        lines
    };
    chosen.join(" ")
}

/// The builtin's one-line summary: its page's, or failing that, its own.
fn summary_of<SE: cash_core::ShellExtensions>(
    name: &str,
    registration: &builtins::Registration<SE>,
) -> Result<String, cash_core::Error> {
    if let Some(page) = cash_man::manual().page(name) {
        return Ok(render::plain(page.summary_of(name)));
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

/// `help NAME`: the builtin's page in short, as the manual draws it, with what only
/// `help` knows: that the name hides a Windows program, or that it is disabled. The
/// short form is the syntax (the builtin's usage line when its page has none) and the
/// examples, and where the whole page is; `whole` (`help -m`) is all of it, a draft
/// ending with the builtin's own `--help` before its See also.
fn builtin_page<SE: cash_core::ShellExtensions>(
    name: &str,
    registration: &builtins::Registration<SE>,
    style: Style,
    whole: bool,
) -> Result<String, cash_core::Error> {
    let manual = cash_man::manual();
    let page = manual.page(name);
    let drawn = page.map(|page| cash_man::render::draw(page, manual, style.width));
    let mut out = page_head(name, registration, page, style)?;
    if !whole {
        out.push_str(&short_form(name, registration, drawn.as_ref(), style)?);
        return Ok(out);
    }
    let lines = drawn
        .as_ref()
        .map_or(&[][..], |drawn| drawn.lines.as_slice());
    // The page's own top, the name and summary up to the first blank line, is left
    // out: the head says it for the name asked for.
    let top = lines
        .iter()
        .position(|line| line.spans.is_empty())
        .unwrap_or(lines.len());
    let see_also = drawn
        .as_ref()
        .and_then(|drawn| {
            drawn
                .sections
                .iter()
                .find(|section| section.title == "SEE ALSO")
        })
        .map_or(lines.len(), |section| section.line.saturating_sub(1));
    out.push_str(&lines_text(
        lines.get(top..see_also).unwrap_or_default(),
        style,
    ));
    if page.is_none_or(|page| page.draft) {
        let options = builtins::ContentOptions {
            colorized: style.colour,
        };
        let own = (registration.content_func)(name, builtins::ContentType::DetailedHelp, &options)?;
        if !own.trim().is_empty() {
            out.push('\n');
            out.push_str(&render::heading("Usage and options", style));
            out.push_str(&render::indented(&own, render::INDENT));
        }
    }
    out.push_str(&lines_text(
        lines.get(see_also..).unwrap_or_default(),
        style,
    ));
    Ok(out)
}

/// The head of `help NAME`: the name asked for and its own summary, in place of the
/// page's top, which names all it covers (`help cmp` starts with cmp); its kind, the
/// other names of its page, and the notes on a hidden Windows program or `enable -n`.
fn page_head<SE: cash_core::ShellExtensions>(
    name: &str,
    registration: &builtins::Registration<SE>,
    page: Option<&Page>,
    style: Style,
) -> Result<String, cash_core::Error> {
    let paragraph =
        |text: &str| render::wrap(&render::inline(text, style.colour), style.width, "", 0);
    let mut out = render::wrap(
        &format!(
            "{} - {}",
            render::paint(name, render::COMMAND, style.colour),
            render::inline(&summary_of(name, registration)?, style.colour)
        ),
        style.width,
        "",
        2,
    );
    if let Some(page) = page {
        let _ = writeln!(out, "({})", page.kind);
        let others: Vec<String> = page
            .names
            .iter()
            .filter(|covered| *covered != name)
            .map(|other| format!("`{other}`"))
            .collect();
        if !others.is_empty() {
            out.push_str(&paragraph(&format!(
                "The same page covers {}.",
                others.join(", ")
            )));
        }
    }
    if let Some(program) = hidden_windows_program(name) {
        out.push('\n');
        out.push_str(&paragraph(&hiding_note(name, &program)));
    }
    if registration.disabled {
        out.push('\n');
        out.push_str(&paragraph(
            "It is disabled (`enable -n`): the name runs a command found on `PATH` \
             instead, until `enable` turns it back on.",
        ));
    }
    Ok(out)
}

/// The short form of a page: its syntax, or the builtin's usage line when it has none,
/// its examples, and where the whole page is.
fn short_form<SE: cash_core::ShellExtensions>(
    name: &str,
    registration: &builtins::Registration<SE>,
    drawn: Option<&cash_man::render::Drawn>,
    style: Style,
) -> Result<String, cash_core::Error> {
    let section = |title: &str| -> Option<&[cash_man::render::Line]> {
        let drawn = drawn?;
        let at = drawn
            .sections
            .iter()
            .position(|section| section.title == title)?;
        let start = drawn.sections.get(at)?.line;
        let end = drawn
            .sections
            .get(at + 1)
            .map_or(drawn.lines.len(), |next| next.line);
        let mut part = drawn.lines.get(start..end)?;
        while let [rest @ .., last] = part
            && last.spans.is_empty()
        {
            part = rest;
        }
        Some(part)
    };
    let mut out = String::from("\n");
    if let Some(syntax) = section("SYNTAX") {
        out.push_str(&lines_text(syntax, style));
    } else {
        out.push_str(&render::heading("Syntax", style));
        out.push_str(&render::indented(
            &usage_lines(name, registration)?,
            render::INDENT + 4,
        ));
    }
    if let Some(examples) = section("EXAMPLES") {
        out.push('\n');
        out.push_str(&lines_text(examples, style));
    }
    out.push('\n');
    out.push_str(&render::wrap(
        &render::inline(
            &format!("The whole page: `man {name}`, or `help -m {name}`."),
            style.colour,
        ),
        style.width,
        "",
        0,
    ));
    Ok(out)
}

/// The builtin's usage lines, from its own `--help`: `Usage: grep [OPTION]... PATTERNS
/// [FILE]...` and GNU's `or:` lines after it; failing those, its short usage.
fn usage_lines<SE: cash_core::ShellExtensions>(
    name: &str,
    registration: &builtins::Registration<SE>,
) -> Result<String, cash_core::Error> {
    let help = (registration.content_func)(
        name,
        builtins::ContentType::DetailedHelp,
        &builtins::ContentOptions::default(),
    )?;
    let mut lines = help.lines().map(str::trim);
    let mut usage: Vec<String> = Vec::new();
    if let Some(first) = lines.by_ref().find_map(|line| line.strip_prefix("Usage")) {
        // `Usage: cat [OPTION]...`, or `Usage` with the line on the next (`ping`).
        let first = first.trim_start_matches(':').trim();
        let first = if first.is_empty() {
            lines
                .by_ref()
                .find(|line| !line.is_empty())
                .unwrap_or_default()
        } else {
            first
        };
        usage.push(first.to_owned());
        usage.extend(
            lines
                .map_while(|line| line.strip_prefix("or:"))
                .map(|line| line.trim().to_owned()),
        );
    }
    if usage.iter().all(|line| line.is_empty()) {
        let short = short_usage(name, registration)?;
        return Ok(short
            .strip_prefix(&format!("{name}: "))
            .unwrap_or(&short)
            .to_owned());
    }
    Ok(usage.join("\n") + "\n")
}

/// Drawn lines as text, a line each, coloured as `style` says.
fn lines_text(lines: &[cash_man::render::Line], style: Style) -> String {
    let mut text = String::new();
    for line in lines {
        text.push_str(line.ansi(style.colour).trim_end());
        text.push('\n');
    }
    text
}

/// A page whole, as the manual draws it: a topic's.
fn drawn(page: &Page, style: Style) -> String {
    cash_man::render::draw(page, cash_man::manual(), style.width).text(style.colour)
}

/// `help` alone: every builtin, grouped by kind, each with its summary.
fn general_help<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    style: Style,
) -> String {
    let manual = cash_man::manual();
    let builtins = builtins_by_name(context);
    let mut out = String::new();
    if let Some(display) = context.shell.product_display_str() {
        let _ = writeln!(
            out,
            "{}\n",
            render::paint(display, render::HEADING, style.colour)
        );
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
            "*"
        } else if let Some(program) = hides {
            hidden.push(
                program
                    .file_name()
                    .map(|file| file.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            );
            "+"
        } else {
            " "
        };
        // The padding by hand: a coloured name is longer than the columns it takes.
        let first = format!(
            " {}{}{}  ",
            render::paint(marker, render::MARK, style.colour),
            render::paint(name, render::COMMAND, style.colour),
            " ".repeat(name_width.saturating_sub(render::visible_width(name)))
        );
        out.push_str(&render::wrap(
            &render::inline(summary, style.colour),
            style.width,
            &first,
            name_width + 4,
        ));
    };

    for kind in cash_man::KINDS {
        let members = kind_members(kind, &builtins);
        if members.is_empty() {
            continue;
        }
        out.push('\n');
        out.push_str(&kind_heading(kind, style));
        for (name, disabled, summary) in members {
            line(&mut out, name, disabled, summary);
        }
    }

    // A builtin the manual has no page for yet still gets listed, with its own words.
    let unknown: Vec<_> = builtins
        .iter()
        .filter(|(name, _)| {
            manual
                .page(name)
                .is_none_or(|page| !cash_man::KINDS.contains(&page.kind.as_str()))
        })
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
            "Windows has a program of the same name in System32 ({}). Inside cash the \
             builtin wins; to run the Windows one, give its path, as \
             `\"$SYSTEMROOT/System32/{}\"`.",
            hidden.join(", "),
            hidden.first().map_or("find.exe", String::as_str)
        );
        out.push_str(&render::wrap(
            &render::inline(&text, style.colour),
            style.width,
            &format!("{} ", render::paint("+", render::MARK, style.colour)),
            2,
        ));
    }
    if any_disabled {
        out.push_str(&render::wrap(
            &render::inline("Disabled with `enable -n`.", style.colour),
            style.width,
            &format!("{} ", render::paint("*", render::MARK, style.colour)),
            2,
        ));
    }
    out
}

fn kind_heading(kind: &str, style: Style) -> String {
    format!("{}\n", render::paint(kind, render::HEADING, style.colour))
}

/// `help topics`.
fn topic_list(style: Style) -> String {
    let topics = &cash_man::manual().topics;
    let width = topics
        .iter()
        .map(|topic| topic.file.len())
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
    for topic in topics {
        let first = format!(
            "  {}{}  ",
            render::paint(&topic.file, render::TOPIC, style.colour),
            " ".repeat(width.saturating_sub(topic.file.len()))
        );
        out.push_str(&render::wrap(
            &render::inline(&topic.summary, style.colour),
            style.width,
            &first,
            width + 4,
        ));
    }
    out
}

/// How many matching lines of one page a search shows.
const LINES_PER_HIT: usize = 3;

impl HelpCommand {
    /// `help search WORD`.
    fn search(
        &self,
        context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
        words: &[String],
        style: Style,
        out: &mut String,
    ) -> Result<ExecutionResult, cash_core::Error> {
        let help = self.invocation();
        let word = words.join(" ");
        if word.trim().is_empty() {
            self.complain(
                context,
                &format!("search: say what to search for: `{help} search WORD'"),
            )?;
            return Ok(ExecutionResult::general_error());
        }
        let hits = cash_man::manual().search(&word);
        if hits.is_empty() {
            self.complain(
                context,
                &format!(
                    "nothing mentions `{word}'. `{help}' lists the builtins, `{help} topics' \
                     the topics."
                ),
            )?;
            return Ok(ExecutionResult::general_error());
        }
        out.push_str(&search_results(hits, &word, style));
        Ok(ExecutionResult::success())
    }

    /// Says that `pattern` matched nothing, and what was meant, if a name is close.
    fn no_match(
        &self,
        context: &cash_core::ExecutionContext<'_, impl cash_core::ShellExtensions>,
        pattern: &str,
    ) -> Result<(), cash_core::Error> {
        let help = self.invocation();
        self.complain(context, &format!("no help topics match `{pattern}'."))?;
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
            "Try `{help} search {pattern}', `{help} topics', or `{help}' for every builtin."
        )?;
        Ok(())
    }
}

/// `line` with each place `word` occurs, case aside, in [`render::HIT`] when colour is on.
fn picked_out(line: &str, word: &str, colour: bool) -> String {
    let needle = word.to_ascii_lowercase();
    if !colour || needle.is_empty() {
        return line.to_owned();
    }
    // ASCII lowercasing keeps every byte where it was, so its offsets are the line's.
    let haystack = line.to_ascii_lowercase();
    let mut out = String::with_capacity(line.len());
    let mut from = 0;
    for (at, _) in haystack.match_indices(&needle) {
        let (Some(before), Some(found)) = (line.get(from..at), line.get(at..at + needle.len()))
        else {
            continue;
        };
        out.push_str(before);
        out.push_str(&render::paint(found, render::HIT, true));
        from = at + needle.len();
    }
    out.push_str(line.get(from..).unwrap_or_default());
    out
}

/// What `help search` prints for `hits`: each page's name, a topic's told from a
/// builtin's by its colour, and the lines that matched, `word` picked out in them.
fn search_results(hits: Vec<cash_man::search::Hit<'_>>, word: &str, style: Style) -> String {
    let mut out = String::new();
    for hit in hits {
        let (what, colour) = if hit.page.subject == Subject::Topic {
            ("topic", render::TOPIC)
        } else {
            ("builtin", render::COMMAND)
        };
        out.push_str(&render::wrap(
            &format!(
                "{} ({what}) - {}",
                render::paint(hit.name, colour, style.colour),
                render::inline(hit.page.summary_of(hit.name), style.colour)
            ),
            style.width,
            "",
            4,
        ));
        for line in hit.lines.iter().take(LINES_PER_HIT) {
            out.push_str(&render::wrap(
                &picked_out(line, word, style.colour),
                style.width,
                "    ",
                6,
            ));
        }
        if hit.lines.len() > LINES_PER_HIT {
            let more = hit.lines.len() - LINES_PER_HIT;
            let text = format!("    ... and {more} more: `help {}`", hit.name);
            let _ = writeln!(out, "{}", render::inline(&text, style.colour));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_takes_a_when_after_an_equals_sign_only() {
        let parsed = |args: &[&str]| {
            let parsed = HelpCommand::try_parse_from(args).unwrap();
            (parsed.colour, parsed.topic_patterns)
        };
        // Bare, as `ls --color`: always; the next word is a pattern, not the WHEN.
        assert_eq!(
            parsed(&["help", "--color", "flock"]),
            (Some(When::Always), vec!["flock".to_owned()])
        );
        assert_eq!(
            parsed(&["help", "--color=never", "tar"]),
            (Some(When::Never), vec!["tar".to_owned()])
        );
        assert_eq!(parsed(&["help", "--colour=auto"]).0, Some(When::Auto));
        assert!(HelpCommand::try_parse_from(["help", "--color=bogus"]).is_err());
    }

    #[test]
    fn a_search_picks_out_the_word_wherever_it_is_case_aside() {
        assert_eq!(
            picked_out("A Pipe and a pipe", "pipe", true),
            "A \x1b[1;33mPipe\x1b[0m and a \x1b[1;33mpipe\x1b[0m"
        );
        assert_eq!(picked_out("A Pipe", "pipe", false), "A Pipe");
        assert_eq!(picked_out("nothing here", "pipe", true), "nothing here");
    }
}
