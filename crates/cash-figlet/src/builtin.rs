//! The fonts inside cash.exe (decisions.md D84).
//!
//! figlet 2.2.5's 17 fonts without `mnemonic` (BSD-3-Clause) and toilet's six most used
//! (WTFPL), 443 KB as the upstreams ship them, `mono12` and `mono9` zipped. Chosen from
//! about 600 `figlet` and 140 `toilet` calls in GitHub code (research/figlet-design.md,
//! section 6.5); any other font is read from a font directory.

/// A built-in font: its name and its file's bytes.
#[derive(Clone, Copy, Debug)]
pub struct BuiltinFont {
    /// The name `-f` takes, without the suffix.
    pub name: &'static str,
    /// `flf` or `tlf`.
    pub suffix: &'static str,
    /// The file as the upstream ships it.
    pub bytes: &'static [u8],
}

macro_rules! fonts {
    ($($name:literal . $suffix:literal),* $(,)?) => {
        &[$(BuiltinFont {
            name: $name,
            suffix: $suffix,
            bytes: include_bytes!(concat!("../fonts/", $name, ".", $suffix)),
        }),*]
    };
}

/// Every built-in font, by name.
pub const FONTS: &[BuiltinFont] = fonts![
    "banner"."flf",
    "big"."flf",
    "block"."flf",
    "bubble"."flf",
    "digital"."flf",
    "future"."tlf",
    "ivrit"."flf",
    "lean"."flf",
    "mini"."flf",
    "mono12"."tlf",
    "mono9"."tlf",
    "pagga"."tlf",
    "script"."flf",
    "shadow"."flf",
    "slant"."flf",
    "small"."flf",
    "smblock"."tlf",
    "smbraille"."tlf",
    "smscript"."flf",
    "smshadow"."flf",
    "smslant"."flf",
    "standard"."flf",
    "term"."flf",
];

/// The built-in font called `name`, given with or without its suffix.
pub fn find(name: &str) -> Option<&'static BuiltinFont> {
    let bare = name
        .strip_suffix(".flf")
        .or_else(|| name.strip_suffix(".tlf"))
        .unwrap_or(name);
    FONTS.iter().find(|font| font.name == bare)
}

#[cfg(test)]
mod tests {
    use super::{FONTS, find};
    use crate::font::Font;

    #[test]
    fn every_built_in_font_reads() {
        for font in FONTS {
            let parsed = Font::parse(font.bytes)
                .unwrap_or_else(|e| panic!("{}.{}: {e}", font.name, font.suffix));
            assert!(parsed.glyph(u32::from('A')).is_some(), "{}", font.name);
        }
        assert_eq!(FONTS.len(), 23);
    }

    #[test]
    fn found_with_or_without_suffix() {
        assert_eq!(find("slant").unwrap().name, "slant");
        assert_eq!(find("mono9.tlf").unwrap().suffix, "tlf");
        assert!(find("mnemonic").is_none());
    }
}
