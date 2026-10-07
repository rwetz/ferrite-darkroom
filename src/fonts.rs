//! The faces text art is drawn in: the IBM VGA that Ferrite's display type
//! uses, a second retro face, and four modern monospaces picked for ASCII
//! art (even stroke weight, distinct `l 1 I 0 O`, full block and box
//! coverage, no ligatures). All are bundled, so exports look the same on
//! every machine; see `assets/fonts` for their licenses.
//!
//! A glyph is drawn into whatever cell size it's asked for, with its edges
//! anti-aliased by area, so a 200-column export at any size has even
//! strokes. Coverage maps are cached.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use ab_glyph::{Font as _, FontRef, PxScale, point};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Font {
    /// IBM VGA 8×16 (PxPlus, int10h.org): the DOS text-mode face.
    #[default]
    Vga,
    /// VT323: the DEC VT320 terminal's face, taller and narrower.
    Vt323,
    JetBrains,
    Plex,
    Cascadia,
    Fira,
}

impl Font {
    pub const ALL: [Font; 6] = [Font::Vga, Font::Vt323, Font::JetBrains, Font::Plex, Font::Cascadia, Font::Fira];

    pub fn key(self) -> &'static str {
        match self {
            Font::Vga => "vga",
            Font::Vt323 => "vt323",
            Font::JetBrains => "jetbrains-mono",
            Font::Plex => "plex-mono",
            Font::Cascadia => "cascadia-mono",
            Font::Fira => "fira-mono",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Font::Vga => "IBM VGA",
            Font::Vt323 => "VT323",
            Font::JetBrains => "JetBrains Mono",
            Font::Plex => "IBM Plex Mono",
            Font::Cascadia => "Cascadia Mono",
            Font::Fira => "Fira Mono",
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Font::Vga => "The DOS text-mode face: chunky pixels, every block and box piece",
            Font::Vt323 => "A DEC terminal: thin pixel strokes, tall and airy",
            Font::JetBrains => "Clean and even, tall lowercase; reads well small",
            Font::Plex => "IBM's monospace: a little warmer, slab serifs",
            Font::Cascadia => "Windows Terminal's face: full block, box and braille coverage",
            Font::Fira => "Rounded and open, a light touch",
        }
    }

    /// For HTML and SVG exports, with fallbacks.
    pub fn css(self) -> &'static str {
        match self {
            Font::Vga => "'PxPlus IBM VGA 8x16', 'Px437 IBM VGA 8x16', monospace",
            Font::Vt323 => "VT323, monospace",
            Font::JetBrains => "'JetBrains Mono', monospace",
            Font::Plex => "'IBM Plex Mono', monospace",
            Font::Cascadia => "'Cascadia Mono', 'Cascadia Code', Consolas, monospace",
            Font::Fira => "'Fira Mono', monospace",
        }
    }

    pub fn from_key(key: &str) -> Option<Font> {
        Font::ALL.into_iter().find(|f| f.key() == key)
    }

    fn bytes(self) -> &'static [u8] {
        match self {
            Font::Vga => include_bytes!("../assets/fonts/PxPlus_IBM_VGA_8x16.ttf"),
            Font::Vt323 => include_bytes!("../assets/fonts/VT323-Regular.ttf"),
            Font::JetBrains => include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
            Font::Plex => include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf"),
            Font::Cascadia => include_bytes!("../assets/fonts/CascadiaMono.ttf"),
            Font::Fira => include_bytes!("../assets/fonts/FiraMono-Regular.ttf"),
        }
    }

    fn face(self) -> &'static FontRef<'static> {
        static FACES: OnceLock<Vec<FontRef<'static>>> = OnceLock::new();
        let faces = FACES.get_or_init(|| Font::ALL.iter().map(|f| FontRef::try_from_slice(f.bytes()).expect("bundled font parses")).collect());
        &faces[self as usize]
    }

    /// Whether the face has `ch`.
    pub fn has(self, ch: char) -> bool {
        ch == ' ' || self.face().glyph_id(ch).0 != 0
    }

    /// `ch` drawn to fill a `w`×`h` cell: coverage 0..1 per pixel, row by
    /// row. The face's ascent to descent spans the height and its advance
    /// the width, so every face lines up on the same grid.
    pub fn coverage(self, ch: char, w: u32, h: u32) -> Arc<Vec<f32>> {
        type Cache = Mutex<HashMap<(Font, char, u32, u32), Arc<Vec<f32>>>>;
        static CACHE: OnceLock<Cache> = OnceLock::new();
        let cache = CACHE.get_or_init(Default::default);
        let key = (self, ch, w, h);
        if let Some(hit) = cache.lock().ok().and_then(|c| c.get(&key).cloned()) {
            return hit;
        }
        let out = Arc::new(self.draw(ch, w.max(1), h.max(1)));
        if let Ok(mut c) = cache.lock() {
            // An export at a new size mustn't grow it without bound.
            if c.len() > 20_000 {
                c.clear();
            }
            c.insert(key, out.clone());
        }
        out
    }

    fn draw(self, ch: char, w: u32, h: u32) -> Vec<f32> {
        let mut out = vec![0.; (w * h) as usize];
        if ch == ' ' {
            return out;
        }
        let face = self.face();
        let id = face.glyph_id(ch);
        if id.0 == 0 {
            return out;
        }
        let height = face.height_unscaled();
        let advance = face.h_advance_unscaled(face.glyph_id('M')).max(1.);
        // Scale so the line fills the cell's height and the advance its width.
        let scale = PxScale { x: w as f32 * height / advance, y: h as f32 };
        let ascent = face.ascent_unscaled() * h as f32 / height;
        let glyph = id.with_scale_and_position(scale, point(0., ascent));
        if let Some(outline) = face.outline_glyph(glyph) {
            let b = outline.px_bounds();
            outline.draw(|x, y, c| {
                let (x, y) = (x as i32 + b.min.x as i32, y as i32 + b.min.y as i32);
                if (0..w as i32).contains(&x) && (0..h as i32).contains(&y) {
                    let at = (y as u32 * w + x as u32) as usize;
                    out[at] = (out[at] + c).min(1.);
                }
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_face_draws_ascii() {
        for f in Font::ALL {
            for ch in ['A', '@', '#', '.', 'g'] {
                assert!(f.has(ch), "{f:?} {ch}");
                let c = f.coverage(ch, 8, 16);
                let ink: f32 = c.iter().sum();
                assert!(ink > 2., "{f:?} {ch}: {ink}");
            }
            assert_eq!(f.coverage(' ', 8, 16).iter().sum::<f32>(), 0.);
            assert_eq!(Font::from_key(f.key()), Some(f));
        }
    }

    #[test]
    fn vga_is_pixel_exact_at_its_own_size() {
        // The 8×16 face at 8×16: every pixel fully on or off.
        let c = Font::Vga.coverage('A', 8, 16);
        assert!(c.iter().all(|&v| !(0.02..=0.98).contains(&v)), "{c:?}");
        // The same pixels as the font's own bitmaps (rendered by Pillow in
        // ferrite-design's gen-glyphs.py), bit 7 = leftmost.
        const BITMAPS: [(char, [u8; 16]); 5] = [
            ('/', [0, 0, 0, 0, 2, 6, 12, 24, 48, 96, 192, 128, 0, 0, 0, 0]),
            ('@', [0, 0, 0, 124, 198, 198, 222, 222, 222, 220, 192, 124, 0, 0, 0, 0]),
            ('A', [0, 0, 16, 56, 108, 198, 198, 254, 198, 198, 198, 198, 0, 0, 0, 0]),
            ('_', [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 0, 0]),
            ('g', [0, 0, 0, 0, 0, 118, 204, 204, 204, 204, 204, 124, 12, 204, 120, 0]),
        ];
        for (ch, rows) in &BITMAPS {
            let c = Font::Vga.coverage(*ch, 8, 16);
            for (y, bits) in rows.iter().enumerate() {
                let drawn = (0..8).fold(0u8, |b, x| if c[y * 8 + x] > 0.5 { b | 0x80 >> x } else { b });
                assert_eq!(drawn, *bits, "{ch} row {y}");
            }
        }
    }

    #[test]
    fn heavier_glyphs_have_more_ink() {
        for f in Font::ALL {
            let ink = |ch| f.coverage(ch, 16, 32).iter().sum::<f32>();
            assert!(ink('@') > ink('.'), "{f:?}");
        }
    }
}
