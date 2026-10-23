# Big letters: figlet and toilet, one design

Status: **decided by the user on 2026-10-23** (section 9), nothing built. The user asked
for "an enhanced figlet": everything figlet and toilet can do, plus extras, written
in-house in Rust, with compatibility that "must not be super strict".

## 1. The family

| Tool | What it is | Licence | On Windows today | In WSL |
| --- | --- | --- | --- | --- |
| `figlet` 2.2.5 | the original (1991); big letters from `.flf` fonts, smushing, wrapping, justification, control files | BSD-3-Clause | Scoop main has a Go port (`lukesampson/figlet`, `1.0-go`) | not installed |
| `toilet` 0.3 | Sam Hocevar's figlet on libcaca: reads `.flf` and `.tlf`, UTF-8, colour fonts, filters, export formats | WTFPL | nothing | not installed |
| `figlist`, `showfigfonts`, `chkfont` | figlet's helpers: list fonts and control files, show each font, check a font file | BSD-3-Clause | nothing | not installed |

Debian's popularity contest (2026-10-21): `toilet` 10,301 installs and 779 regular users,
`figlet` 4,502 and 1,068, `sysvbanner` 439 and 62. figlet is the one people run; toilet
is installed more often.

## 2. The standard: one font format, three flavours

There is one standard, **FIGfont 2.0** (`figfont.txt`, by Cowan and Burton). toilet did not
invent a new one; it reads the same layout under another signature.

| File | First line | Text | What is different |
| --- | --- | --- | --- |
| `.flf` | `flf2a$ 6 5 16 15 11 0 24463` | bytes; figlet 2.2's own fonts are ASCII, many fonts in the wild are Latin-1 | the reference |
| `.tlf` | `tlf2a$ 4 3 8 0 16 0 64 0` | UTF-8 | glyphs may hold ANSI colour escapes: toilet's `biggray9` and `biggray12` have 1,500 of them, so they are **colour fonts**; often no hardblank |
| `.flc` | `flc2a` | text | a control file: maps input characters to font characters (`t` ranges, `f` freeze, ISO 2022 `g` sets, `j` JIS); figlet ships 30 (`utf8.flc`, `8859-2.flc`, `koi8r.flc`, `646-de.flc` ...) |

Any of the three may be a zip file holding one member (`PK\3\4`); half of toilet's fonts
are stored that way. The header carries height, baseline, maximum width, the old layout
(-1, 0, or smushing rules 1 to 63), comment line count, print direction, the full layout
(rules, kerning, smushing, and vertical bits) and the number of code-tagged characters.
After the 102 required characters (ASCII 32 to 126 and seven German letters) come code
tags in decimal, octal or hex; negative codes are allowed and skipped.

Findings that matter for the parser:

- figlet strips **every** repeated end mark at the end of a glyph line, not one or two.
- A missing character falls back to character 0 when the font defines one.
- The vertical layout bits (8192 and up) are in the spec, but neither figlet nor toilet
  smushes vertically; figlet 2.2.5 reads and ignores them.
- libcaca's loader (`caca/figfont.c`) imports glyph lines with its UTF-8 importer, which
  understands ANSI SGR escapes: that is how colour fonts work. It also adds an
  **overlap** mode figlet lacks (glyphs overlap the previous one by one column).

## 3. What the two tools do, together

| Area | figlet | toilet | Clash |
| --- | --- | --- | --- |
| Font | `-f name\|file`, `-d dir`, `FIGLET_FONTDIR` | `-f`, `-d`, built-in font `term` (the text as it is) | none |
| Layout | `-s` font default, `-S` smush, `-k` kern, `-W` full width, `-o` overlap (smush any pair), `-m n` raw layout | `-s`, `-S`, `-k`, `-W`, `-o` (libcaca's overlap) | `-o` means different things; both are "tighter than kerning" |
| Width | `-w n`, `-t` terminal width, default 80 | `-w n`, `-t`, default 80 | none |
| Justification | `-c` `-l` `-r` `-x` | none | none |
| Paragraphs | `-p` paragraph mode, `-n` normal | none (a newline in the input starts a new line) | none |
| Direction | `-L` `-R` `-X` | follows the font | none |
| Characters | `-C file` control files, `-N` clears them, `-D`/`-E` German translation on and off | UTF-8 always | **`-E`**: figlet's "German off", toilet's export |
| Colour | none | `-F list`, `-F a:b:c`, `--metal`, `--rainbow` (`--gay` is an old alias) | none |
| Filters | none | `crop`, `rainbow`, `metal`, `flip`, `flop`, `180` (`rotate`), `left`, `right`, `border` | none |
| Output | text | `-E list`, `-E fmt`, `--html`, `--irc`; libcaca's formats: `utf8` (default), `utf8cr`, `ansi` (CP437), `html`, `html3`, `irc`, `bbfr`, `svg`, `ps`, `tga`, `troff`, `caca` | none |
| Info | `-I 0..5` (version, font directory, font, width, formats), `-v` | `-I` the same, `-v` | none |

Only `-E` clashes; the rest is a union. cash has one command, `figlet`, so `-E` is
toilet's export (section 6.6).

## 4. Prior work, and what can be taken

| Project | Language, licence | What it gives | Take |
| --- | --- | --- | --- |
| **rs-figlet** (`figlet-rs` 1.0.0, 2026-03) | Rust, Apache-2.0 | a close port of figlet's smushing (all six rules, kerning, full width, old layout, right-to-left), code tags, `.tlf` signature, zipped fonts; golden tests against real figlet and toilet output | **copy the parser and smusher** (about 700 lines) into cash, NOTICE and licence kept as for sevenz-rust2; fix Latin-1 fonts, column width, end marks, character 0; its fixtures become tests |
| figlet 2.2.5 (`figlet.c`) | C, BSD-3 | the reference: wrapping, justification, paragraphs, control files, `-I` | read as the spec |
| libcaca `figfont.c`, toilet `src/` | C, WTFPL | `.tlf` and colour glyphs, overlap mode, the filters (each is 10 to 30 lines on a canvas) | read freely, reimplement on our canvas |
| cfonts (Rust and Node) | GPL-3.0 | gradients with colour stops, `--transition`, alignment incl. top and bottom, letter spacing, line height, background colour, 16/256/true colour detection | **ideas only**, no code (GPL) |
| oh-my-logo | Node | the "giant gradient logo" look of Claude Code and Gemini CLI: 13 named palettes, vertical, horizontal and diagonal gradients, filled block mode with shadow styles, reversed gradients | ideas only |
| lolcat, lolcrab | Ruby BSD-3, Rust MIT | rainbow over any text, frequency and spread, noise-based colour fields | ideas; lolcrab could be read |
| tui-big-text | Rust, Apache-2.0 | 8x8 pixel fonts drawn with half blocks, quadrants, sextants and octants (one cell = up to 2x4 pixels) | the idea for pixel fonts (section 7) |
| tdfiglet, tdfgo, retrofont | C, Go, Rust | TheDraw (`.tdf`) fonts: colour ANSI fonts from DOS, about 1,200 known | an optional reader; fonts not bundled (licences unknown) |
| xero/figlet-fonts | fonts | 1,600-star collection of `.flf`/`.tlf` | **not bundled**: no licence; users can point `-d` at a clone |

## 5. Windows Terminal: what helps

| Feature | State | Use |
| --- | --- | --- |
| 24-bit colour | yes (`WT_SESSION` set; conhost too since Windows 10) | gradients and palettes in true colour, cut down to 256 or 16 elsewhere |
| Built-in glyphs (`builtinGlyphs`, default on, Terminal PR 16729, 2024) | Terminal draws box drawing, block elements and Powerline glyphs itself, so they join with no gaps at any font size; Cascadia Code 2404 has all quadrants, sextants and the 256 Unicode 16 octants | block, shade and pixel fonts look solid in Terminal; whether Terminal draws the octants itself or leaves them to the font is unverified, so octant mode should be tried in the user's profile before it becomes a default |
| DEC double-size lines (`ESC #3`/`#4` double height, `ESC #6` double width) | implemented; open bugs on wrapping and resetting (issues 15681, 15575, 19682) | a "real font, twice the size" mode needing no font file at all: smooth Cascadia letters at 2x2 |
| Sixel | from Terminal 1.22; `coolfetch` already draws one | an image mode: render any installed Windows font (GDI or DirectWrite) to a bitmap and send it as sixel, true typography at any size |
| Kitty text sizing (OSC 66) | kitty only; Terminal has no issue for it | nothing to use |
| Width | the console's columns | `-t`, and the default when the output is a terminal |

## 6. Design

### 6.1 Layers

```
cash-builtins: figlet, figlist, showfigfonts; options, font search, colour decision
        |
cash-figlet (new leaf crate, depends on no cash crate)
  font      .flf .tlf parse, Latin-1 or UTF-8, code tags, colour escapes in glyphs
  control   .flc: translation table, freeze, ISO 2022 / JIS modes
  layout    the smusher (from rs-figlet), wrapping, justification, paragraphs, direction
  canvas    cells: character, foreground, background, bold/underline; width by unicode-width
  filter    crop rainbow metal flip flop 180 left right border, and the extras
  export    utf8 ansi text
  pixel     sub-cell block drawing of a bitmap (8x8 fonts, rasterised Windows fonts)
  tdf       TheDraw .tdf colour fonts, onto the same canvas
```

Fonts arrive as bytes; the front end unzips a zipped font with cash-archive's zip reader,
so the leaf crate needs no zip dependency (rs-figlet's `zip 0.6` goes).

### 6.2 Everything renders into a canvas

libcaca's model, which is why toilet's filters are short: the text becomes a grid of
cells, filters rewrite the grid, an exporter writes it out. figlet's plain output is the
`text` exporter on an uncoloured canvas. A colour font fills cell colours as it is
parsed; a filter may overwrite them; the exporter decides how to say them.

### 6.3 Unicode

Input is UTF-8 (cash's arguments are already Unicode). Width comes from `unicode-width`
so a wide glyph (katakana in `smblock`, emoji) takes two columns in smushing, wrapping and
borders. A character with no glyph falls back to character 0, then to a blank, never to a
crash. Control files keep working for the fonts that need them.

### 6.4 Colour

- One rule decides colour: `--color=auto|always|never`, `auto` meaning the shell's own
  test (a terminal, and `NO_COLOR` unset or empty). toilet colours pipes unconditionally;
  under loose compatibility cash does not.
- Depth: true colour in Windows Terminal and conhost, 256 or 16 when asked
  (`--colors=16|256|true`) or when `COLORTERM`/`TERM` say less. toilet's `rainbow` and
  `metal` keep their 16-colour palettes so they look as they always did.

### 6.5 Fonts

Search order: `-f path` as given; `-d dir`; `FIGLET_FONTDIR`; a user font directory
(`%LOCALAPPDATA%\cash\fonts`); the built-in set inside cash.exe. `.flf`, `.tlf` and
`.flc` are all looked for, each possibly zipped.

What the upstreams ship, all as files in a font directory, none inside the program
(toilet's `term` is code, not a file):

| Set | Files | Size as stored | Licence |
| --- | --- | --- | --- |
| figlet 2.2.5 fonts | 18 `.flf`: banner, big, block, bubble, digital, ivrit, lean, mini, mnemonic, script, shadow, slant, small, smscript, smshadow, smslant, standard, term | 379 KB | BSD-3 |
| figlet control files | 30 `.flc` | 198 KB | BSD-3 |
| toilet fonts | 26 `.tlf` (future, pagga, smblock, emboss, circle, letter, mono9/12, biggray ...) | 982 KB (10.5 MB unzipped) | WTFPL |

What scripts use, from about 600 `figlet` and 140 `toilet` calls in GitHub code search
(2026-10-23): 79 % of `figlet` calls name no font, so get `standard`; of those that do,
about a third say `slant`, then `small`, `mini`, `big`, `standard`, `smslant`, `shadow`,
`script`, all figlet's own. `toilet` calls mostly name one: `mono12` first, then figlet's
`slant`, `big`, `standard`, `small`, the built-in `term`, and toilet's `future`,
`smbraille`, `pagga`, `smblock`, `mono9`. A tail names fonts from add-on collections
(`3d`, `ansishadow`, `doom`, `speed`) that neither tool ships. No script expects every
font.

**Built into cash.exe (decided 2026-10-23): 23 fonts, 443 KB as stored.** figlet's 17
fonts without `mnemonic` (83 KB, unused in the sample; 296 KB) and toilet's six most used:
`mono12`, `mono9`, `future`, `smbraille`, `pagga`, `smblock` (148 KB). No control file is
built in: cash reads UTF-8 natively, so `utf8.flc` is not needed, and the national ones
load from a font directory like every other font not built in. toilet's `term` is a few
lines of code.

### 6.6 One name, and the `-E` clash

One command, `figlet`, with toilet's options added (decided 2026-10-23); there is no
`toilet` command. `-E fmt` is toilet's export. figlet's `-E` only undid an earlier `-D`
(German translation is off by default), so nothing is lost: `-D` stays, and `-N` clears it
with the control files. Defaults are figlet's: the `standard` font, kerning or smushing
as the font says, no colour until a colour font, filter or option asks for it.

Scoop's Go figlet, where installed, is shadowed; `cash doctor` lists `figlet` among the
deliberate shadows ("figlet and toilet's options, colour, more fonts").

### 6.7 Exports

Three, decided 2026-10-23: `utf8` (the default: UTF-8 with ANSI colour), `ansi` (CP437
with 16 colours, for old consoles and BBS-style art) and `text` (no escapes at all, what a
pipe gets under `--color=auto`). `-E list` names them. HTML, SVG, IRC and libcaca's other
formats are left out; the sixel image mode (section 7) is a render mode, not an export.

### 6.8 Errors and output

Messages in cash's voice, statuses as the tools have them (0, 1 for a bad font or
option). `--help` and `man figlet`, `man figfont` pages. Nothing cites this document.

### 6.9 Windows-only parts

Rasterising an installed font (the Windows-fonts extra and the sixel image mode) needs
GDI or DirectWrite; that code goes in cash-win32, which hands cash-figlet a plain bitmap,
so the leaf crate stays free of Windows calls.

## 7. Extras beyond figlet and toilet

Costs are rough: S a day or less, M a few days, L a week or more.

| Extra | Where it comes from | Cost | Gain |
| --- | --- | --- | --- |
| True-colour gradients: two or more stops, horizontal, vertical, diagonal; named palettes (sunset, ocean, fire, matrix ...); reversed | cfonts, oh-my-logo | S | the look people want today ("like Claude Code"); a filter on the canvas |
| lolcat rainbow in true colour, with seed and spread | lolcat | S | a filter |
| Letter spacing, line height, background colour | cfonts | S | layout options |
| Shadow and outline filters | oh-my-logo's filled mode | S | filters |
| Unicode borders: single, double, rounded, heavy, ASCII; padding | toilet's CP437 `border`, extended | S | a filter |
| Font gallery and picker: `figlist`, `showfigfonts`, and `figlet --pick "text"` with a live preview in cash's picker | figlet's helpers, cash-picker | M | finding a font among 75 without scrolling |
| Pixel fonts with sub-cell blocks: half, quadrant, sextant, octant (an 8x8 font becomes 4x2 cells) | tui-big-text, Cascadia Code | M | crisp small big-text; looks best with Terminal's built-in glyphs |
| Any installed Windows font as big text: rasterise with GDI, draw with blocks or octants | new | M | Segoe, Consolas, any TTF, no font files to find |
| Double-size lines in Windows Terminal (`--dec`) | DEC VT100 | S | real smooth text at 2x; open Terminal bugs on wrapping |
| Image mode: the text as a sixel picture in any Windows font | Terminal 1.22 sixel, coolfetch | M to L | true typography; Terminal only |
| TheDraw `.tdf` colour fonts | tdfiglet | M | 1,200 DOS colour fonts the user can download; none bundled |
| Animation: scrolling rainbow, typewriter | lolcat `-a` | S to M | fun; never in a pipe |
| Export to PNG | new | M | an image without a terminal |

Chosen on 2026-10-23: the colour pack (gradients and palettes, lolcat rainbow, shadow and
outline, Unicode borders, letter spacing, line height, background), the font gallery and
picker, Windows fonts and pixel modes (with double-size lines), and the sixel image mode
with TheDraw fonts. Not chosen: animation, PNG export.

## 8. Tests

- rs-figlet's golden fixtures (real figlet and toilet output for four fonts and four texts)
  come along with the code.
- An oracle in WSL once `figlet` and `toilet` are installed there (`sudo apt install
  figlet toilet`, the user's sudo): same behaviour, not bytes, as the archive tools.
  Every built-in font times a set of texts, each layout mode, widths, justification.
- Unit tests per filter and exporter on small canvases; the `ansi` export checked as
  CP437 bytes.
- Visual checks in the user's Terminal profile ("Bash", 175 %) for colour, blocks, octants
  and double-size lines.

## 9. Decided by the user (2026-10-23, by pick lists)

1. **Names: `figlet` only**, with toilet's options added; `-E` is export. Turned down:
   `figlet` and `toilet` both, and a new name with the old ones as aliases.
2. **Fonts: 23 built into cash.exe, 443 KB** (section 6.5): figlet's 17 without
   `mnemonic`, and toilet's `mono12`, `mono9`, `future`, `smbraille`, `pagga`, `smblock`;
   the rest from a font directory. First answer "all of figlet's and toilet's" (1.5 MB),
   changed the same day once the GitHub sample showed what scripts use. Turned down: the
   same with `mnemonic` (527 KB), rs-figlet's nine (228 KB), everything.
3. **Extras: all four groups** offered (colour pack; gallery and picker; Windows fonts and
   pixel modes; sixel image and TheDraw fonts). Not offered as choices and left out:
   animation, PNG export.
4. **Exports: `utf8`, `ansi` (CP437) and `text`.** Turned down: six formats with HTML,
   SVG and IRC, and all twelve of libcaca's.

5. **The release that finishes this work is 2.0.0**, a milestone, not a breaking change;
   its notes say so, so nobody holds back. Turned down: a minor 1.x bump as every release
   before it, and deciding at release time.

Still open, settled in their phases: the order of the extras (section 10 proposes one),
and whether the user installs `figlet` and `toilet` in WSL for the oracle.

## 10. Order of work

1. `cash-figlet`: font, control, layout (the copied smusher plus wrapping and
   justification), canvas, `text` export; rs-figlet's goldens green.
2. `figlet` front end, built-in fonts, font search, `-I`, help and man pages.
3. Colour fonts, toilet's filters and options, the `utf8` and `ansi` exports.
4. The colour pack (cheapest, most visible).
5. `figlist`, `showfigfonts` and the picker.
6. Pixel modes, Windows fonts (cash-win32 rasteriser), double-size lines; checked in the
   user's Terminal profile, octants first.
7. Sixel image mode (reusing the rasteriser) and the TheDraw reader.
8. The WSL oracle, the `cash doctor` row, NOTICE and the `upstreams.toml` entry for
   rs-figlet.
