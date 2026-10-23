//! Named gradients for `--gradient`, and the colour lists it takes.
//!
//! The look of the "giant gradient logo" that tools such as oh-my-logo and cfonts draw:
//! a few stops in true colour, interpolated across the text. The names and colours are
//! cash's own.

use crate::style::Color;

/// A colour as red, green and blue.
pub type Rgb = (u8, u8, u8);

/// Every named gradient, with its stops in order.
pub const PALETTES: &[(&str, &[Rgb])] = &[
    ("sunset", &[(255, 94, 98), (255, 153, 102), (255, 209, 148)]),
    ("ocean", &[(0, 198, 255), (0, 114, 255), (0, 31, 92)]),
    ("fire", &[(255, 208, 0), (255, 106, 0), (209, 0, 0)]),
    ("forest", &[(168, 224, 99), (86, 171, 47), (19, 78, 23)]),
    ("matrix", &[(0, 255, 65), (0, 143, 17), (0, 59, 0)]),
    ("neon", &[(255, 0, 204), (51, 51, 255), (0, 255, 255)]),
    (
        "pastel",
        &[(255, 209, 220), (205, 180, 219), (162, 210, 255)],
    ),
    ("aurora", &[(0, 255, 163), (3, 225, 255), (220, 31, 255)]),
    (
        "candy",
        &[(255, 110, 199), (255, 211, 110), (110, 243, 255)],
    ),
    ("ice", &[(224, 247, 255), (127, 219, 255), (0, 116, 217)]),
    ("gold", &[(255, 241, 168), (255, 204, 51), (184, 134, 11)]),
    (
        "vapor",
        &[
            (255, 113, 206),
            (1, 205, 254),
            (5, 255, 161),
            (185, 103, 255),
        ],
    ),
    ("mono", &[(255, 255, 255), (119, 119, 119)]),
    (
        "rainbow",
        &[
            (255, 0, 0),
            (255, 128, 0),
            (255, 255, 0),
            (0, 255, 0),
            (0, 255, 255),
            (0, 0, 255),
            (128, 0, 255),
        ],
    ),
];

/// The stops `spec` names: a palette's name, or colours separated by commas
/// (`red,#00ff00,blue`), one colour making a flat fill. The part that is not a colour
/// when one is not.
pub fn stops(spec: &str) -> Result<Vec<(u8, u8, u8)>, String> {
    if let Some((_, stops)) = PALETTES
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(spec.trim()))
    {
        return Ok(stops.to_vec());
    }
    spec.split(',')
        .map(|part| {
            Color::parse(part)
                .map(Color::rgb)
                .ok_or_else(|| part.trim().to_owned())
        })
        .collect()
}

/// The colour a fraction `t` (0 to 1) of the way along `stops`.
pub fn at(stops: &[(u8, u8, u8)], t: f64) -> (u8, u8, u8) {
    let (Some(&first), Some(&last)) = (stops.first(), stops.last()) else {
        return (255, 255, 255);
    };
    if stops.len() == 1 || t <= 0.0 {
        return first;
    }
    if t >= 1.0 {
        return last;
    }
    let spans = u32::try_from(stops.len() - 1).unwrap_or(1);
    let scaled = t * f64::from(spans);
    let index = whole(scaled.floor());
    let within = scaled - scaled.floor();
    let from = stops.get(index).copied().unwrap_or(first);
    let to = stops.get(index + 1).copied().unwrap_or(last);
    let mix = |a: u8, b: u8| channel((f64::from(b) - f64::from(a)).mul_add(within, f64::from(a)));
    (mix(from.0, to.0), mix(from.1, to.1), mix(from.2, to.2))
}

/// A colour channel from a float, rounded and held to 0 to 255.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped to 0..=255 first"
)]
pub const fn channel(value: f64) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

/// A small non-negative whole number from a float, for an index.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "held to 0..=65535 first"
)]
const fn whole(value: f64) -> usize {
    value.clamp(0.0, 65535.0) as usize
}

#[cfg(test)]
mod tests {
    use super::{at, channel, stops};

    #[test]
    fn names_and_lists() {
        assert_eq!(stops("Ocean").unwrap().len(), 3);
        assert_eq!(stops("red, #00ff00").unwrap(), [(197, 15, 31), (0, 255, 0)]);
        assert_eq!(stops("red,nope"), Err("nope".to_owned()));
    }

    #[test]
    fn interpolation() {
        let black_white = [(0, 0, 0), (255, 255, 255)];
        assert_eq!(at(&black_white, 0.0), (0, 0, 0));
        assert_eq!(at(&black_white, 0.5), (128, 128, 128));
        assert_eq!(at(&black_white, 1.0), (255, 255, 255));
        let three = [(0, 0, 0), (100, 0, 0), (100, 100, 0)];
        assert_eq!(at(&three, 0.75), (100, 50, 0));
        assert_eq!(channel(-3.0), 0);
        assert_eq!(channel(300.0), 255);
    }
}
