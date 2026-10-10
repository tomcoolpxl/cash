//! Where the picker starts from the word being typed, and how a pick is written back on
//! the line (spec D73).

use std::path::{Path, PathBuf};

/// The folder to start in and the filter to start with, from the word under the cursor.
///
/// `~/src/fo` starts in `~/src` filtered by `fo`, `D:/x/` in `D:/x`, and a word that
/// names no folder in the current folder, filtered by its last part.
#[must_use]
pub fn start(word: &str, cwd: &Path, home: Option<&Path>) -> (PathBuf, String) {
    if word.is_empty() {
        return (cwd.to_path_buf(), String::new());
    }
    let expanded = match (word.strip_prefix('~'), home) {
        (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with(['/', '\\']) => {
            format!("{}{rest}", home.to_string_lossy())
        }
        _ => word.to_owned(),
    };
    let absolute = |text: &str| {
        let path = cash_win32::path::accept_path(text);
        if path.is_absolute() {
            path
        } else {
            cwd.join(path)
        }
    };
    let (folder, last) = match expanded.rfind(['/', '\\']) {
        Some(at) => (
            expanded.get(..=at).unwrap_or_default(),
            expanded.get(at + 1..).unwrap_or_default(),
        ),
        None => ("", expanded.as_str()),
    };
    if folder.is_empty() {
        return (cwd.to_path_buf(), last.to_owned());
    }
    // `C:/x//` is `C:/x`; a drive's root keeps its `/`.
    let trimmed = folder.trim_end_matches(['/', '\\']);
    let folder = if trimmed.is_empty() || trimmed.ends_with(':') {
        absolute(&format!("{trimmed}/"))
    } else {
        absolute(trimmed)
    };
    if folder.is_dir() {
        (folder, last.to_owned())
    } else {
        (cwd.to_path_buf(), last.to_owned())
    }
}

/// How `pick` is written on the line: relative when it is below `cwd` (`src/lib/`), `~/…`
/// under `home`, else in cash's `C:/…` spelling; quoted only where it needs quoting; a
/// folder ends in `/`.
#[must_use]
pub fn written(pick: &Path, folder: bool, cwd: &Path, home: Option<&Path>) -> String {
    let pick_text = cash_win32::path::render(pick);
    let trailing = if folder { "/" } else { "" };
    if let Some(rest) = below(&pick_text, &cash_win32::path::render(cwd)) {
        let rest = if rest.is_empty() { "." } else { rest };
        return format!("{}{trailing}", quoted(rest));
    }
    if let Some(home) = home
        && let Some(rest) = below(&pick_text, &cash_win32::path::render(home))
    {
        return if rest.is_empty() {
            "~/".to_owned()
        } else {
            format!("~/{}{trailing}", quoted(rest))
        };
    }
    format!("{}{trailing}", quoted(&pick_text))
}

/// The part of `path` below `folder`, compared as Windows compares names (case aside);
/// `Some("")` for the folder itself.
fn below<'a>(path: &'a str, folder: &str) -> Option<&'a str> {
    let folder = folder.trim_end_matches('/');
    let head = path.get(..folder.len())?;
    if !head.eq_ignore_ascii_case(folder) && !cash_win32::fold::same_name(head, folder) {
        return None;
    }
    let rest = path.get(folder.len()..)?;
    if rest.is_empty() {
        Some("")
    } else {
        rest.strip_prefix('/')
    }
}

/// `text` as one shell word: as it is when nothing in it is special to the shell, else in
/// single quotes, with each `'` written `'\''`.
fn quoted(text: &str) -> String {
    let plain = !text.is_empty()
        && text.chars().all(|c| {
            c.is_alphanumeric()
                || matches!(c, '/' | '.' | '-' | '_' | ':' | '+' | ',' | '@' | '%' | '=')
        })
        && !text.starts_with(['-', '=']);
    if plain {
        text.to_owned()
    } else {
        format!("'{}'", text.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> PathBuf {
        PathBuf::from(r"C:\Users\me")
    }

    #[test]
    fn a_pick_below_the_current_folder_is_relative() {
        let cwd = home().join("github");
        let pick = cwd.join("cash").join("crates");
        assert_eq!(written(&pick, true, &cwd, Some(&home())), "cash/crates/");
        assert_eq!(written(&cwd, true, &cwd, Some(&home())), "./");
    }

    #[test]
    fn a_pick_under_home_starts_with_a_tilde() {
        let cwd = PathBuf::from(r"D:\work");
        let pick = home().join("notes").join("todo.md");
        assert_eq!(
            written(&pick, false, &cwd, Some(&home())),
            "~/notes/todo.md"
        );
        assert_eq!(written(&home(), true, &cwd, Some(&home())), "~/");
    }

    #[test]
    fn other_picks_are_absolute_in_cash_spelling() {
        let pick = PathBuf::from(r"D:\data\x.csv");
        assert_eq!(
            written(&pick, false, &home(), Some(&home())),
            "D:/data/x.csv"
        );
    }

    #[test]
    fn only_what_needs_it_is_quoted() {
        let pick = PathBuf::from(r"D:\My Files\it's.txt");
        assert_eq!(
            written(&pick, false, &home(), Some(&home())),
            r"'D:/My Files/it'\''s.txt'"
        );
        let pick = home().join("My Docs");
        assert_eq!(
            written(&pick, true, Path::new(r"D:\"), Some(&home())),
            "~/'My Docs'/"
        );
    }

    #[test]
    fn the_start_comes_from_the_typed_folder() {
        let dir = std::env::temp_dir();
        let typed = format!("{}/zz", cash_win32::path::render(&dir));
        let (folder, filter) = start(&typed, Path::new(r"C:\"), None);
        let rendered = |path: &Path| {
            cash_win32::path::render(path)
                .trim_end_matches('/')
                .to_owned()
        };
        assert!(
            cash_win32::fold::same_name(&rendered(&folder), &rendered(&dir)),
            "{folder:?} is not {dir:?}"
        );
        assert_eq!(filter, "zz");

        let (folder, filter) = start("fo", &home(), Some(&home()));
        assert_eq!((folder, filter.as_str()), (home(), "fo"));
        let (folder, filter) = start("no/such/place/x", &home(), Some(&home()));
        assert_eq!((folder, filter.as_str()), (home(), "x"));
    }

    #[test]
    fn every_spelling_of_a_folder_starts_the_picker_in_it() {
        let parent = tempfile::tempdir().unwrap();
        let dir = parent.path().join("spell dir");
        std::fs::create_dir(&dir).unwrap();
        let fwd = cash_win32::path::render(&dir);
        let (drive, rest) = fwd.split_at(2);
        let letter = drive.get(..1).unwrap();
        let back = fwd.replace('/', "\\");
        let spellings = [
            format!("{fwd}/in"),
            format!("{}{rest}/in", drive.to_lowercase()),
            format!("{back}\\in"),
            format!("{fwd}\\in"),
            format!("/{}{rest}/in", letter.to_lowercase()),
            "~/spell dir/in".to_owned(),
            "spell dir/in".to_owned(),
            "./spell dir/in".to_owned(),
            format!("//localhost/{letter}${rest}/in"),
            format!("\\\\localhost\\{letter}${}\\in", rest.replace('/', "\\")),
        ];
        for typed in spellings {
            let (folder, filter) = start(&typed, parent.path(), Some(parent.path()));
            let shown = cash_win32::path::render(&folder);
            assert!(
                folder.is_dir() && shown.trim_end_matches('/').ends_with("spell dir"),
                "{typed}: {shown}"
            );
            assert_eq!(filter, "in", "{typed}");
        }
    }
}
