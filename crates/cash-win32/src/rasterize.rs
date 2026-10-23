//! Characters drawn by an installed font into a bitmap, for `figlet`'s Windows fonts
//! (decisions.md D84).
//!
//! GDI draws each character, white on black and without anti-aliasing, into a 32-bit
//! bitmap of the size the font says it takes; what comes back is how much of each pixel
//! the character covers, which `figlet` turns into block characters. GDI substitutes a
//! font it has for one it has not, without saying so; that is caught and refused, so a
//! misspelt name is an error rather than Arial.

use std::io;

use windows_sys::Win32::Graphics::Gdi::{HDC, HFONT};

/// A character as pixels: how much of each one it covers, 0 to 255, row by row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Raster {
    /// Pixels across: how far the font moves on after the character.
    pub width: usize,
    /// Pixels down: the font's line.
    pub height: usize,
    /// Coverage of each pixel, row by row.
    pub coverage: Vec<u8>,
}

/// A font, chosen once, that draws characters one at a time.
pub struct Rasterizer {
    dc: HDC,
    font: HFONT,
}

impl Rasterizer {
    /// The installed font `face`, `pixels` high, bold and italic as asked.
    ///
    /// # Errors
    ///
    /// `NotFound` when no font of that name is installed, `InvalidInput` for a name GDI
    /// cannot hold (more than 31 UTF-16 units), and GDI's failures.
    pub fn new(face: &str, pixels: i32, bold: bool, italic: bool) -> io::Result<Self> {
        use windows_sys::Win32::Graphics::Gdi::{
            CLIP_DEFAULT_PRECIS, CreateCompatibleDC, CreateFontW, DEFAULT_CHARSET, DeleteDC,
            DeleteObject, FW_BOLD, FW_NORMAL, GetTextFaceW, NONANTIALIASED_QUALITY, OUT_TT_PRECIS,
            SelectObject,
        };

        let units = face.encode_utf16().count();
        if units == 0 || units > 31 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a font name is 1 to 31 characters",
            ));
        }
        let wide = crate::wide::to_wide_nul(face);
        // SAFETY: a memory device context, which needs no screen; deleted on drop.
        let dc = unsafe { CreateCompatibleDC(std::ptr::null_mut()) };
        if dc.is_null() {
            return Err(io::Error::last_os_error());
        }
        let weight = if bold { FW_BOLD } else { FW_NORMAL };
        // SAFETY: the face name is NUL-terminated; a negative height asks for that many
        // pixels of character, without the line's internal leading.
        let font = unsafe {
            CreateFontW(
                -pixels.max(1),
                0,
                0,
                0,
                weight.cast_signed(),
                u32::from(italic),
                0,
                0,
                u32::from(DEFAULT_CHARSET),
                u32::from(OUT_TT_PRECIS),
                u32::from(CLIP_DEFAULT_PRECIS),
                u32::from(NONANTIALIASED_QUALITY),
                0,
                wide.as_ptr(),
            )
        };
        if font.is_null() {
            let error = io::Error::last_os_error();
            // SAFETY: the context came from CreateCompatibleDC above and is deleted once.
            unsafe { DeleteDC(dc) };
            return Err(error);
        }
        // SAFETY: both handles were made above; the font stays selected while they live.
        unsafe { SelectObject(dc, font) };
        let mut chosen = [0u16; 32];
        // SAFETY: `chosen` holds the 32 units it is said to.
        let length = unsafe { GetTextFaceW(dc, 32, chosen.as_mut_ptr()) };
        let chosen = chosen
            .get(..usize::try_from(length).unwrap_or(0).saturating_sub(1))
            .unwrap_or_default();
        if !String::from_utf16_lossy(chosen).eq_ignore_ascii_case(face) {
            // SAFETY: the context is deleted once, before the font it holds.
            unsafe { DeleteDC(dc) };
            // SAFETY: the font came from CreateFontW above and is deleted once.
            unsafe { DeleteObject(font) };
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("no font called {face} is installed"),
            ));
        }
        Ok(Self { dc, font })
    }

    /// `ch` drawn in the font.
    ///
    /// # Errors
    ///
    /// GDI's failures.
    pub fn glyph(&mut self, ch: char) -> io::Result<Raster> {
        use windows_sys::Win32::Foundation::SIZE;
        use windows_sys::Win32::Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateDIBSection, DIB_RGB_COLORS, DeleteObject,
            GdiFlush, GetTextExtentPoint32W, OPAQUE, SelectObject, SetBkColor, SetBkMode,
            SetTextColor, TextOutW,
        };

        let mut units = [0u16; 2];
        let text = ch.encode_utf16(&mut units);
        let length = i32::try_from(text.len()).unwrap_or(1);
        let mut size = SIZE { cx: 0, cy: 0 };
        // SAFETY: `text` holds the `length` units it is said to; `size` is written.
        let measured =
            unsafe { GetTextExtentPoint32W(self.dc, text.as_ptr(), length, &raw mut size) };
        if measured == 0 {
            return Err(io::Error::last_os_error());
        }
        let width = usize::try_from(size.cx).unwrap_or(0);
        let height = usize::try_from(size.cy).unwrap_or(0);
        if width == 0 || height == 0 {
            return Ok(Raster {
                width,
                height,
                coverage: vec![0; width * height],
            });
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: u32::try_from(std::mem::size_of::<BITMAPINFOHEADER>()).unwrap_or(40),
                biWidth: size.cx,
                // Negative: the rows top first.
                biHeight: -size.cy,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [windows_sys::Win32::Graphics::Gdi::RGBQUAD {
                rgbBlue: 0,
                rgbGreen: 0,
                rgbRed: 0,
                rgbReserved: 0,
            }],
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        // SAFETY: `info` describes a 32-bit top-down bitmap; GDI allocates its pixels and
        // says where in `bits`, which live as long as the bitmap.
        let bitmap = unsafe {
            CreateDIBSection(
                self.dc,
                &raw const info,
                DIB_RGB_COLORS,
                &raw mut bits,
                std::ptr::null_mut(),
                0,
            )
        };
        if bitmap.is_null() || bits.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: both handles are live; the old bitmap is selected back before the new
        // one is deleted.
        let old = unsafe { SelectObject(self.dc, bitmap) };
        // SAFETY: the context is live.
        unsafe { SetTextColor(self.dc, 0x00ff_ffff) };
        // SAFETY: the context is live.
        unsafe { SetBkColor(self.dc, 0) };
        // SAFETY: the context is live.
        unsafe { SetBkMode(self.dc, OPAQUE.cast_signed()) };
        // SAFETY: `text` holds the `length` units it is said to.
        let drawn = unsafe { TextOutW(self.dc, 0, 0, text.as_ptr(), length) };
        // SAFETY: no arguments; makes GDI finish drawing before the pixels are read.
        unsafe { GdiFlush() };
        let coverage = if drawn == 0 {
            vec![0; width * height]
        } else {
            // SAFETY: GDI allocated `width * height` 32-bit pixels at `bits` for the
            // bitmap, which is still alive; they are only read here.
            let pixels = unsafe { std::slice::from_raw_parts(bits.cast::<u32>(), width * height) };
            pixels
                .iter()
                .map(|pixel| u8::try_from(pixel & 0xff).unwrap_or(0))
                .collect()
        };
        // SAFETY: the bitmap this replaced goes back into the context.
        unsafe { SelectObject(self.dc, old) };
        // SAFETY: the bitmap came from CreateDIBSection above, is selected into nothing
        // now, and is deleted once.
        unsafe { DeleteObject(bitmap) };
        Ok(Raster {
            width,
            height,
            coverage,
        })
    }
}

impl Drop for Rasterizer {
    fn drop(&mut self) {
        use windows_sys::Win32::Graphics::Gdi::{DeleteDC, DeleteObject};
        // SAFETY: the context is deleted once, before the font it holds.
        unsafe { DeleteDC(self.dc) };
        // SAFETY: the font came from CreateFontW and is deleted once.
        unsafe { DeleteObject(self.font) };
    }
}

#[cfg(test)]
mod tests {
    use super::Rasterizer;

    #[test]
    fn an_installed_font_draws_and_a_missing_one_is_refused() {
        let mut arial = Rasterizer::new("Arial", 32, false, false).unwrap();
        let l = arial.glyph('l').unwrap();
        let m = arial.glyph('M').unwrap();
        assert!(m.width > l.width, "an M is wider than an l");
        assert_eq!(l.height, m.height);
        assert!(m.coverage.iter().any(|&c| c > 128), "the M is drawn");
        let space = arial.glyph(' ').unwrap();
        assert!(space.coverage.iter().all(|&c| c == 0), "a space is blank");

        let missing = Rasterizer::new("No Such Font Here", 16, false, false);
        assert_eq!(
            missing.err().map(|e| e.kind()),
            Some(std::io::ErrorKind::NotFound)
        );
    }
}
