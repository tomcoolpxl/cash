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

/// The Nerd Fonts the user already has Terminal draw in, first seen first: each install's
/// default profile's font, its `profiles.defaults`, then each profile's own.
pub fn nerd_fonts_in_use(local_app_data: &Path, program_data: Option<&Path>) -> Vec<String> {
    let mut fonts: Vec<String> = Vec::new();
    for file in settings_files(local_app_data) {
        let Some(settings) = std::fs::read_to_string(&file)
            .ok()
            .and_then(|text| jsonc::parse(&text))
        else {
            continue;
        };
        if let Some(default) = settings.get("defaultProfile").and_then(Value::as_str) {
            fonts.push(profile_font(local_app_data, program_data, default));
        }
        let defaults = settings
            .get("profiles")
            .and_then(|profiles| profiles.get("defaults"));
        fonts.extend(defaults.and_then(face_of));
        fonts.extend(user_profiles(&settings).filter_map(face_of));
    }
    let mut seen = std::collections::HashSet::new();
    fonts.retain(|font| is_nerd_font(font) && seen.insert(font.to_ascii_lowercase()));
    fonts
}

/// The name Terminal knows a Nerd Font family by, from a name Windows lists it under.
///
/// With its kind: 0 for `Nerd Font Mono`, whose icons keep to one cell, 1 for `Nerd
/// Font`, 2 for `Nerd Font Propo`. Nerd Fonts 3 list short names (`UbuntuSansMono NFM
/// Bold`) and Terminal wants the long one (`UbuntuSansMono Nerd Font Mono`); older ones
/// list the long name itself. `None` for any other font, for Symbols Nerd Font, which has
/// icons but no letters, and for Microsoft's own `Cascadia … NF`, whose short name is its
/// name.
pub fn terminal_family(listed: &str) -> Option<(u8, String)> {
    let name = listed
        .strip_suffix(" (TrueType)")
        .or_else(|| listed.strip_suffix(" (OpenType)"))
        .unwrap_or(listed)
        .trim();
    let words: Vec<&str> = name.split_whitespace().collect();
    let short = words.iter().enumerate().find_map(|(at, word)| {
        let kind = match *word {
            "NFM" => (0, "Nerd Font Mono"),
            "NF" => (1, "Nerd Font"),
            "NFP" => (2, "Nerd Font Propo"),
            _ => return None,
        };
        Some((at, kind))
    });
    let (at, (kind, long)) = if let Some(found) = short {
        found
    } else {
        let at = words.windows(2).position(|pair| pair == ["Nerd", "Font"])?;
        let kind = match words.get(at + 2) {
            Some(&"Mono") => (0, "Nerd Font Mono"),
            Some(&"Propo") => (2, "Nerd Font Propo"),
            Some(&"Complete") => return None,
            _ => (1, "Nerd Font"),
        };
        (at, kind)
    };
    let base = words.get(..at)?.join(" ");
    if base.is_empty() || base.starts_with("Symbols") || base.starts_with("Cascadia ") {
        return None;
    }
    Some((kind, format!("{base} {long}")))
}

/// The font families Windows draws with a fixed pitch, as GDI names them.
///
/// `UbuntuSansMono NFM`, `Hack Nerd Font Mono`, `Consolas`. With [`FONTS_KEY_VAR`] set,
/// that key's value names instead, as for [`installed_fonts`].
pub fn monospace_families() -> Vec<String> {
    use windows_sys::Win32::Foundation::LPARAM;
    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DEFAULT_CHARSET, DeleteDC, EnumFontFamiliesExW, LOGFONTW,
    };

    if std::env::var_os(FONTS_KEY_VAR).is_some() {
        return installed_fonts();
    }
    let mut families: Vec<String> = Vec::new();
    // SAFETY: a memory device context, which needs no screen; deleted below.
    let dc = unsafe { CreateCompatibleDC(std::ptr::null_mut()) };
    if dc.is_null() {
        return families;
    }
    // SAFETY: LOGFONTW is plain data and valid all zeros: an empty face name with the
    // default character set asks for every family, once each.
    let mut wanted: LOGFONTW = unsafe { std::mem::zeroed() };
    wanted.lfCharSet = DEFAULT_CHARSET;
    // SAFETY: `add_fixed_pitch` runs only during this call, and `families` outlives it.
    unsafe {
        EnumFontFamiliesExW(
            dc,
            &raw const wanted,
            Some(add_fixed_pitch),
            (&raw mut families) as LPARAM,
            0,
        );
    }
    // SAFETY: the context came from CreateCompatibleDC above and is deleted once.
    unsafe { DeleteDC(dc) };
    families.sort();
    families.dedup();
    families
}

/// `EnumFontFamiliesExW` callback: keeps a family whose pitch is fixed.
unsafe extern "system" fn add_fixed_pitch(
    font: *const windows_sys::Win32::Graphics::Gdi::LOGFONTW,
    _metrics: *const windows_sys::Win32::Graphics::Gdi::TEXTMETRICW,
    _kind: u32,
    families: windows_sys::Win32::Foundation::LPARAM,
) -> i32 {
    // SAFETY: `families` is the Vec `monospace_families` passed, alive for the call.
    let families = unsafe { &mut *(families as *mut Vec<String>) };
    // SAFETY: GDI hands the callback a valid LOGFONTW for the length of the call.
    let font = unsafe { &*font };
    let fixed = font.lfPitchAndFamily & 0x3 == 1;
    let name = font
        .lfFaceName
        .split(|&unit| unit == 0)
        .next()
        .unwrap_or(&[]);
    // `@` names are the same fonts turned for vertical writing.
    if fixed && name.first() != Some(&u16::from(b'@')) {
        families.push(String::from_utf16_lossy(name));
    }
    1
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
    fn a_listed_font_maps_to_the_family_terminal_knows() {
        let cases = [
            (
                "UbuntuSansMono NFM Bold (TrueType)",
                Some((0, "UbuntuSansMono Nerd Font Mono")),
            ),
            (
                "UbuntuSansMono NFM",
                Some((0, "UbuntuSansMono Nerd Font Mono")),
            ),
            (
                "JetBrainsMonoNL NF SemiBold",
                Some((1, "JetBrainsMonoNL Nerd Font")),
            ),
            (
                "NotoSans NFP Cond ExtBd",
                Some((2, "NotoSans Nerd Font Propo")),
            ),
            (
                "Hack Nerd Font Mono Regular (TrueType)",
                Some((0, "Hack Nerd Font Mono")),
            ),
            (
                "RobotoMono Nerd Font Mono Th It",
                Some((0, "RobotoMono Nerd Font Mono")),
            ),
            (
                "Inconsolata LGC Nerd Font",
                Some((1, "Inconsolata LGC Nerd Font")),
            ),
            (
                "FiraCode Nerd Font Propo Reg",
                Some((2, "FiraCode Nerd Font Propo")),
            ),
        ];
        for (listed, expected) in cases {
            let expected = expected.map(|(kind, name)| (kind, name.to_owned()));
            assert_eq!(terminal_family(listed), expected, "{listed}");
        }
        for listed in [
            "Cascadia Mono Regular (TrueType)",
            "Symbols Nerd Font Mono",
            "Cascadia Mono NF SemiBold (TrueType)",
            "Consolas Nerd Font Complete Mono Windows Compatible",
            "NFM",
        ] {
            assert_eq!(terminal_family(listed), None, "{listed}");
        }
    }

    #[test]
    fn nerd_fonts_in_use_start_with_the_default_profile() {
        let settings = format!(
            r#"{{ "defaultProfile": "{GUID}", "profiles": {{ "list": [
                {{ "guid": "{{aaaa}}", "font": {{ "face": "Hack Nerd Font Mono" }} }},
                {{ "guid": "{{bbbb}}", "font": {{ "face": "Consolas" }} }},
                {{ "guid": "{GUID}", "font": {{ "face": "UbuntuSansMono Nerd Font Mono" }} }} ] }} }}"#
        );
        let local = install("in-use", &settings, None);
        assert_eq!(
            nerd_fonts_in_use(&local, None),
            ["UbuntuSansMono Nerd Font Mono", "Hack Nerd Font Mono"]
        );
        let _ = std::fs::remove_dir_all(&local);
    }

    #[test]
    fn this_machine_reports_consolas_as_fixed_pitch() {
        // Every Windows has Consolas; GDI says its pitch is fixed.
        assert!(
            monospace_families()
                .iter()
                .any(|family| family == "Consolas"),
            "{:?}",
            monospace_families()
        );
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
