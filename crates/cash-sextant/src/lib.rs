//! sextant: a shooter drawn in the terminal with text, cash's `sextant` (D88).
//!
//! A canvas of pixels, two wide and three high to a cell, shown with the 64 patterns of a
//! 2x3 cell in 24-bit colour: from a DEC soft font that the game sends the terminal, so
//! no font needs anything, or from the font's Unicode sextants. On it, a ship that steers
//! with held keys (the Windows console reports releases), a formation that sways and
//! sends enemies down in dives, explosions and stars. Begun as a prototype of its own and
//! taken in whole.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::suboptimal_flops,
    reason = "a game's pixel and time arithmetic moves between floats and small integers \
              that fit by construction (canvas sizes, sprite offsets, colour bytes), and \
              reads as motion, not as fused multiply-adds"
)]

mod canvas;
mod game;
mod input;
mod screen;
mod soft_font;
mod sprites;

use std::fmt::Write as _;
use std::io::{self, Write};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyModifiers};

use game::Game;
use input::Keys;
use screen::Screen;

/// How to play.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// The game plays by itself.
    pub demo: bool,
    /// Draw with a soft font sent to the terminal, not the font's Unicode sextants.
    pub soft_font: bool,
    /// Frames a second.
    pub fps: u32,
}

/// How a game ended.
#[derive(Clone, Copy, Debug)]
pub struct Outcome {
    /// The score.
    pub score: u32,
    /// The wave reached.
    pub wave: u32,
}

/// The terminal as the game needs it, given back when dropped: raw keys, the alternate
/// screen, no cursor, no line wrap, and ASCII as the character set again.
struct Terminal<'a, W: Write> {
    out: &'a mut W,
    raw: bool,
}

impl<'a, W: Write> Terminal<'a, W> {
    fn enter(out: &'a mut W) -> io::Result<Self> {
        let raw = crossterm::terminal::enable_raw_mode().is_ok();
        out.write_all(b"\x1b[?1049h\x1b[?25l\x1b[?7l\x1b[2J")?;
        out.flush()?;
        Ok(Self { out, raw })
    }
}

impl<W: Write> Drop for Terminal<'_, W> {
    fn drop(&mut self) {
        let _ = write!(
            self.out,
            "{}\x1b[0m\x1b[?7h\x1b[?25h\x1b[?1049l",
            soft_font::DESELECT
        );
        let _ = self.out.flush();
        if self.raw {
            let _ = crossterm::terminal::disable_raw_mode();
        }
        // The releases of the last keys, and anything typed, are the game's, not the
        // prompt's.
        while event::poll(Duration::ZERO).unwrap_or(false) {
            let _ = event::read();
        }
    }
}

/// Play on the terminal that `out` writes to, until the player quits (`q`, Esc, Ctrl-C).
pub fn play(out: &mut impl Write, options: Options) -> io::Result<Outcome> {
    let terminal = Terminal::enter(out)?;
    let out = &mut *terminal.out;
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let mut screen = Screen::new(cols.into(), rows.into(), options.soft_font);
    let mut canvas = screen.canvas();
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(7, |d| d.as_nanos() as u64);
    let mut game = Game::new(canvas.width, canvas.height, seed);
    let mut keys = Keys::default();
    let frame = Duration::from_secs_f64(1.0 / f64::from(options.fps.clamp(1, 240)));
    let mut last = Instant::now();
    let mut next_frame = last;

    loop {
        let now = Instant::now();
        while event::poll(Duration::ZERO)? {
            match event::read()? {
                Event::Key(key) => {
                    let quit = matches!(key.code, KeyCode::Esc | KeyCode::Char('q' | 'Q'))
                        || (key.code == KeyCode::Char('c')
                            && key.modifiers.contains(KeyModifiers::CONTROL));
                    if quit {
                        return Ok(Outcome {
                            score: game.score,
                            wave: game.wave,
                        });
                    }
                    keys.handle(key, now);
                }
                Event::Resize(cols, rows) => {
                    screen = Screen::new(cols.into(), rows.into(), options.soft_font);
                    canvas = screen.canvas();
                    game.resize(canvas.width, canvas.height);
                    out.write_all(b"\x1b[2J")?;
                }
                _ => {}
            }
        }
        keys.expire(now);
        if keys.take_pressed().contains(&KeyCode::Enter) && game.over {
            game.restart();
        }
        if options.demo && game.over {
            game.restart();
        }

        let dt = now.duration_since(last).as_secs_f32().min(0.05);
        last = now;
        game.update(dt, &keys, now, options.demo);
        canvas.clear();
        game.draw(&mut canvas);
        screen.set_text(status(&game, options.demo));
        screen.present(&canvas, out)?;

        next_frame += frame;
        let now = Instant::now();
        if next_frame > now {
            std::thread::sleep(next_frame - now);
        } else {
            // Behind: start the count again rather than run frames back to back.
            next_frame = now;
        }
    }
}

/// The top line.
fn status(game: &Game, demo: bool) -> String {
    let lives = "\u{2665}".repeat(game.lives as usize);
    let state = if game.over {
        "   GAME OVER: Enter plays again"
    } else if demo {
        "   demo"
    } else {
        ""
    };
    format!(
        " SEXTANT   score {:06}   wave {}   {lives:<3}{state}      \u{2190}\u{2192} move   space fire   q quit",
        game.score, game.wave
    )
}

/// The 64 patterns of a cell, each from the font's Unicode sextants and then from the
/// soft font, lit pixels yellow on blue so the cell's edges show.
pub fn glyph_table() -> String {
    let cell = |glyph: String| format!("\x1b[33;44m{glyph}\x1b[0m");
    let mut table = soft_font::load();
    table.push_str(
        "The 64 patterns of a cell, 2x3 pixels; the number's bits are the lit pixels,\n\
         bit 0 the top left, in reading order. Beside each number: the pattern from\n\
         the terminal's font (Unicode's sextants), then from the soft font.\n\n",
    );
    for row in 0..8u8 {
        table.push_str("  ");
        for col in 0..8u8 {
            let mask = row * 8 + col;
            let soft = format!(
                "{}{}{}",
                soft_font::SELECT,
                soft_font::character(mask),
                soft_font::DESELECT
            );
            let _ = write!(
                table,
                "{mask:2} {} {}   ",
                cell(canvas::glyph(mask).to_string()),
                cell(soft)
            );
        }
        table.push_str("\n\n");
    }
    table.push_str(
        "A question mark in a diamond or a box: the font has no sextants (play with the\n\
         soft font, the default). A plain character (! \" # ...) in the second place:\n\
         the terminal has no soft fonts (play with --unicode and a font that has them).\n",
    );
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ten seconds of the demo on a 120x40 terminal, written to memory: a frame costs a
    /// few kilobytes, either way of drawing.
    #[test]
    fn a_frame_costs_a_few_kilobytes() {
        for soft_font in [false, true] {
            let mut screen = Screen::new(120, 40, soft_font);
            let mut canvas = screen.canvas();
            let mut game = Game::new(canvas.width, canvas.height, 42);
            let keys = Keys::default();
            let now = Instant::now();
            let mut total = 0;
            let mut sink = Vec::new();
            for _ in 0..600 {
                game.update(1.0 / 60.0, &keys, now, true);
                if game.over {
                    game.restart();
                }
                canvas.clear();
                game.draw(&mut canvas);
                sink.clear();
                total += screen
                    .present(&canvas, &mut sink)
                    .unwrap_or(usize::MAX / 1000);
            }
            assert!(total / 600 < 8 * 1024, "{soft_font}: {}", total / 600);
        }
    }

    #[test]
    fn the_glyph_table_loads_the_soft_font_first() {
        let table = glyph_table();
        assert!(table.starts_with("\x1bP0;1;0;10;0;2;16;0{ @"));
        assert!(table.contains('\u{1FB00}'));
    }
}
