//! How a developed print is drawn: each art pixel becomes a cell of
//! `cell`×`cell` image pixels holding a shape (square, circle, diamond,
//! plus) on the paper, with an optional gutter between cells, dots that
//! grow with the tone (halftone without a halftone algorithm), a
//! transparent paper, and a lattice of dots over the bare paper.
//!
//! Shapes are hard-edged on purpose: pixel art, not anti-aliased vector
//! art. Pure, so the preview and the PNG export draw identical pixels.

use crate::engine::{Rgb, oklab};
use crate::studio::{Art, Print};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shape {
    Square,
    Circle,
    Diamond,
    Plus,
}

impl Shape {
    pub const ALL: [Shape; 4] = [Shape::Square, Shape::Circle, Shape::Diamond, Shape::Plus];

    pub fn key(self) -> &'static str {
        match self {
            Shape::Square => "square",
            Shape::Circle => "circle",
            Shape::Diamond => "diamond",
            Shape::Plus => "plus",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Shape::Square => "Square",
            Shape::Circle => "Circle",
            Shape::Diamond => "Diamond",
            Shape::Plus => "Plus",
        }
    }

    pub fn from_key(key: &str) -> Option<Shape> {
        Shape::ALL.into_iter().find(|s| s.key() == key)
    }

    /// Whether offset `(dx, dy)` from the cell centre is inside the shape
    /// of half-size `r`.
    fn contains(self, dx: f32, dy: f32, r: f32) -> bool {
        let (ax, ay) = (dx.abs(), dy.abs());
        match self {
            Shape::Square => ax <= r && ay <= r,
            Shape::Circle => ax * ax + ay * ay <= r * r,
            Shape::Diamond => ax + ay <= r,
            Shape::Plus => (ax <= r / 3. && ay <= r) || (ay <= r / 3. && ax <= r),
        }
    }
}

/// Which palette colour is the paper the shapes sit on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Paper {
    /// The palette's first colour: the scheme's background for Scheme.
    First,
    Darkest,
    Lightest,
}

impl Paper {
    pub const ALL: [Paper; 3] = [Paper::First, Paper::Darkest, Paper::Lightest];

    pub fn key(self) -> &'static str {
        match self {
            Paper::First => "first",
            Paper::Darkest => "darkest",
            Paper::Lightest => "lightest",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Paper::First => "First",
            Paper::Darkest => "Darkest",
            Paper::Lightest => "Lightest",
        }
    }

    pub fn from_key(key: &str) -> Option<Paper> {
        Paper::ALL.into_iter().find(|p| p.key() == key)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub shape: Shape,
    /// 0..0.9: the share of each cell left as gap.
    pub gutter: f32,
    /// Shapes shrink where the photo had less of their colour.
    pub modulate: bool,
    pub paper: Paper,
    /// The paper is left transparent (PNG exports; the preview shows the
    /// easel through it).
    pub transparent: bool,
    /// 0..1: a dot of this size, in the colour furthest from the paper, on
    /// every bare paper cell. 0 = off.
    pub lattice: f32,
}

impl Default for Look {
    fn default() -> Self {
        Look { shape: Shape::Square, gutter: 0., modulate: false, paper: Paper::First, transparent: false, lattice: 0. }
    }
}

impl Look {
    /// Plain squares edge to edge: every cell is one solid block.
    fn is_plain(&self) -> bool {
        self.shape == Shape::Square && self.gutter <= 0. && !self.modulate && self.lattice <= 0.
    }

    /// For cache keys.
    pub fn bits(&self) -> (Shape, u32, bool, Paper, bool, u32) {
        (self.shape, self.gutter.to_bits(), self.modulate, self.paper, self.transparent, self.lattice.to_bits())
    }
}

/// The palette index of the paper.
pub fn paper_index(colors: &[Rgb], paper: Paper) -> usize {
    let l = |i: usize| oklab(colors[i])[0];
    let by = |better: fn(f32, f32) -> bool| (0..colors.len()).fold(0, |best, i| if better(l(i), l(best)) { i } else { best });
    match paper {
        Paper::First => 0,
        Paper::Darkest => by(|a, b| a < b),
        Paper::Lightest => by(|a, b| a > b),
    }
}

fn rgba8(c: Rgb) -> [u8; 4] {
    let [r, g, b] = c.map(|v| (v.clamp(0., 1.) * 255.).round() as u8);
    [r, g, b, 255]
}

/// Draw `art` at `cell` pixels per art pixel. Returns `(w, h, rgba)`.
pub fn rgba(art: &Art, cell: u32, look: Look) -> (u32, u32, Vec<u8>) {
    let cell = cell.max(1);
    let (w, h) = (art.w * cell, art.h * cell);
    let lut: Vec<[u8; 4]> = art.colors.iter().map(|&c| rgba8(c)).collect();
    let paper = paper_index(&art.colors, look.paper);
    let paper_px = if look.transparent { [0; 4] } else { lut[paper] };
    let ink_of = |i: u8| if look.transparent && i as usize == paper { [0; 4] } else { lut[i as usize] };

    let mut out = vec![0u8; (w * h * 4) as usize];
    if look.is_plain() {
        for y in 0..h {
            let row = &art.index[((y / cell) * art.w) as usize..][..art.w as usize];
            let line = &mut out[(y * w * 4) as usize..][..(w * 4) as usize];
            for (x, px) in line.chunks_exact_mut(4).enumerate() {
                px.copy_from_slice(&ink_of(row[x / cell as usize]));
            }
        }
        return (w, h, out);
    }

    for px in out.chunks_exact_mut(4) {
        px.copy_from_slice(&paper_px);
    }
    // The lattice is drawn in the colour furthest from the paper.
    let paper_l = oklab(art.colors[paper])[0];
    let contrast = (0..art.colors.len()).max_by(|&a, &b| {
        let d = |i: usize| (oklab(art.colors[i])[0] - paper_l).abs();
        d(a).total_cmp(&d(b))
    });
    let color_l: Vec<f32> = art.colors.iter().map(|&c| oklab(c)[0]).collect();
    let half = cell as f32 / 2.;
    let full = half * (1. - look.gutter.clamp(0., 0.9));

    for ay in 0..art.h {
        for ax in 0..art.w {
            let at = (ay * art.w + ax) as usize;
            let i = art.index[at] as usize;
            let (color, r) = if i == paper {
                match contrast {
                    Some(c) if look.lattice > 0. && c != paper => (lut[c], half * look.lattice.clamp(0., 1.)),
                    _ => continue,
                }
            } else {
                let mut r = full;
                if look.modulate {
                    // How much of this colour the photo had here, as area.
                    let span = (color_l[i] - paper_l).abs().max(0.05);
                    let level = ((art.lum[at] - paper_l).abs() / span).clamp(0.15, 1.);
                    r *= level.sqrt();
                }
                (ink_of(i as u8), r)
            };
            for cy in 0..cell {
                let dy = cy as f32 + 0.5 - half;
                let row = ((ay * cell + cy) * w + ax * cell) as usize * 4;
                for cx in 0..cell {
                    let dx = cx as f32 + 0.5 - half;
                    // The centre pixel always draws, so 1× and 2× keep their pixels.
                    if look.shape.contains(dx, dy, r) || (dx.abs() < 0.5 && dy.abs() < 0.5) {
                        out[row + cx as usize * 4..][..4].copy_from_slice(&color);
                    }
                }
            }
        }
    }
    (w, h, out)
}

/// The same pixels as BGRA, for a GPU texture.
pub fn bgra(art: &Art, cell: u32, look: Look) -> (u32, u32, Vec<u8>) {
    let (w, h, mut px) = rgba(art, cell, look);
    for p in px.chunks_exact_mut(4) {
        p.swap(0, 2);
    }
    (w, h, px)
}

/// A PNG at `scale` pixels per art pixel; transparent paper keeps alpha.
pub fn png(art: &Art, scale: u32, look: Look) -> Result<Vec<u8>, String> {
    let (w, h, px) = rgba(art, scale, look);
    let img = image::RgbaImage::from_raw(w, h, px).ok_or("image size mismatch")?;
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

/// The untouched photo at the art's size, BGRA at `cell`: the "before".
pub fn before_bgra(print: &Print, art_w: u32, art_h: u32, cell: u32) -> (u32, u32, Vec<u8>) {
    let small = print.resize(art_w, art_h);
    let cell = cell.max(1);
    let (w, h) = (art_w * cell, art_h * cell);
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let [r, g, b, a] = rgba8(small.rgb[((y / cell) * art_w + x / cell) as usize]);
            out.extend_from_slice(&[b, g, r, a]);
        }
    }
    (w, h, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAPER: Rgb = [0., 0., 0.];
    const INK: Rgb = [1., 1., 1.];

    /// A 2×1 art: paper then ink, with full tone on the ink.
    fn art() -> Art {
        Art { w: 2, h: 1, index: vec![0, 1], colors: vec![PAPER, INK], lum: vec![0., 1.] }
    }

    fn at(px: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
        px[((y * w + x) * 4) as usize..][..4].try_into().unwrap()
    }

    #[test]
    fn plain_is_blocks() {
        let (w, h, px) = rgba(&art(), 4, Look::default());
        assert_eq!((w, h), (8, 4));
        assert_eq!(at(&px, w, 0, 0), [0, 0, 0, 255]);
        assert_eq!(at(&px, w, 7, 3), [255, 255, 255, 255]);
    }

    #[test]
    fn gutter_leaves_paper_round_shapes() {
        let look = Look { shape: Shape::Circle, gutter: 0.5, ..Look::default() };
        let (w, _, px) = rgba(&art(), 8, look);
        // The ink cell's centre is ink, its corner is paper.
        assert_eq!(at(&px, w, 12, 4), [255, 255, 255, 255]);
        assert_eq!(at(&px, w, 8, 0), [0, 0, 0, 255]);
    }

    #[test]
    fn shapes_differ() {
        let ink_count = |shape| {
            let (_, _, px) = rgba(&art(), 12, Look { shape, gutter: 0.1, ..Look::default() });
            px.chunks_exact(4).filter(|p| p[0] == 255).count()
        };
        let (sq, ci, di, pl) = (ink_count(Shape::Square), ink_count(Shape::Circle), ink_count(Shape::Diamond), ink_count(Shape::Plus));
        // Areas: square 4r², circle πr², plus 20r²/9, diamond 2r².
        assert!(sq > ci && ci > pl && pl > di, "{sq} {ci} {di} {pl}");
    }

    #[test]
    fn transparent_paper_is_clear() {
        let (w, _, px) = rgba(&art(), 2, Look { transparent: true, ..Look::default() });
        assert_eq!(at(&px, w, 0, 0)[3], 0);
        assert_eq!(at(&px, w, 3, 1)[3], 255);
        let png = png(&art(), 2, Look { transparent: true, ..Look::default() }).unwrap();
        let img = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(img.get_pixel(0, 0).0[3], 0);
    }

    #[test]
    fn modulate_shrinks_weak_tones() {
        let mut weak = art();
        weak.lum = vec![0., 0.3];
        let look = Look { shape: Shape::Square, modulate: true, ..Look::default() };
        let ink = |a: &Art| rgba(a, 10, look).2.chunks_exact(4).filter(|p| p[0] == 255).count();
        assert!(ink(&weak) < ink(&art()));
    }

    #[test]
    fn lattice_dots_the_paper() {
        let look = Look { shape: Shape::Circle, lattice: 0.4, ..Look::default() };
        let (w, _, px) = rgba(&art(), 10, look);
        // The paper cell gets a small dot at its centre, paper at its corner.
        assert_eq!(at(&px, w, 5, 5), [255, 255, 255, 255]);
        assert_eq!(at(&px, w, 0, 0), [0, 0, 0, 255]);
    }

    #[test]
    fn paper_choice() {
        let colors = [[0.5, 0.5, 0.5], [1., 1., 1.], [0., 0., 0.]];
        assert_eq!(paper_index(&colors, Paper::First), 0);
        assert_eq!(paper_index(&colors, Paper::Lightest), 1);
        assert_eq!(paper_index(&colors, Paper::Darkest), 2);
    }

    #[test]
    fn one_pixel_cells_keep_every_pixel() {
        let look = Look { shape: Shape::Plus, gutter: 0.8, ..Look::default() };
        let (_, _, px) = rgba(&art(), 1, look);
        assert_eq!(px, vec![0, 0, 0, 255, 255, 255, 255, 255]);
    }

    #[test]
    fn keys_round_trip() {
        for s in Shape::ALL {
            assert_eq!(Shape::from_key(s.key()), Some(s));
        }
        for p in Paper::ALL {
            assert_eq!(Paper::from_key(p.key()), Some(p));
        }
    }
}
