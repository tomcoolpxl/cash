//! The picker's colours: entries as `ls` colours them (`LS_COLORS`, through the same
//! `lscolors` crate), and its own parts from `CASH_PICKER_COLORS` (spec D73).

use lscolors::{Indicator, LsColors};

/// SGR parameters for the picker's own parts, and `LS_COLORS` for entries.
pub struct Colours {
    ls: Option<LsColors>,
    /// The selected line.
    pub selected: String,
    /// The letters a filter matched.
    pub matched: String,
    /// The header and the folder path in it.
    pub frame: String,
    /// The status and key hints at the bottom.
    pub status: String,
    /// `… N more` lines and tree branches.
    pub dim: String,
}

impl Colours {
    /// No colour at all: for a terminal that should get none (`NO_COLOR`).
    #[must_use]
    pub fn none() -> Self {
        Self {
            ls: None,
            selected: "7".to_owned(),
            matched: String::new(),
            frame: String::new(),
            status: String::new(),
            dim: String::new(),
        }
    }

    /// Colours from `ls_colors` (`LS_COLORS`, or dircolors' defaults when it is unset)
    /// and `picker` (`CASH_PICKER_COLORS`: `sel=…:match=…:frame=…:status=…:dim=…`).
    #[must_use]
    pub fn new(ls_colors: &str, picker: Option<&str>) -> Self {
        let mut colours = Self {
            ls: Some(LsColors::from_string(ls_colors)),
            selected: "7".to_owned(),
            matched: "1;33".to_owned(),
            frame: "1".to_owned(),
            status: "2".to_owned(),
            dim: "2".to_owned(),
        };
        for pair in picker.unwrap_or_default().split(':') {
            let Some((name, sgr)) = pair.split_once('=') else {
                continue;
            };
            if !sgr.chars().all(|c| c.is_ascii_digit() || c == ';') {
                continue;
            }
            let slot = match name.trim() {
                "sel" => &mut colours.selected,
                "match" => &mut colours.matched,
                "frame" => &mut colours.frame,
                "status" => &mut colours.status,
                "dim" => &mut colours.dim,
                _ => continue,
            };
            sgr.clone_into(slot);
        }
        colours
    }

    /// The SGR parameters for an entry, as `ls` would colour it.
    #[must_use]
    pub fn entry(&self, name: &str, folder: bool) -> String {
        let Some(ls) = &self.ls else {
            return String::new();
        };
        let style = if folder {
            ls.style_for_indicator(Indicator::Directory)
        } else {
            ls.style_for_str(name).or_else(|| {
                executable(name)
                    .then(|| ls.style_for_indicator(Indicator::ExecutableFile))
                    .flatten()
            })
        };
        style.map_or_else(String::new, |style| {
            let prefix = style.to_nu_ansi_term_style().prefix().to_string();
            // `\x1b[1;34m` to `1;34`.
            prefix
                .strip_prefix("\x1b[")
                .and_then(|rest| rest.strip_suffix('m'))
                .unwrap_or_default()
                .to_owned()
        })
    }
}

/// Whether Windows runs a file of this name as a program, as `ls` colours them.
fn executable(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".exe", ".com", ".bat", ".cmd", ".ps1"]
        .iter()
        .any(|extension| lower.ends_with(extension))
}

/// `text` in the colour `sgr`, or as it is for none.
#[must_use]
pub fn paint(sgr: &str, text: &str) -> String {
    if sgr.is_empty() {
        text.to_owned()
    } else {
        format!("\x1b[{sgr}m{text}\x1b[0m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_and_files_take_ls_colours() {
        let colours = Colours::new("di=01;34:*.zip=01;31:ex=01;32", None);
        assert_eq!(colours.entry("src", true), "1;34");
        assert_eq!(colours.entry("x.zip", false), "1;31");
        assert_eq!(colours.entry("run.exe", false), "1;32");
        assert_eq!(colours.entry("notes.txt", false), "");
    }

    #[test]
    fn the_picker_variable_sets_its_own_parts() {
        let colours = Colours::new("", Some("sel=1;37;44:match=4:bogus=1:frame=x"));
        assert_eq!(
            (
                colours.selected.as_str(),
                colours.matched.as_str(),
                colours.frame.as_str()
            ),
            ("1;37;44", "4", "1")
        );
        assert_eq!(paint("4", "a"), "\x1b[4ma\x1b[0m");
        assert_eq!(paint("", "a"), "a");
    }
}
