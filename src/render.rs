//! How a developed print is drawn: each art pixel becomes a cell of
//! `cell`×`cell` image pixels holding a shape (square, circle, diamond,
//! plus) on the paper, with an optional gutter between cells, dots that
//! grow with the tone (halftone without a halftone algorithm), a
//! transparent paper, and a lattice of dots over the bare paper.
//!
//! Shapes are hard-edged on purpose: pixel art, not anti-aliased vector
//! art. Pure, so the preview and the PNG export draw identical pixels.

use crate::engine::{Rgb, oklab};
use crate::export::{self, Format, Frame, Size};
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Paper {
    /// The palette's first colour: the scheme's background for Scheme.
    #[default]
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

/// How one layer's cells are drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cells {
    pub shape: Shape,
    /// 0..0.9: the share of each cell left as gap.
    pub gutter: f32,
    /// Shapes shrink where the photo had less of their colour.
    pub modulate: bool,
    /// 0..1: a dot of this size, in the colour furthest from the paper, on
    /// every bare paper cell. 0 = off.
    pub lattice: f32,
}

impl Default for Cells {
    fn default() -> Self {
        Cells { shape: Shape::Square, gutter: 0., modulate: false, lattice: 0. }
    }
}

impl Cells {
    /// Plain squares edge to edge: every cell is one solid block.
    fn is_plain(&self) -> bool {
        self.shape == Shape::Square && self.gutter <= 0. && !self.modulate && self.lattice <= 0.
    }

    fn bits(&self) -> (Shape, u32, bool, u32) {
        (self.shape, self.gutter.to_bits(), self.modulate, self.lattice.to_bits())
    }
}

/// How a print is drawn: the subject's cells, the background layer's cells
/// (used where a mask put the background), and the paper.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Look {
    pub cells: Cells,
    pub bg: Cells,
    pub paper: Paper,
    /// The paper is left transparent (PNG exports; the preview shows the
    /// easel through it).
    pub transparent: bool,
}

impl Look {
    /// One cell look for both layers.
    #[cfg(test)]
    pub fn of(cells: Cells) -> Look {
        Look { cells, bg: cells, ..Look::default() }
    }

    /// For cache keys.
    pub fn bits(&self) -> impl std::hash::Hash + use<> {
        (self.cells.bits(), self.bg.bits(), self.paper, self.transparent)
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

/// One art pixel's mark: its colour (and palette index), its shape, and
/// its radius as a share of half the cell (1 = touching the cell's edges).
struct Mark {
    color: [u8; 4],
    index: usize,
    shape: Shape,
    r: f32,
}

/// Every art pixel's mark in reading order (`None`: bare paper), and the
/// paper's index. The raster and the SVG both draw from these.
fn marks(art: &Art, look: Look) -> (usize, Vec<Option<Mark>>) {
    let lut: Vec<[u8; 4]> = art.colors.iter().map(|&c| rgba8(c)).collect();
    let paper = paper_index(&art.colors, look.paper);
    let ink_of = |i: usize| if look.transparent && i == paper { [0; 4] } else { lut[i] };
    let layered = !art.layer.is_empty();
    // The lattice is drawn in the colour furthest from the paper.
    let paper_l = oklab(art.colors[paper])[0];
    let contrast = (0..art.colors.len()).max_by(|&a, &b| {
        let d = |i: usize| (oklab(art.colors[i])[0] - paper_l).abs();
        d(a).total_cmp(&d(b))
    });
    let color_l: Vec<f32> = art.colors.iter().map(|&c| oklab(c)[0]).collect();
    let marks = (0..art.index.len())
        .map(|at| {
            let i = art.index[at] as usize;
            let cl = if layered && art.layer[at] == 1 { look.bg } else { look.cells };
            if i == paper {
                return match contrast {
                    Some(c) if cl.lattice > 0. && c != paper => Some(Mark { color: lut[c], index: c, shape: cl.shape, r: cl.lattice.clamp(0., 1.) }),
                    _ => None,
                };
            }
            let mut r = 1. - cl.gutter.clamp(0., 0.9);
            if cl.modulate {
                // How much of this colour the photo had here, as area.
                let span = (color_l[i] - paper_l).abs().max(0.05);
                let level = ((art.lum[at] - paper_l).abs() / span).clamp(0.15, 1.);
                r *= level.sqrt();
            }
            Some(Mark { color: ink_of(i), index: i, shape: cl.shape, r })
        })
        .collect();
    (paper, marks)
}

/// Draw `art` at `cell` pixels per art pixel. Returns `(w, h, rgba)`.
pub fn rgba(art: &Art, cell: u32, look: Look) -> (u32, u32, Vec<u8>) {
    let cell = cell.max(1);
    rgba_sized(art, art.w * cell, art.h * cell, look)
}

/// Draw `art` into exactly `w`×`h` pixels. Cells take whole pixels, so at a
/// size that isn't a multiple of the art some are a pixel wider than
/// others; shapes stay hard-edged. Returns `(w, h, rgba)`.
pub fn rgba_sized(art: &Art, w: u32, h: u32, look: Look) -> (u32, u32, Vec<u8>) {
    let (w, h) = (w.max(1), h.max(1));
    let lut: Vec<[u8; 4]> = art.colors.iter().map(|&c| rgba8(c)).collect();
    let paper = paper_index(&art.colors, look.paper);
    let paper_px = if look.transparent { [0; 4] } else { lut[paper] };
    // Where art pixel `a` of `n` starts, `out` pixels across.
    let edge = |a: u32, n: u32, out: u32| (a as u64 * out as u64 / n as u64) as u32;

    let mut out = vec![0u8; w as usize * h as usize * 4];
    let layered = !art.layer.is_empty();
    if look.cells.is_plain() && (!layered || look.bg.is_plain()) {
        let ink_of = |i: u8| if look.transparent && i as usize == paper { [0; 4] } else { lut[i as usize] };
        // Which art pixel each output pixel falls in, cut where the cells are.
        let spans = |n: u32, out: u32| -> Vec<u32> { (0..n).flat_map(|a| std::iter::repeat_n(a, (edge(a + 1, n, out) - edge(a, n, out)) as usize)).collect() };
        let (cols, rows) = (spans(art.w, w), spans(art.h, h));
        for y in 0..h {
            let ay = rows[y as usize];
            let row = &art.index[(ay * art.w) as usize..][..art.w as usize];
            let line = &mut out[y as usize * w as usize * 4..][..w as usize * 4];
            for (x, px) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                *px = ink_of(row[cols[x] as usize]);
            }
        }
        return (w, h, out);
    }

    for px in out.as_chunks_mut::<4>().0 {
        *px = paper_px;
    }
    let (_, marks) = marks(art, look);
    for ay in 0..art.h {
        let (y0, y1) = (edge(ay, art.h, h), edge(ay + 1, art.h, h));
        for ax in 0..art.w {
            let Some(m) = &marks[(ay * art.w + ax) as usize] else { continue };
            let (x0, x1) = (edge(ax, art.w, w), edge(ax + 1, art.w, w));
            let (half_w, half_h) = ((x1 - x0) as f32 / 2., (y1 - y0) as f32 / 2.);
            let r = half_w.min(half_h) * m.r;
            for y in y0..y1 {
                let dy = (y - y0) as f32 + 0.5 - half_h;
                let row = y as usize * w as usize * 4;
                for x in x0..x1 {
                    let dx = (x - x0) as f32 + 0.5 - half_w;
                    // The centre pixel always draws, so 1× and 2× keep their pixels.
                    if m.shape.contains(dx, dy, r) || (dx.abs() < 0.5 && dy.abs() < 0.5) {
                        out[row + x as usize * 4..][..4].copy_from_slice(&m.color);
                    }
                }
            }
        }
    }
    (w, h, out)
}

/// The print as an SVG `w`×`h` pixels big, drawn in art-pixel units with
/// one path per colour, so it stays sharp at any size.
pub fn svg(art: &Art, w: u32, h: u32, look: Look) -> String {
    use std::fmt::Write as _;
    let hex = |c: [u8; 4]| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
    let num = |v: f32| {
        let t = format!("{v:.3}");
        t.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    let (paper, marks) = marks(art, look);
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {} {}\" preserveAspectRatio=\"none\">\n",
        art.w, art.h
    );
    if !look.transparent {
        let _ = writeln!(s, "<rect width=\"{}\" height=\"{}\" fill=\"{}\"/>", art.w, art.h, hex(rgba8(art.colors[paper])));
    }
    let full = |m: &Mark| m.shape == Shape::Square && m.r >= 1.;
    let mut paths = vec![String::new(); art.colors.len()];
    for ay in 0..art.h {
        let mut ax = 0;
        while ax < art.w {
            let at = (ay * art.w + ax) as usize;
            let Some(m) = marks[at].as_ref().filter(|m| m.color[3] > 0) else {
                ax += 1;
                continue;
            };
            let d = &mut paths[m.index];
            if full(m) {
                // A run of full squares in one colour is one rectangle.
                let mut end = ax + 1;
                while end < art.w && marks[at + (end - ax) as usize].as_ref().is_some_and(|o| o.index == m.index && full(o)) {
                    end += 1;
                }
                let _ = write!(d, "M{ax} {ay}h{}v1h-{}z", end - ax, end - ax);
                ax = end;
                continue;
            }
            let (cx, cy, r) = (ax as f32 + 0.5, ay as f32 + 0.5, 0.5 * m.r);
            let _ = match m.shape {
                Shape::Square => write!(d, "M{} {}h{d2}v{d2}h-{d2}z", num(cx - r), num(cy - r), d2 = num(2. * r)),
                Shape::Circle => write!(d, "M{} {}a{r} {r} 0 1 0 {d2} 0a{r} {r} 0 1 0 -{d2} 0z", num(cx - r), num(cy), r = num(r), d2 = num(2. * r)),
                Shape::Diamond => write!(d, "M{} {}l{r} {r}l-{r} {r}l-{r} -{r}z", num(cx), num(cy - r), r = num(r)),
                Shape::Plus => {
                    let (t, l) = (num(2. * r / 3.), num(2. * r));
                    write!(d, "M{} {}h{t}v{l}h-{t}zM{} {}h{l}v{t}h-{l}z", num(cx - r / 3.), num(cy - r), num(cx - r), num(cy - r / 3.))
                }
            };
            ax += 1;
        }
    }
    for (i, d) in paths.iter().enumerate().filter(|(_, d)| !d.is_empty()) {
        let _ = writeln!(s, "<path fill=\"{}\" d=\"{d}\"/>", hex(rgba8(art.colors[i])));
    }
    s.push_str("</svg>\n");
    s
}

/// The same pixels as BGRA, for a GPU texture.
pub fn bgra(art: &Art, cell: u32, look: Look) -> (u32, u32, Vec<u8>) {
    let (w, h, mut px) = rgba(art, cell, look);
    for p in px.as_chunks_mut::<4>().0 {
        p.swap(0, 2);
    }
    (w, h, px)
}

/// A PNG at `scale` pixels per art pixel; transparent paper keeps alpha.
#[cfg(test)]
pub fn png(art: &Art, scale: u32, look: Look) -> Result<Vec<u8>, String> {
    let (w, h, px) = rgba(art, scale, look);
    export::encode(w, h, px, Format::Png, [1.; 3])
}

/// A still in any format at `size`, in `frame`. Formats without alpha lay
/// transparent paper on the paper colour.
pub fn still(art: &Art, size: Size, frame: Frame, scale: u32, format: Format, look: Look) -> Result<Vec<u8>, String> {
    let paper = art.colors[paper_index(&art.colors, look.paper)];
    let ((w, h), rect) = frame.canvas(art.w, art.h, size, scale);
    if format == Format::Svg {
        let inner = svg(art, rect[2], rect[3], look);
        if rect == [0, 0, w, h] {
            return Ok(inner.into_bytes());
        }
        let back = if look.transparent { String::new() } else { format!("<rect width=\"{w}\" height=\"{h}\" fill=\"{}\"/>
", export::hex(paper)) };
        let inner = inner.replacen("<svg ", &format!("<svg x=\"{}\" y=\"{}\" ", rect[0], rect[1]), 1);
        return Ok(format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\">
{back}{inner}</svg>
").into_bytes());
    }
    let (w, h, px) = framed(art, size, frame, scale, look);
    export::encode(w, h, px, format, paper)
}

/// The print's pixels at `size` in `frame`: RGBA, padded with paper.
pub fn framed(art: &Art, size: Size, frame: Frame, scale: u32, look: Look) -> (u32, u32, Vec<u8>) {
    let ((w, h), rect) = frame.canvas(art.w, art.h, size, scale);
    let (_, _, px) = rgba_sized(art, rect[2], rect[3], look);
    let pad = if look.transparent { [0; 4] } else { rgba8(art.colors[paper_index(&art.colors, look.paper)]) };
    (w, h, export::place(px, (w, h), rect, pad))
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
        Art { w: 2, h: 1, index: vec![0, 1], colors: vec![PAPER, INK], lum: vec![0., 1.], layer: vec![] }
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
        let look = Look::of(Cells { shape: Shape::Circle, gutter: 0.5, ..Cells::default() });
        let (w, _, px) = rgba(&art(), 8, look);
        // The ink cell's centre is ink, its corner is paper.
        assert_eq!(at(&px, w, 12, 4), [255, 255, 255, 255]);
        assert_eq!(at(&px, w, 8, 0), [0, 0, 0, 255]);
    }

    #[test]
    fn shapes_differ() {
        let ink_count = |shape| {
            let (_, _, px) = rgba(&art(), 12, Look::of(Cells { shape, gutter: 0.1, ..Cells::default() }));
            px.as_chunks::<4>().0.iter().filter(|p| p[0] == 255).count()
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
        let look = Look::of(Cells { shape: Shape::Square, modulate: true, ..Cells::default() });
        let ink = |a: &Art| rgba(a, 10, look).2.as_chunks::<4>().0.iter().filter(|p| p[0] == 255).count();
        assert!(ink(&weak) < ink(&art()));
    }

    #[test]
    fn lattice_dots_the_paper() {
        let look = Look::of(Cells { shape: Shape::Circle, lattice: 0.4, ..Cells::default() });
        let (w, _, px) = rgba(&art(), 10, look);
        // The paper cell gets a small dot at its centre, paper at its corner.
        assert_eq!(at(&px, w, 5, 5), [255, 255, 255, 255]);
        assert_eq!(at(&px, w, 0, 0), [0, 0, 0, 255]);
    }

    #[test]
    fn layers_draw_their_own_cells() {
        // Two ink pixels; the second is background, drawn as a small circle.
        let mut a = Art { w: 2, h: 1, index: vec![1, 1], colors: vec![PAPER, INK], lum: vec![1., 1.], layer: vec![0, 1] };
        let look = Look { bg: Cells { shape: Shape::Circle, gutter: 0.6, ..Cells::default() }, ..Look::default() };
        let (w, _, px) = rgba(&a, 10, look);
        assert_eq!(at(&px, w, 0, 0), [255, 255, 255, 255], "subject: a full square");
        assert_eq!(at(&px, w, 10, 0), [0, 0, 0, 255], "background: a small dot, corner bare");
        assert_eq!(at(&px, w, 15, 5), [255, 255, 255, 255]);
        // Without layers, the background look is ignored.
        a.layer.clear();
        assert_eq!(at(&rgba(&a, 10, look).2, w, 10, 0), [255, 255, 255, 255]);
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
        let look = Look::of(Cells { shape: Shape::Plus, gutter: 0.8, ..Cells::default() });
        let (_, _, px) = rgba(&art(), 1, look);
        assert_eq!(px, vec![0, 0, 0, 255, 255, 255, 255, 255]);
    }

    #[test]
    fn sized_matches_whole_cells() {
        let look = Look::of(Cells { shape: Shape::Circle, gutter: 0.3, modulate: true, lattice: 0.2 });
        assert_eq!(rgba(&art(), 6, look), rgba_sized(&art(), 12, 6, look));
    }

    #[test]
    fn sized_fills_odd_sizes() {
        // Two art pixels across seven: cells of three and four.
        let (w, h, px) = rgba_sized(&art(), 7, 3, Look::default());
        assert_eq!((w, h), (7, 3));
        assert_eq!(at(&px, w, 2, 1), [0, 0, 0, 255]);
        assert_eq!(at(&px, w, 3, 1), [255, 255, 255, 255]);
    }

    #[test]
    fn svg_draws_each_colour() {
        let s = svg(&art(), 200, 100, Look::default());
        assert!(s.contains("width=\"200\"") && s.contains("viewBox=\"0 0 2 1\""));
        assert!(s.contains("<rect width=\"2\" height=\"1\" fill=\"#000000\"/>"));
        assert!(s.contains("<path fill=\"#ffffff\" d=\"M1 0h1v1h-1z\"/>"), "{s}");
        let dots = svg(&art(), 20, 10, Look::of(Cells { shape: Shape::Circle, gutter: 0.5, ..Cells::default() }));
        assert!(dots.contains("a0.25 0.25"), "{dots}");
        assert!(!svg(&art(), 2, 1, Look { transparent: true, ..Look::default() }).contains("<rect"));
    }

    #[test]
    fn stills_in_every_format() {
        for f in Format::ALL {
            let bytes = still(&art(), Size::Long(64), Frame::default(), 1, f, Look::default()).unwrap();
            if f != Format::Svg {
                let img = image::load_from_memory(&bytes).unwrap();
                assert_eq!((img.width(), img.height()), (64, 32), "{f:?}");
            }
        }
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
