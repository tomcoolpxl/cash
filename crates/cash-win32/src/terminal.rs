//! Windows Terminal's settings, read the way Terminal reads them, for what cash does inside
//! it: the + menu entry `cash --terminal-profile` adds, and the font of the tab that
//! `ls --icons` draws in.
//!
//! Terminal gives every shell it starts `WT_SESSION` and `WT_PROFILE_ID`, the GUID of the
//! profile (microsoft/terminal#4852). A profile's font is in `settings.json`, or in the
//! fragment that brings the profile; Terminal draws Cascadia Mono when none names one.
//! No terminal can be asked which font it draws in, so this is how cash knows whether
//! Nerd Font glyphs will show or come out as empty boxes.

use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{ERROR_NO_MORE_ITEMS, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, RegCloseKey, RegEnumValueW,
    RegOpenKeyExW,
};

use self::jsonc::Value;

/// The font Terminal draws in when a profile names none.
pub const DEFAULT_FONT: &str = "Cascadia Mono";

/// A key under `HKEY_CURRENT_USER` whose value names stand in for the installed fonts.
/// Tests set it, so that which fonts a machine has does not decide what they see.
pub const FONTS_KEY_VAR: &str = "CASH_FONTS_KEY";

/// The settings files of the Windows Terminal installs this user has: the Store's
/// release, Preview and Canary, and an unpackaged one's.
pub fn settings_files(local_app_data: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(local_app_data.join("Packages"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            name.starts_with("microsoft.windowsterminal") && name.ends_with("_8wekyb3d8bbwe")
        })
        .map(|entry| entry.path().join("LocalState").join("settings.json"))
        .collect();
    files.push(
        local_app_data
            .join("Microsoft")
            .join("Windows Terminal")
            .join("settings.json"),
    );
    files.retain(|file| file.is_file());
    files.sort();
    files
}

/// The fragment files Terminal reads: each application's folder under
/// `Fragments`, for this user and for all users.
pub fn fragment_files(local_app_data: &Path, program_data: Option<&Path>) -> Vec<PathBuf> {
    let mut roots = vec![local_app_data.join("Microsoft").join("Windows Terminal")];
    if let Some(program_data) = program_data {
        roots.push(program_data.join("Microsoft").join("Windows Terminal"));
    }
    let mut files: Vec<PathBuf> = roots
        .iter()
        .flat_map(|root| {
            std::fs::read_dir(root.join("Fragments"))
                .into_iter()
                .flatten()
        })
        .flatten()
        .flat_map(|app| std::fs::read_dir(app.path()).into_iter().flatten())
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        })
        .collect();
    files.sort();
    files
}

/// The font face Terminal draws the profile `guid` in.
///
/// Resolved as Terminal layers a profile's settings: the user's own entry for it in
/// `settings.json`, then `profiles.defaults`, then the fragment that brings or updates
/// it, then [`DEFAULT_FONT`]. Of several Terminal installs, the one whose settings list
/// the profile is read.
pub fn profile_font(local_app_data: &Path, program_data: Option<&Path>, guid: &str) -> String {
    let guid = guid.to_ascii_lowercase();
    let settings: Vec<Value> = settings_files(local_app_data)
        .iter()
        .filter_map(|file| std::fs::read_to_string(file).ok())
        .filter_map(|text| jsonc::parse(&text))
        .collect();
    let chosen = settings
        .iter()
        .find(|install| user_profiles(install).any(|profile| names(profile, &guid)))
        .or_else(|| settings.first());
    if let Some(install) = chosen {
        let own = user_profiles(install).find(|profile| names(profile, &guid));
        let defaults = install
            .get("profiles")
            .and_then(|profiles| profiles.get("defaults"));
        if let Some(face) = own.and_then(face_of).or_else(|| defaults.and_then(face_of)) {
            return face;
        }
    }
    for fragment in fragment_files(local_app_data, program_data) {
        let Some(fragment) = std::fs::read_to_string(&fragment)
            .ok()
            .and_then(|text| jsonc::parse(&text))
        else {
            continue;
        };
        let face = fragment
            .get("profiles")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|profile| names(profile, &guid) || updates(profile, &guid))
            .find_map(face_of);
        if let Some(face) = face {
            return face;
        }
    }
    DEFAULT_FONT.to_owned()
}

/// The profiles a `settings.json` lists: `profiles.list`, or `profiles` itself in the
/// older layout where it is the list.
fn user_profiles(settings: &Value) -> impl Iterator<Item = &Value> {
    let profiles = settings.get("profiles");
    let list = match profiles {
        Some(Value::Array(list)) => Some(list.as_slice()),
        Some(object) => object.get("list").and_then(Value::as_array),
        None => None,
    };
    list.into_iter().flatten()
}

fn names(profile: &Value, guid: &str) -> bool {
    profile
        .get("guid")
        .and_then(Value::as_str)
        .is_some_and(|own| own.eq_ignore_ascii_case(guid))
}

fn updates(profile: &Value, guid: &str) -> bool {
    profile
        .get("updates")
        .and_then(Value::as_str)
        .is_some_and(|target| target.eq_ignore_ascii_case(guid))
}

/// A profile's font face: `font.face`, or the older `fontFace`.
fn face_of(profile: &Value) -> Option<String> {
    profile
        .get("font")
        .and_then(|font| font.get("face"))
        .or_else(|| profile.get("fontFace"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|face| !face.is_empty())
        .map(str::to_owned)
}

/// Whether a font face is a Nerd Font, which has the glyphs `ls --icons` draws.
///
/// Nerd Fonts say so in the name: `… Nerd Font`, `… Nerd Font Mono`, or the short `NF`,
/// `NFM` and `NFP` (Microsoft's own `Cascadia Mono NF` too). A list of faces, as Terminal
/// takes one, is a Nerd Font when any of them is.
pub fn is_nerd_font(face: &str) -> bool {
    face.split(',').any(|one| {
        let lower = one.trim().to_ascii_lowercase();
        lower.contains("nerd font")
            || lower
                .split_whitespace()
                .any(|word| matches!(word, "nf" | "nfm" | "nfp"))
    })
}

/// The fonts Windows has installed, for this user and for all, by the names it lists
/// them under (`Cascadia Mono Regular (TrueType)`, `CaskaydiaMono NFM (TrueType)`).
pub fn installed_fonts() -> Vec<String> {
    const FONTS: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts";
    if let Ok(key) = std::env::var(FONTS_KEY_VAR) {
        return value_names(HKEY_CURRENT_USER, &key);
    }
    let mut fonts = value_names(HKEY_LOCAL_MACHINE, FONTS);
    fonts.extend(value_names(HKEY_CURRENT_USER, FONTS));
    fonts
}

/// The names of the values in a registry key; none when it cannot be read.
fn value_names(root: HKEY, subkey: &str) -> Vec<String> {
    let subkey: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: the subkey is NUL-terminated and `key` receives the handle, closed below.
    let status = unsafe { RegOpenKeyExW(root, subkey.as_ptr(), 0, KEY_QUERY_VALUE, &raw mut key) };
    if status != ERROR_SUCCESS {
        return Vec::new();
    }
    let mut names = Vec::new();
    let mut buffer = [0u16; 512];
    for index in 0.. {
        let mut length = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
        // SAFETY: the key is open; `buffer` holds `length` UTF-16 units; the value's type
        // and data are not asked for.
        let status = unsafe {
            RegEnumValueW(
                key,
                index,
                buffer.as_mut_ptr(),
                &raw mut length,
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if status == ERROR_NO_MORE_ITEMS {
            break;
        }
        if status == ERROR_SUCCESS {
            let length = usize::try_from(length).unwrap_or_default();
            let name = buffer.get(..length).unwrap_or_default();
            names.push(String::from_utf16_lossy(name));
        }
    }
    // SAFETY: the key was opened above and is closed once.
    unsafe { RegCloseKey(key) };
    names
}

/// JSON with comments, as Terminal's settings and fragments are written: `//` and
/// `/* */` comments and trailing commas are allowed.
pub mod jsonc {
    /// What a token is.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum Kind {
        /// One of `{ } [ ] : ,`.
        Punct(u8),
        /// A string, quotes included.
        Str,
        /// A bare word: `null`, `true`, a number.
        Word,
    }

    /// One token and the bytes it spans. Whitespace and comments are no tokens.
    #[derive(Clone, Copy, Debug)]
    pub struct Token {
        /// What it is.
        pub kind: Kind,
        /// Where it starts, in bytes.
        pub start: usize,
        /// Just past its end, in bytes.
        pub end: usize,
    }

    /// The tokens of `text`, or `None` when a string or comment is left open.
    pub fn tokens(text: &str) -> Option<Vec<Token>> {
        let bytes = text.as_bytes();
        let mut out = Vec::new();
        let mut i = if text.starts_with('\u{feff}') { 3 } else { 0 };
        while let Some(&byte) = bytes.get(i) {
            match byte {
                b' ' | b'\t' | b'\r' | b'\n' => i += 1,
                b'/' if bytes.get(i + 1) == Some(&b'/') => {
                    while bytes.get(i).is_some_and(|&b| b != b'\n') {
                        i += 1;
                    }
                }
                b'/' if bytes.get(i + 1) == Some(&b'*') => {
                    let close = text.get(i + 2..)?.find("*/")?;
                    i += 2 + close + 2;
                }
                b'{' | b'}' | b'[' | b']' | b':' | b',' => {
                    out.push(Token {
                        kind: Kind::Punct(byte),
                        start: i,
                        end: i + 1,
                    });
                    i += 1;
                }
                b'"' => {
                    let start = i;
                    i += 1;
                    loop {
                        match *bytes.get(i)? {
                            b'\\' => i += 2,
                            b'"' => {
                                i += 1;
                                break;
                            }
                            _ => i += 1,
                        }
                    }
                    out.push(Token {
                        kind: Kind::Str,
                        start,
                        end: i,
                    });
                }
                _ => {
                    let start = i;
                    while bytes.get(i).is_some_and(|&b| {
                        !matches!(
                            b,
                            b' ' | b'\t'
                                | b'\r'
                                | b'\n'
                                | b'{'
                                | b'}'
                                | b'['
                                | b']'
                                | b':'
                                | b','
                                | b'"'
                                | b'/'
                        )
                    }) {
                        i += 1;
                    }
                    out.push(Token {
                        kind: Kind::Word,
                        start,
                        end: i,
                    });
                }
            }
        }
        Some(out)
    }

    /// A parsed value, with just what reading Terminal's settings needs.
    #[derive(Debug, PartialEq, Eq)]
    pub enum Value {
        /// An object's members, in order.
        Object(Vec<(String, Self)>),
        /// An array's items.
        Array(Vec<Self>),
        /// A string, unescaped.
        Str(String),
        /// `null`, `true`, `false` or a number.
        Other,
    }

    impl Value {
        /// An object's member.
        pub fn get(&self, key: &str) -> Option<&Self> {
            match self {
                Self::Object(members) => members
                    .iter()
                    .find(|(name, _)| name == key)
                    .map(|(_, value)| value),
                _ => None,
            }
        }

        /// A string's text.
        pub fn as_str(&self) -> Option<&str> {
            match self {
                Self::Str(text) => Some(text),
                _ => None,
            }
        }

        /// An array's items.
        pub fn as_array(&self) -> Option<&[Self]> {
            match self {
                Self::Array(items) => Some(items),
                _ => None,
            }
        }
    }

    /// `text` parsed, or `None` when it is not JSON with comments.
    pub fn parse(text: &str) -> Option<Value> {
        let tokens = tokens(text)?;
        let (value, next) = parse_at(text, &tokens, 0)?;
        (next == tokens.len()).then_some(value)
    }

    fn parse_at(text: &str, tokens: &[Token], i: usize) -> Option<(Value, usize)> {
        let token = tokens.get(i)?;
        match token.kind {
            Kind::Punct(b'{') => {
                let mut members = Vec::new();
                let mut j = i + 1;
                loop {
                    let member = tokens.get(j)?;
                    match member.kind {
                        Kind::Punct(b'}') => return Some((Value::Object(members), j + 1)),
                        Kind::Punct(b',') => j += 1,
                        Kind::Str => {
                            if tokens.get(j + 1)?.kind != Kind::Punct(b':') {
                                return None;
                            }
                            let (value, next) = parse_at(text, tokens, j + 2)?;
                            members.push((string(text, member)?, value));
                            j = next;
                        }
                        _ => return None,
                    }
                }
            }
            Kind::Punct(b'[') => {
                let mut items = Vec::new();
                let mut j = i + 1;
                loop {
                    match tokens.get(j)?.kind {
                        Kind::Punct(b']') => return Some((Value::Array(items), j + 1)),
                        Kind::Punct(b',') => j += 1,
                        _ => {
                            let (value, next) = parse_at(text, tokens, j)?;
                            items.push(value);
                            j = next;
                        }
                    }
                }
            }
            Kind::Str => Some((Value::Str(string(text, token)?), i + 1)),
            Kind::Word => Some((Value::Other, i + 1)),
            Kind::Punct(_) => None,
        }
    }

    /// A string token's text, its escapes undone.
    fn string(text: &str, token: &Token) -> Option<String> {
        let raw = text.get(token.start + 1..token.end.checked_sub(1)?)?;
        let mut out = String::with_capacity(raw.len());
        let mut chars = raw.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                'b' => out.push('\u{8}'),
                'f' => out.push('\u{c}'),
                'u' => {
                    let unit = |chars: &mut std::str::Chars<'_>| {
                        let hex: String = chars.by_ref().take(4).collect();
                        u16::from_str_radix(&hex, 16).ok()
                    };
                    let first = unit(&mut chars)?;
                    let mut units = vec![first];
                    if (0xd800..0xdc00).contains(&first) && chars.as_str().starts_with("\\u") {
                        chars.next();
                        chars.next();
                        units.push(unit(&mut chars)?);
                    }
                    out.push_str(&String::from_utf16_lossy(&units));
                }
                other => out.push(other),
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::jsonc::{Value, parse};
    use super::*;

    #[test]
    fn nerd_fonts_are_known_by_name() {
        for face in [
            "UbuntuSansMono Nerd Font Mono",
            "CaskaydiaMono Nerd Font",
            "Cascadia Mono NF",
            "JetBrainsMono NFM",
            "Hack NFP",
            "Cascadia Mono, Symbols Nerd Font Mono",
        ] {
            assert!(is_nerd_font(face), "{face}");
        }
        for face in [
            "Cascadia Mono",
            "Consolas",
            "Lucida Console",
            "NFL Sans",
            "",
        ] {
            assert!(!is_nerd_font(face), "{face}");
        }
    }

    #[test]
    fn comments_escapes_and_trailing_commas_parse() {
        let text = "\u{feff}{ // a comment\n \"a\": [1, \"x\\\"y\", ], /* b */ \"b\": { \"c\": \"\\u00e9\\ud83d\\ude00\" }, }";
        let value = parse(text).unwrap_or(Value::Other);
        assert_eq!(
            value
                .get("a")
                .and_then(Value::as_array)
                .and_then(|a| a.get(1))
                .and_then(Value::as_str),
            Some("x\"y")
        );
        assert_eq!(
            value
                .get("b")
                .and_then(|b| b.get("c"))
                .and_then(Value::as_str),
            Some("é😀")
        );
        assert_eq!(parse("{ \"a\": "), None);
        assert_eq!(parse("{} {}"), None);
    }

    /// A `LOCALAPPDATA` of its own, with the Store's `settings.json` and a fragment.
    fn install(name: &str, settings: &str, fragment: Option<&str>) -> PathBuf {
        let local = std::env::temp_dir().join(format!("cash-win32-terminal-{name}"));
        let _ = std::fs::remove_dir_all(&local);
        let state = local
            .join("Packages")
            .join("Microsoft.WindowsTerminal_8wekyb3d8bbwe")
            .join("LocalState");
        let _ = std::fs::create_dir_all(&state);
        let _ = std::fs::write(state.join("settings.json"), settings);
        if let Some(fragment) = fragment {
            let dir = local
                .join("Microsoft")
                .join("Windows Terminal")
                .join("Fragments")
                .join("app");
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(dir.join("app.json"), fragment);
        }
        local
    }

    const GUID: &str = "{43e4cdd3-eb67-5e13-bd17-fa0d7f8cf3ff}";

    #[test]
    fn a_profile_font_comes_from_its_entry_then_defaults_then_its_fragment() {
        let fragment = format!(
            r#"{{ "profiles": [ {{ "guid": "{GUID}", "name": "cash", "font": {{ "face": "CaskaydiaMono Nerd Font Mono" }} }} ] }}"#
        );

        // The user's own entry wins.
        let own = format!(
            r#"{{ "profiles": {{ "defaults": {{ "font": {{ "face": "Consolas" }} }}, "list": [ {{ "guid": "{}", "fontFace": "Hack NF" }} ] }} }}"#,
            GUID.to_uppercase()
        );
        let local = install("own", &own, Some(&fragment));
        assert_eq!(profile_font(&local, None, GUID), "Hack NF");

        // Then profiles.defaults, over the fragment.
        let defaults = format!(
            r#"{{ "profiles": {{ "defaults": {{ "font": {{ "face": "Consolas" }} }}, "list": [ {{ "guid": "{GUID}", "source": "cash" }} ] }} }}"#
        );
        let local = install("defaults", &defaults, Some(&fragment));
        assert_eq!(profile_font(&local, None, GUID), "Consolas");

        // Then the fragment.
        let stub = format!(r#"{{ "profiles": {{ "list": [ {{ "guid": "{GUID}" }} ] }} }}"#);
        let local = install("fragment", &stub, Some(&fragment));
        assert_eq!(
            profile_font(&local, None, GUID),
            "CaskaydiaMono Nerd Font Mono"
        );

        // Then Terminal's own.
        let local = install("none", &stub, None);
        assert_eq!(profile_font(&local, None, GUID), DEFAULT_FONT);

        for name in ["own", "defaults", "fragment", "none"] {
            let _ = std::fs::remove_dir_all(
                std::env::temp_dir().join(format!("cash-win32-terminal-{name}")),
            );
        }
    }

    #[test]
    fn the_older_layout_with_profiles_as_a_list_is_read() {
        let old = format!(
            r#"{{ "profiles": [ {{ "guid": "{GUID}", "fontFace": "Cascadia Mono NF" }} ] }}"#
        );
        let local = install("old", &old, None);
        assert_eq!(profile_font(&local, None, GUID), "Cascadia Mono NF");
        let _ = std::fs::remove_dir_all(&local);
    }
}
