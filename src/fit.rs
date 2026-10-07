//! Choosing characters: which set, and how each cell picks from it.
//!
//! Every cell of the picture is compared with every glyph of the set as
//! the chosen font draws it, so the art is fitted to the face it's shown
//! in. *Tone* picks by how much ink a glyph has; *shape* by how well its
//! 4×8 coverage matches the picture under the cell, so edges become `/`,
//! `|`, `_` where they fall. *Diffuse* carries each cell's tone error to
//! its neighbours (Floyd–Steinberg), so a gradient doesn't band.
//!
//! The sets are ferrite-design's thirteen, plus plain printable ASCII:
//! the default, since its text pastes anywhere.

use std::sync::Arc;

use ferrite_design::ascii::{Charset, Fit};

use crate::fonts::Font;

/// A character set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Set {
    /// Printable ASCII, `!` to `~`: pastes into any text box or terminal.
    #[default]
    Ascii,
    /// One of ferrite-design's sets.
    Ferrite(Charset),
}

impl Set {
    pub fn all() -> Vec<Set> {
        std::iter::once(Set::Ascii).chain(Charset::ALL.into_iter().map(Set::Ferrite)).collect()
    }

    pub fn key(self) -> String {
        match self {
            Set::Ascii => "ascii".into(),
            Set::Ferrite(c) => c.name().replace(' ', "-"),
        }
    }

    pub fn name(self) -> String {
        match self {
            Set::Ascii => "Plain ASCII".into(),
            Set::Ferrite(Charset::Full) => "Every glyph".into(),
            Set::Ferrite(c) => {
                let n = c.name();
                n[..1].to_uppercase() + &n[1..]
            }
        }
    }

    pub fn from_key(key: &str) -> Option<Set> {
        let key = key.trim().to_lowercase();
        Set::all().into_iter().find(|s| s.key() == key || s.name().to_lowercase() == key)
    }

    /// The characters, space first.
    pub fn chars(self) -> String {
        match self {
            Set::Ascii => (' '..='~').collect(),
            Set::Ferrite(c) => c.chars(),
        }
    }

    pub fn default_fit(self) -> Fit {
        match self {
            Set::Ascii => Fit::Shape,
            Set::Ferrite(c) => c.default_fit(),
        }
    }

    /// Whether a short set needs its tone carried as density to read.
    pub fn default_diffuse(self) -> bool {
        matches!(self, Set::Ferrite(Charset::Slashes | Charset::Lines | Charset::Binary | Charset::Classic | Charset::Digits))
    }
}

/// Sub-samples per cell: an 8×16 glyph in 2×2 blocks.
pub const SUB_W: usize = 4;
pub const SUB_H: usize = 8;
const SUBS: usize = SUB_W * SUB_H;

struct Glyph {
    ch: char,
    cover: [f32; SUBS],
    ink: f32,
}

/// The set's glyphs as `font` draws them. `ink` is scaled so the darkest
/// is 1, and tone uses the set's whole range.
fn glyphs(set: Set, font: Font) -> Arc<Vec<Glyph>> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    type Cache = Mutex<HashMap<(Set, Font), Arc<Vec<Glyph>>>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().ok().and_then(|c| c.get(&(set, font)).cloned()) {
        return hit;
    }
    let mut out: Vec<Glyph> = set
        .chars()
        .chars()
        .filter(|&c| font.has(c))
        .map(|ch| {
            let px = font.coverage(ch, 8, 16);
            let mut cover = [0.; SUBS];
            for (i, c) in cover.iter_mut().enumerate() {
                let (sx, sy) = (i % SUB_W * 2, i / SUB_W * 2);
                *c = (px[sy * 8 + sx] + px[sy * 8 + sx + 1] + px[(sy + 1) * 8 + sx] + px[(sy + 1) * 8 + sx + 1]) / 4.;
            }
            Glyph { ch, ink: cover.iter().sum::<f32>() / SUBS as f32, cover }
        })
        .collect();
    if !out.iter().any(|g| g.ch == ' ') {
        out.insert(0, Glyph { ch: ' ', cover: [0.; SUBS], ink: 0. });
    }
    // Tone uses the whole range; shapes compare at their real coverage.
    let max = out.iter().map(|g| g.ink).fold(0., f32::max).max(1e-3);
    for g in &mut out {
        g.ink /= max;
    }
    let out = Arc::new(out);
    if let Ok(mut c) = cache.lock() {
        c.insert((set, font), out.clone());
    }
    out
}

/// How a picture becomes characters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    pub set: Set,
    pub fit: Fit,
    pub diffuse: bool,
    pub font: Font,
}

/// `ink` (0 = paper, 1 = ink) at `cols × SUB_W` by `rows × SUB_H` as
/// characters, row by row. `skip` cells (masked out) stay blank.
pub fn fit(ink: &[f32], cols: usize, rows: usize, style: Style, skip: Option<&[bool]>) -> Vec<char> {
    let glyphs = glyphs(style.set, style.font);
    let sw = cols * SUB_W;
    let mut carry = vec![0f32; cols * rows];
    let mut out = Vec::with_capacity(cols * rows);
    for y in 0..rows {
        for x in 0..cols {
            let at = y * cols + x;
            if skip.is_some_and(|s| s[at]) {
                out.push(' ');
                continue;
            }
            let mut cell = [0f32; SUBS];
            for (i, c) in cell.iter_mut().enumerate() {
                *c = ink[(y * SUB_H + i / SUB_W) * sw + x * SUB_W + i % SUB_W];
            }
            let mean = cell.iter().sum::<f32>() / SUBS as f32;
            let err = if style.diffuse { carry[at] } else { 0. };
            let target = (mean + err).clamp(0., 1.);
            let best = match style.fit {
                Fit::Tone => glyphs.iter().min_by(|a, b| (a.ink - target).abs().total_cmp(&(b.ink - target).abs())),
                Fit::Shape => {
                    // The cell's shape, shifted by the carried tone.
                    let shift = target - mean;
                    glyphs.iter().min_by(|a, b| {
                        let d = |g: &Glyph| g.cover.iter().zip(&cell).map(|(c, p)| (c - (p + shift)).powi(2)).sum::<f32>();
                        d(a).total_cmp(&d(b))
                    })
                }
            };
            let best = best.map(|g| (g.ch, g.ink)).unwrap_or((' ', 0.));
            out.push(best.0);
            if style.diffuse {
                let e = target - best.1;
                let mut give = |dx: isize, dy: usize, k: f32| {
                    let (nx, ny) = (x as isize + dx, y + dy);
                    if nx >= 0 && (nx as usize) < cols && ny < rows {
                        carry[ny * cols + nx as usize] += e * k;
                    }
                };
                give(1, 0, 7. / 16.);
                give(-1, 1, 3. / 16.);
                give(0, 1, 5. / 16.);
                give(1, 1, 1. / 16.);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(set: Set, fit: Fit, diffuse: bool) -> Style {
        Style { set, fit, diffuse, font: Font::Vga }
    }

    #[test]
    fn plain_ascii_is_ascii() {
        let ramp: Vec<f32> = (0..40 * SUB_W * SUB_H).map(|i| (i % (40 * SUB_W)) as f32 / (40 * SUB_W) as f32).collect();
        let out = fit(&ramp, 40, 1, style(Set::Ascii, Fit::Shape, false), None);
        assert!(out.iter().all(|c| c.is_ascii()));
        assert_eq!(out[0], ' ');
        assert_ne!(out[39], ' ');
    }

    #[test]
    fn diffusion_breaks_up_bands() {
        // A flat tone between two of the set's glyphs: without diffusion
        // every cell is the same character; with it, a mix.
        let flat = vec![0.3; 20 * 4 * SUB_W * SUB_H];
        let set = Set::Ferrite(Charset::Classic);
        let plain = fit(&flat, 20, 4, style(set, Fit::Tone, false), None);
        let mixed = fit(&flat, 20, 4, style(set, Fit::Tone, true), None);
        assert!(plain.iter().all(|c| *c == plain[0]));
        assert!(mixed.iter().any(|c| *c != mixed[0]));
    }

    #[test]
    fn shape_finds_edges() {
        let cell = |f: &dyn Fn(usize, usize) -> f32| -> Vec<f32> { (0..SUB_W * SUB_H).map(|i| f(i % SUB_W, i / SUB_W)).collect() };
        let ascii = style(Set::Ascii, Fit::Shape, false);
        // A thin vertical stroke down the middle, and a line low in the cell
        // (where the VGA face's underscore sits).
        let bar = cell(&|x, _| if x == 1 || x == 2 { 0.5 } else { 0. });
        assert_eq!(fit(&bar, 1, 1, ascii, None), vec!['|']);
        let floor = cell(&|_, y| if y == SUB_H - 2 { 1. } else { 0. });
        assert_eq!(fit(&floor, 1, 1, ascii, None), vec!['_']);
    }

    #[test]
    fn masked_cells_stay_blank() {
        let full = vec![1.; 2 * SUB_W * SUB_H];
        let out = fit(&full, 2, 1, style(Set::Ascii, Fit::Tone, false), Some(&[false, true]));
        assert_ne!(out[0], ' ');
        assert_eq!(out[1], ' ');
    }

    #[test]
    fn keys_round_trip() {
        for s in Set::all() {
            assert_eq!(Set::from_key(&s.key()), Some(s), "{s:?}");
        }
        // 1.0 recipes wrote ferrite's names; any case reads.
        assert_eq!(Set::from_key("Classic"), Some(Set::Ferrite(Charset::Classic)));
        assert_eq!(Set::from_key("best-character"), Some(Set::Ferrite(Charset::Full)));
    }

    #[test]
    fn every_font_fits() {
        let ramp: Vec<f32> = (0..10 * SUB_W * SUB_H).map(|i| (i % (10 * SUB_W)) as f32 / 40.).collect();
        for f in Font::ALL {
            let out = fit(&ramp, 10, 1, Style { font: f, ..style(Set::Ascii, Fit::Tone, true) }, None);
            assert_eq!(out.len(), 10);
        }
    }
}
