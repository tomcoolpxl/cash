//! `man`: cash's own manual, in a browser (`research/man-browser-design.md`).
//!
//! Not man(1): the pages are cash's, one for each command it has and for its topics,
//! from `cash-man`. In a terminal `man NAME` opens the browser on NAME's page, and `man`
//! alone on Getting started; anywhere else it prints the page as plain text. `man -k
//! WORD` lists the commands and topics whose name or summary holds WORD.

use std::collections::HashMap;
use std::io::Write as _;

use cash_core::{ExecutionResult, builtins};
use cash_man::Manual;
use cash_man::browser::{Browser, Key, Outcome};
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::top::{FullScreen, RawMode, terminal_size};

/// Show cash's manual: a page for each command and topic.
#[derive(Parser)]
pub(crate) struct ManCommand {
    /// Print the commands and topics whose name or summary holds WORD.
    #[arg(short = 'k', value_name = "WORD")]
    apropos: Option<String>,

    /// The command or topic to show; Getting started without one.
    name: Option<String>,
}

impl builtins::Command for ManCommand {
    type Error = cash_core::Error;

    async fn execute<SE: cash_core::ShellExtensions>(
        &self,
        context: cash_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let manual = cash_man::manual();
        if let Some(word) = &self.apropos {
            return apropos(&context, manual, word);
        }
        let name = self.name.as_deref();
        if let Some(name) = name
            && manual.find(name).is_none()
            && !manual
                .topics
                .iter()
                .any(|topic| topic.title.eq_ignore_ascii_case(name))
        {
            writeln!(context.stderr(), "man: no page for {name}")?;
            let builtins: Vec<&str> = context
                .shell
                .builtins()
                .keys()
                .map(String::as_str)
                .collect();
            let close = crate::helpdocs::suggestions(name, &builtins);
            if !close.is_empty() {
                writeln!(context.stderr(), "Did you mean: {}?", close.join(", "))?;
            }
            writeln!(context.stderr(), "`man -k {name}' searches the summaries.")?;
            return Ok(ExecutionResult::general_error());
        }
        let helps: HashMap<String, builtins::CommandContentFunc> = context
            .shell
            .builtins()
            .iter()
            .map(|(name, registration)| (name.clone(), registration.content_func))
            .collect();
        let own_help = move |name: &str| {
            let content = helps.get(name)?;
            content(
                name,
                builtins::ContentType::DetailedHelp,
                &builtins::ContentOptions::default(),
            )
            .ok()
        };
        let terminal = context
            .try_fd(cash_core::openfiles::OpenFiles::STDOUT_FD)
            .is_some_and(|fd| fd.is_terminal());
        if !terminal {
            let page = name
                .and_then(|name| {
                    manual.find(name).or_else(|| {
                        manual
                            .topics
                            .iter()
                            .find(|topic| topic.title.eq_ignore_ascii_case(name))
                    })
                })
                .or_else(|| manual.topic("getting-started"));
            if let Some(page) = page {
                let width = context
                    .shell
                    .env_str("COLUMNS")
                    .and_then(|value| value.trim().parse::<usize>().ok())
                    .unwrap_or(80)
                    .clamp(40, 200);
                let mut drawn = cash_man::render::draw(page, manual, width);
                if page.draft
                    && let Some(own) = page.names.first().and_then(|name| own_help(name))
                {
                    cash_man::render::add_own_help(&mut drawn, &own);
                }
                write!(context.stdout(), "{}", drawn.text(false))?;
            }
            return Ok(ExecutionResult::success());
        }
        let colour = context
            .shell
            .env_str("NO_COLOR")
            .is_none_or(|value| value.is_empty());
        let browser = Browser::new(manual, name, colour, own_help);
        let mut screen = FullScreen::enter(context.stdout())?;
        let _raw = RawMode::enter();
        browse(screen.out(), browser)?;
        Ok(ExecutionResult::success())
    }
}

/// `man -k WORD`: each command and topic whose name or summary holds `word`, case
/// aside, the commands from A to Z and then the topics.
fn apropos<SE: cash_core::ShellExtensions>(
    context: &cash_core::ExecutionContext<'_, SE>,
    manual: &Manual,
    word: &str,
) -> Result<ExecutionResult, cash_core::Error> {
    let needle = word.to_lowercase();
    let holds = |text: &str| text.replace('`', "").to_lowercase().contains(&needle);
    let mut found: Vec<(String, String)> = manual
        .pages
        .iter()
        .flat_map(|page| {
            page.names
                .iter()
                .map(move |name| (name.clone(), page.summary_of(name).to_owned()))
        })
        .filter(|(name, summary)| holds(name) || holds(summary))
        .collect();
    found.sort_by_key(|(name, _)| name.to_lowercase());
    found.extend(
        manual
            .topics
            .iter()
            .filter(|topic| holds(&topic.file) || holds(&topic.title) || holds(&topic.summary))
            .map(|topic| (topic.file.clone(), topic.summary.clone())),
    );
    if found.is_empty() {
        writeln!(context.stderr(), "man: nothing appropriate for {word}")?;
        return Ok(ExecutionResult::general_error());
    }
    let width = found
        .iter()
        .map(|(name, _)| name.chars().count())
        .max()
        .unwrap_or_default();
    let mut out = context.stdout();
    for (name, summary) in found {
        writeln!(out, "{name:<width$}  {}", summary.replace('`', ""))?;
    }
    Ok(ExecutionResult::success())
}

/// The browser on the screen until it is left: a frame, then a key or a resize.
fn browse(out: &mut impl std::io::Write, mut browser: Browser<'_>) -> std::io::Result<()> {
    let (mut width, mut height) = terminal_size();
    loop {
        for (row, line) in browser.frame(width, height).iter().enumerate() {
            write!(out, "\x1b[{};1H{line}", row + 1)?;
        }
        out.flush()?;
        match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                let control = key.modifiers.contains(KeyModifiers::CONTROL);
                if control && key.code == KeyCode::Char('c') {
                    return Ok(());
                }
                if let Some(key) = key_of(key)
                    && browser.key(key) == Outcome::Quit
                {
                    return Ok(());
                }
            }
            Event::Resize(columns, rows) => {
                width = usize::from(columns).max(1);
                height = usize::from(rows).max(1);
                write!(out, "\x1b[2J")?;
            }
            _ => {}
        }
    }
}

/// The browser's key for a key pressed.
const fn key_of(key: KeyEvent) -> Option<Key> {
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    Some(match key.code {
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::Left if alt => Key::Back,
        KeyCode::Right if alt => Key::Forward,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Tab | KeyCode::BackTab => Key::Tab,
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Esc,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Char('f') if control => Key::FindEverywhere,
        KeyCode::Char(c) if !control && !alt => Key::Char(c),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_map_to_the_browser_s() {
        let key = |code, modifiers| key_of(KeyEvent::new(code, modifiers));
        assert_eq!(key(KeyCode::Left, KeyModifiers::ALT), Some(Key::Back));
        assert_eq!(key(KeyCode::Left, KeyModifiers::NONE), Some(Key::Left));
        assert_eq!(
            key(KeyCode::Char('f'), KeyModifiers::CONTROL),
            Some(Key::FindEverywhere)
        );
        assert_eq!(
            key(KeyCode::Char('x'), KeyModifiers::NONE),
            Some(Key::Char('x'))
        );
        assert_eq!(key(KeyCode::Char('x'), KeyModifiers::ALT), None);
        assert_eq!(key(KeyCode::BackTab, KeyModifiers::SHIFT), Some(Key::Tab));
    }
}
