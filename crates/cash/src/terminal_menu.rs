//! Windows Terminal's new-tab menu (the + dropdown), for `cash --terminal-profile`.
//!
//! A user who laid out `newTabMenu` in `settings.json` lists profiles one by one, and a
//! profile an installer adds through a fragment then shows nowhere in the menu unless the
//! list has a `remainingProfiles` entry. That is what the first Scoop install of 1.1.0 did
//! on the author's machine (2026-09-28): the profile loaded and could not be opened from
//! the + menu. A fragment cannot change the menu, so cash adds its profile to such a menu
//! in `settings.json` itself, and takes it out again on removal.
//!
//! `settings.json` is the user's own file, JSON with comments, so it is never parsed and
//! written back whole: one entry is inserted into, or cut out of, the text, and every
//! other byte stays as it was. Terminal reloads the file when it changes, fragments
//! included, so the profile shows at once.

use cash_win32::terminal::jsonc::{Kind, Token, tokens};

/// `text` with the profile `guid` added at the end of its new-tab menu, or `None` when
/// the menu shows it already: there is no `newTabMenu` (Terminal's own menu lists every
/// profile), it has a `remainingProfiles` entry, or it names `guid`. `None` too when the
/// text is not the JSON Terminal would read.
pub fn with_profile(text: &str, guid: &str) -> Option<String> {
    let tokens = tokens(text)?;
    let menu = menu(text, &tokens)?;
    let body = text.get(tokens.get(menu.open)?.end..tokens.get(menu.close)?.start)?;
    if body.contains("\"remainingProfiles\"")
        || body
            .to_ascii_lowercase()
            .contains(&guid.to_ascii_lowercase())
    {
        return None;
    }

    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let indent = match menu.elements.first() {
        Some(&(first, _)) => line_indent(text, tokens.get(first)?.start),
        None => format!("{}    ", line_indent(text, tokens.get(menu.open)?.start)),
    };
    let entry = format!(
        "{{{newline}{indent}    \"profile\": \"{guid}\",{newline}{indent}    \"type\": \"profile\"{newline}{indent}}}"
    );

    // After the last entry, and after a trailing comma if the user left one.
    let before_close = tokens.get(menu.close.checked_sub(1)?)?;
    let (at, insert) = match menu.elements.last() {
        Some(_) if before_close.kind == Kind::Punct(b',') => {
            (before_close.end, format!("{newline}{indent}{entry}"))
        }
        Some(&(_, end)) => (
            tokens.get(end.checked_sub(1)?)?.end,
            format!(",{newline}{indent}{entry}"),
        ),
        None => (
            tokens.get(menu.open)?.end,
            format!("{newline}{indent}{entry}"),
        ),
    };
    Some(format!("{}{insert}{}", text.get(..at)?, text.get(at..)?))
}

/// `text` without the new-tab menu entries for the profile `guid`, or `None` when it has
/// none. The text an entry [`with_profile`] added is cut out exactly, so adding then
/// removing gives back the same bytes.
pub fn without_profile(text: &str, guid: &str) -> Option<String> {
    let guid = guid.to_ascii_lowercase();
    let mut text = text.to_owned();
    let mut changed = false;
    loop {
        let tokens = tokens(&text)?;
        let Some(menu) = menu(&text, &tokens) else {
            break;
        };
        let names_guid = |&(start, end): &(usize, usize)| {
            let span = tokens
                .get(start)
                .zip(end.checked_sub(1).and_then(|last| tokens.get(last)))
                .and_then(|(first, last)| text.get(first.start..last.end));
            span.is_some_and(|span| {
                span.starts_with('{')
                    && span.contains("\"profile\"")
                    && span.to_ascii_lowercase().contains(&guid)
            })
        };
        let Some(index) = menu.elements.iter().position(names_guid) else {
            break;
        };
        let (start, end) = *menu.elements.get(index)?;
        let cut_end = tokens.get(end.checked_sub(1)?)?.end;
        let cut = if index > 0 {
            // From the end of the entry before, taking the comma between them.
            let (_, before) = *menu.elements.get(index - 1)?;
            tokens.get(before.checked_sub(1)?)?.end..cut_end
        } else if let Some(&(next, _)) = menu.elements.get(1) {
            tokens.get(start)?.start..tokens.get(next)?.start
        } else {
            tokens.get(menu.open)?.end..cut_end
        };
        text.replace_range(cut, "");
        changed = true;
    }
    changed.then_some(text)
}

/// The top-level `newTabMenu` array: the tokens of its brackets, and each entry's tokens
/// as `start..end`.
struct Menu {
    open: usize,
    close: usize,
    elements: Vec<(usize, usize)>,
}

/// The token just past the value that starts at `i`.
fn value_end(tokens: &[Token], i: usize) -> Option<usize> {
    match tokens.get(i)?.kind {
        Kind::Punct(b'{' | b'[') => {
            let mut depth = 0usize;
            let mut j = i;
            loop {
                match tokens.get(j)?.kind {
                    Kind::Punct(b'{' | b'[') => depth += 1,
                    Kind::Punct(b'}' | b']') => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(j + 1);
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
        }
        Kind::Str | Kind::Word => Some(i + 1),
        Kind::Punct(_) => None,
    }
}

fn menu(text: &str, tokens: &[Token]) -> Option<Menu> {
    if tokens.first()?.kind != Kind::Punct(b'{') {
        return None;
    }
    let mut i = 1;
    loop {
        let token = tokens.get(i)?;
        match token.kind {
            Kind::Punct(b',') => i += 1,
            Kind::Str => {
                let key = text.get(token.start + 1..token.end - 1)?;
                if tokens.get(i + 1)?.kind != Kind::Punct(b':') {
                    return None;
                }
                let value = i + 2;
                let end = value_end(tokens, value)?;
                if key == "newTabMenu" {
                    if tokens.get(value)?.kind != Kind::Punct(b'[') {
                        return None;
                    }
                    let close = end - 1;
                    let mut elements = Vec::new();
                    let mut j = value + 1;
                    while j < close {
                        if tokens.get(j)?.kind == Kind::Punct(b',') {
                            j += 1;
                        } else {
                            let element_end = value_end(tokens, j)?;
                            elements.push((j, element_end));
                            j = element_end;
                        }
                    }
                    return Some(Menu {
                        open: value,
                        close,
                        elements,
                    });
                }
                i = end;
            }
            _ => return None,
        }
    }
}

/// The spaces and tabs that begin the line holding byte `at`.
fn line_indent(text: &str, at: usize) -> String {
    let line_start = text
        .get(..at)
        .and_then(|before| before.rfind('\n'))
        .map_or(0, |newline| newline + 1);
    text.get(line_start..)
        .unwrap_or_default()
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GUID: &str = "{43e4cdd3-eb67-5e13-bd17-fa0d7f8cf3ff}";

    /// The shape of the author's own settings: a menu listing profiles one by one.
    const LISTED: &str = r#"{
    "defaultProfile": "{465d1d2d-478a-4eee-8c87-cd7cafd28372}",
    "newTabMenu":
    [
        {
            "icon": null,
            "profile": "{465d1d2d-478a-4eee-8c87-cd7cafd28372}",
            "type": "profile"
        },
        {
            "icon": null,
            "profile": "{c766fc77-e242-4828-b6ca-fb1f3709929e}",
            "type": "profile"
        }
    ],
    "profiles": { "list": [] }
}
"#;

    #[test]
    fn a_listed_menu_gets_the_profile_last_and_gives_it_back() {
        let added = with_profile(LISTED, GUID).unwrap_or_default();
        assert!(
            added.contains(
                "        },\n        {\n            \"profile\": \"{43e4cdd3-eb67-5e13-bd17-fa0d7f8cf3ff}\",\n            \"type\": \"profile\"\n        }\n    ],"
            ),
            "{added}"
        );
        assert_eq!(with_profile(&added, GUID), None, "added twice");
        assert_eq!(without_profile(&added, GUID).as_deref(), Some(LISTED));
        assert_eq!(without_profile(LISTED, GUID), None);
    }

    #[test]
    fn menus_that_show_every_profile_are_left_alone() {
        assert_eq!(
            with_profile(r#"{ "profiles": { "list": [] } }"#, GUID),
            None
        );
        let remaining = r#"{ "newTabMenu": [ { "type": "remainingProfiles" } ] }"#;
        assert_eq!(with_profile(remaining, GUID), None);
        let in_folder = r#"{ "newTabMenu": [ { "type": "folder", "entries": [ { "type": "remainingProfiles" } ] } ] }"#;
        assert_eq!(with_profile(in_folder, GUID), None);
    }

    #[test]
    fn comments_and_strings_do_not_fool_it() {
        let text = "{\r\n    // \"newTabMenu\": [] was here\r\n    /* \"newTabMenu\": [ */\r\n    \"name\": \"a \\\"newTabMenu\\\" [\",\r\n    \"newTabMenu\": [ { \"type\": \"profile\", \"profile\": \"{x}\" }, ] // trailing comma\r\n}\r\n";
        let added = with_profile(text, GUID).unwrap_or_default();
        // After the user's trailing comma, in the file's own CRLF.
        assert!(added.contains("},\r\n    {\r\n"), "{added:?}");
        assert!(added.contains(GUID), "{added:?}");
        assert!(!added.contains("\n    {\n"), "kept CRLF: {added:?}");
        assert!(
            added.contains("// \"newTabMenu\": [] was here"),
            "{added:?}"
        );
        // Taking it out again drops that trailing comma too: still what Terminal reads.
        let removed = without_profile(&added, GUID).unwrap_or_default();
        assert!(!removed.contains(GUID), "{removed:?}");
        assert!(removed.contains("\"profile\": \"{x}\" } ]"), "{removed:?}");
    }

    #[test]
    fn an_empty_menu_and_a_first_entry_come_out_clean() {
        let empty = "{\n    \"newTabMenu\": []\n}\n";
        let added = with_profile(empty, GUID).unwrap_or_default();
        assert!(added.contains(GUID), "{added}");
        assert_eq!(without_profile(&added, GUID).as_deref(), Some(empty));

        let first = format!(
            r#"{{ "newTabMenu": [ {{ "profile": "{GUID}", "type": "profile" }}, {{ "type": "profile", "profile": "{{x}}" }} ] }}"#
        );
        assert_eq!(
            without_profile(&first, GUID).as_deref(),
            Some(r#"{ "newTabMenu": [ { "type": "profile", "profile": "{x}" } ] }"#)
        );
    }

    #[test]
    fn text_terminal_could_not_read_is_left_alone() {
        assert_eq!(with_profile("{ \"newTabMenu\": [ \"unclosed", GUID), None);
        assert_eq!(with_profile("", GUID), None);
        assert_eq!(with_profile("{ \"newTabMenu\": {} }", GUID), None);
    }
}
