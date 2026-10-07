//! The darkroom's process, all pure: a photo becomes a colour working print,
//! the print is adjusted, then it's developed either as dither (the
//! [`engine`](crate::engine), into any palette) or as ASCII (Ferrite's
//! glyph-fitted character sets). Preview and export run the same code, so
//! what you see is what you save.

use std::path::Path;

use ferrite_design::ascii::{self, ArtStyle, Charset, Fit};
use ferrite_design::dither::Picture;

use crate::engine::{self, Params, Rgb};
use crate::mask;
use crate::recipe::{Background, Recipe};
use crate::render;

/// The working print is at most this wide; more detail than any export uses.
const WORK_W: u32 = 960;

/// The working print: sRGB colour plus its luminance (0 = black, 1 = white).
#[derive(Clone, Debug, PartialEq)]
pub struct Print {
    pub w: u32,
    pub h: u32,
    pub rgb: Vec<Rgb>,
    pub lum: Vec<f32>,
}

fn luminance([r, g, b]: Rgb) -> f32 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

impl Print {
    pub fn from_rgb(w: u32, h: u32, rgb: Vec<Rgb>) -> Print {
        let lum = rgb.iter().map(|&c| luminance(c)).collect();
        Print { w, h, rgb, lum }
    }

    pub fn aspect(&self) -> f32 {
        self.h as f32 / self.w.max(1) as f32
    }

    /// Area-averaged resample to `w`×`h`.
    pub fn resize(&self, w: u32, h: u32) -> Print {
        let (w, h) = (w.max(1), h.max(1));
        let (sx, sy) = (self.w as f32 / w as f32, self.h as f32 / h as f32);
        let mut rgb = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            let (y0, y1) = ((y as f32 * sy) as u32, (((y + 1) as f32 * sy).ceil() as u32).clamp(1, self.h));
            for x in 0..w {
                let (x0, x1) = ((x as f32 * sx) as u32, (((x + 1) as f32 * sx).ceil() as u32).clamp(1, self.w));
                let (mut sum, mut n) = ([0f32; 3], 0f32);
                for yy in y0.min(y1 - 1)..y1 {
                    for xx in x0.min(x1 - 1)..x1 {
                        let c = self.rgb[(yy * self.w + xx) as usize];
                        sum = [sum[0] + c[0], sum[1] + c[1], sum[2] + c[2]];
                        n += 1.;
                    }
                }
                let n = n.max(1.);
                rgb.push([sum[0] / n, sum[1] / n, sum[2] / n]);
            }
        }
        Print::from_rgb(w, h, rgb)
    }
}

/// Open a photo from disk as a working print.
pub fn load(path: &Path) -> Result<Print, String> {
    let img = image::open(path).map_err(|e| match e {
        image::ImageError::Unsupported(_) => "that isn't an image Darkroom can read (PNG, JPEG, GIF, BMP, WebP)".to_string(),
        image::ImageError::IoError(e) => e.to_string(),
        e => format!("couldn't read the image: {e}"),
    })?;
    let img = img.to_rgb8();
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return Err("the image is empty".into());
    }
    let print = Print::from_rgb(w, h, img.pixels().map(|p| p.0.map(|c| c as f32 / 255.)).collect());
    Ok(if w > WORK_W { print.resize(WORK_W, ((h as f32 / w as f32) * WORK_W as f32).round() as u32) } else { print })
}

/// A test print to work on before you've brought a photo: a lit sphere on
/// a horizon under a low sun — smooth ramps, hard edges, and some colour
/// for the palettes to find.
pub fn sample() -> Print {
    let (w, h) = (640u32, 400u32);
    let rgb = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x as f32 / w as f32, y as f32 / h as f32)))
        .map(|(u, v)| {
            let horizon = 0.62;
            // Sky: dark overhead, a glow around a low sun.
            let (sun_u, sun_v) = (0.76, 0.5);
            let d_sun = ((u - sun_u).powi(2) + ((v - sun_v) * 0.62).powi(2)).sqrt();
            let glow = (-d_sun * 7.).exp();
            let sky = v < horizon;
            let sun = d_sun < 0.05 && sky;
            let mut l = if sky { 0.06 + 0.3 * v / horizon + 0.6 * glow } else { 0.1 + 0.2 * (1. - (v - horizon) / (1. - horizon)) };
            if sun {
                l = 1.;
            }
            // Ground stripes running to the horizon.
            if !sky && (((u - 0.5) / (v - horizon + 0.05)) * 2.).rem_euclid(1.) < 0.12 {
                l += 0.18;
            }
            // Tint: a blue sky warming to orange around the sun, green ground.
            let mut tint = if sky { [0.75 + 0.5 * glow, 0.9, 1.25 - 0.7 * glow] } else { [0.8, 1.15, 0.7] };
            if sun {
                tint = [1.; 3];
            }
            // The sphere, lit from the sun's side.
            let (cx, cy, r) = (0.32, 0.52, 0.2);
            let (dx, dy) = ((u - cx) / r, (v - cy) / (r * 1.6));
            let d2 = dx * dx + dy * dy;
            if d2 < 1. {
                let dz = (1. - d2).sqrt();
                let (lx, ly, lz) = (0.62, -0.3, 0.72);
                l = 0.04 + 0.96 * (dx * lx + dy * ly + dz * lz).max(0.).powf(1.4);
                tint = [1.3, 0.75, 0.6];
            }
            tint.map(|t| (l * t).clamp(0., 1.))
        })
        .collect();
    Print::from_rgb(w, h, rgb)
}

/// How the print is adjusted before it's developed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Adjust {
    /// -1..1
    pub brightness: f32,
    /// 0.25..3, 1 = unchanged
    pub contrast: f32,
    /// 0.2..5, 1 = unchanged; above 1 lifts the midtones.
    pub gamma: f32,
    pub invert: bool,
    /// Whether the ink is lighter than the paper (a dark scheme): then the
    /// light parts of the photo get the ink. ASCII only; a dither's palette
    /// decides this itself.
    pub light_ink: bool,
}

impl Default for Adjust {
    fn default() -> Self {
        Adjust { brightness: 0., contrast: 1., gamma: 1., invert: false, light_ink: true }
    }
}

impl Adjust {
    /// One channel (or a luminance) through brightness, contrast and gamma.
    fn tone(&self, v: f32) -> f32 {
        let v = ((v - 0.5) * self.contrast + 0.5 + self.brightness * 0.5).clamp(0., 1.);
        v.powf(1. / self.gamma.max(0.05))
    }

    /// A colour through the tone controls and invert.
    pub fn color(&self, c: Rgb) -> Rgb {
        c.map(|v| {
            let v = self.tone(v);
            if self.invert { 1. - v } else { v }
        })
    }
}

/// Ink levels (0 = paper, 255 = ink) for a print, quantised once so the
/// preview and the export see identical numbers.
pub fn ink_levels(print: &Print, adjust: Adjust) -> Vec<u8> {
    print
        .lum
        .iter()
        .map(|&l| {
            let l = adjust.tone(l);
            let ink = if adjust.light_ink != adjust.invert { l } else { 1. - l };
            (ink * 255.).round() as u8
        })
        .collect()
}

/// Rows for `cols` columns of square pixels.
pub fn rows_for(print: &Print, cols: u32) -> u32 {
    ((cols as f32 * print.aspect()).round() as u32).max(1)
}

/// A developed print: one palette index per pixel, the palette, the
/// photo's Oklab lightness per pixel (for shapes sized by tone), and which
/// layer each pixel belongs to (0 subject, 1 background; empty when there
/// is no mask).
#[derive(Clone, Debug, PartialEq)]
pub struct Art {
    pub w: u32,
    pub h: u32,
    pub index: Vec<u8>,
    pub colors: Vec<Rgb>,
    pub lum: Vec<f32>,
    pub layer: Vec<u8>,
}

/// Develop the print at `cols`×rows into `palette` with bare dither
/// settings: no mask, no frame before. Tests use it; the app develops by
/// recipe ([`develop_recipe`]).
#[cfg(test)]
pub fn develop(print: &Print, cols: u32, adjust: Adjust, palette: Vec<Rgb>, params: Params) -> Art {
    let (w, h, pixels, lum) = prepare(print, cols, adjust);
    let index = dither_held(&pixels, &lum, w, h, &palette, params, None);
    Art { w, h, index, colors: palette, lum, layer: Vec::new() }
}

/// Develop by a whole recipe: the subject everywhere, and where the mask
/// says background, the background layer (the subject again, bare paper,
/// or its own dither). `paint` is the photo's painted mask, if any.
pub fn develop_recipe(print: &Print, r: &Recipe, adjust: Adjust, palette: Vec<Rgb>, paint: Option<&mask::Paint>, prev: Option<(&Art, f32)>) -> Art {
    let (w, h, pixels, lum) = prepare(print, r.cols, adjust);
    let subject = dither_held(&pixels, &lum, w, h, &palette, r.params(), prev);
    if !r.masked() {
        return Art { w, h, index: subject, colors: palette, lum, layer: Vec::new() };
    }
    let painted = paint.map(|p| p.resize(w, h));
    let soft = mask::compute(&r.mask, &pixels, w, h, painted.as_deref());
    let chosen = mask::select(&soft, w);
    let background = match r.background {
        Background::Same => subject.clone(),
        Background::Paper => vec![render::paper_index(&palette, r.paper) as u8; subject.len()],
        Background::Own => dither_held(&pixels, &lum, w, h, &palette, r.bg_params(), prev),
    };
    let index = chosen.iter().zip(subject.iter().zip(&background)).map(|(&s, (&a, &b))| if s { a } else { b }).collect();
    let layer = chosen.iter().map(|&s| if s { 0 } else { 1 }).collect();
    Art { w, h, index, colors: palette, lum, layer }
}

/// The mask view: the adjusted photo at art size with the background dimmed,
/// BGRA at `cell` pixels per art pixel.
pub fn mask_bgra(print: &Print, r: &Recipe, adjust: Adjust, paint: Option<&mask::Paint>, cell: u32) -> (u32, u32, Vec<u8>) {
    let (w, h, pixels, _) = prepare(print, r.cols, adjust);
    let painted = paint.map(|p| p.resize(w, h));
    let soft = if r.mask.kind == mask::Kind::None { vec![1.; pixels.len()] } else { mask::compute(&r.mask, &pixels, w, h, painted.as_deref()) };
    let cell = cell.max(1);
    let (ow, oh) = (w * cell, h * cell);
    let mut out = Vec::with_capacity((ow * oh * 4) as usize);
    for y in 0..oh {
        for x in 0..ow {
            let at = ((y / cell) * w + x / cell) as usize;
            // Background: dimmed to a quarter and greyed, so the subject stands out.
            let keep = 0.25 + 0.75 * soft[at];
            let c = pixels[at];
            let grey = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
            let [r, g, b] = c.map(|v| ((v * soft[at] + grey * (1. - soft[at])) * keep * 255.).round().clamp(0., 255.) as u8);
            out.extend_from_slice(&[b, g, r, 255]);
        }
    }
    (ow, oh, out)
}

/// The print at `cols` wide, adjusted, and its Oklab lightness.
fn prepare(print: &Print, cols: u32, adjust: Adjust) -> (u32, u32, Vec<Rgb>, Vec<f32>) {
    let small = print.resize(cols, rows_for(print, cols));
    let pixels: Vec<Rgb> = small.rgb.iter().map(|&c| adjust.color(c)).collect();
    let lum: Vec<f32> = pixels.iter().map(|&c| engine::oklab(c)[0]).collect();
    (small.w, small.h, pixels, lum)
}

fn dither_held(pixels: &[Rgb], lum: &[f32], w: u32, h: u32, palette: &[Rgb], params: Params, prev: Option<(&Art, f32)>) -> Vec<u8> {
    match prev {
        Some((prev, margin)) if prev.index.len() == pixels.len() => {
            let still: Vec<bool> = lum.iter().zip(&prev.lum).map(|(a, b)| (a - b).abs() < STILL).collect();
            let hold = engine::Hold { prev: &prev.index, still: &still, margin };
            engine::dither_held(pixels, w, h, palette, params, Some(&hold))
        }
        _ => engine::dither(pixels, w, h, palette, params),
    }
}

/// A pixel whose lightness moved less than this between frames stood still.
const STILL: f32 = 0.02;

/// The print as text, `cols` characters wide.
pub fn ascii_lines(print: &Print, cols: usize, adjust: Adjust, charset: Charset, fit: Fit) -> std::rc::Rc<Vec<String>> {
    // Fitting samples sub-character detail: give it ~8 samples per column.
    let w = (cols as u32 * 8).min(print.w).max(1);
    let small = print.resize(w, rows_for(print, w));
    let pic = Picture::new(small.w, small.h, ink_levels(&small, adjust));
    let style = ArtStyle { fit, ..ArtStyle::new(charset) };
    ascii::picture_art(&pic, cols, style)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Algo;

    const INK: Rgb = [1., 0.66, 0.23];
    const PAPER: Rgb = [0.07, 0.07, 0.08];

    fn grey(w: u32, h: u32, lum: Vec<f32>) -> Print {
        Print::from_rgb(w, h, lum.into_iter().map(|l| [l; 3]).collect())
    }

    fn ramp(w: u32, h: u32) -> Print {
        grey(w, h, (0..h).flat_map(|_| (0..w).map(move |x| x as f32 / (w - 1) as f32)).collect())
    }

    #[test]
    fn resize_averages_area() {
        let p = grey(4, 2, vec![0., 1., 0., 1., 0., 1., 0., 1.]);
        let s = p.resize(2, 1);
        assert_eq!((s.w, s.h), (2, 1));
        assert!(s.lum.iter().all(|v| (v - 0.5).abs() < 1e-6), "{:?}", s.lum);
        let up = p.resize(8, 4);
        assert_eq!(up.lum.len(), 32);
    }

    #[test]
    fn ink_follows_the_scheme() {
        let p = grey(2, 1, vec![0., 1.]);
        // Dark scheme: light ink, so white in the photo is ink.
        assert_eq!(ink_levels(&p, Adjust::default()), vec![0, 255]);
        // Light scheme: dark ink, so black in the photo is ink.
        assert_eq!(ink_levels(&p, Adjust { light_ink: false, ..Adjust::default() }), vec![255, 0]);
        // Invert flips either.
        assert_eq!(ink_levels(&p, Adjust { invert: true, ..Adjust::default() }), vec![255, 0]);
    }

    #[test]
    fn brightness_contrast_gamma() {
        let p = grey(3, 1, vec![0.25, 0.5, 0.75]);
        let flat = ink_levels(&p, Adjust { contrast: 0.25, ..Adjust::default() });
        assert!(flat[2] - flat[0] < 40);
        let bright = ink_levels(&p, Adjust { brightness: 1., ..Adjust::default() });
        assert_eq!(bright[1], 255);
        let lifted = ink_levels(&p, Adjust { gamma: 2., ..Adjust::default() });
        assert!(lifted[1] > 170, "{lifted:?}");
    }

    #[test]
    fn develop_sizes_and_inks() {
        let art = develop(&ramp(64, 16), 64, Adjust::default(), vec![PAPER, INK], Params { algo: Algo::Atkinson, ..Params::default() });
        assert_eq!((art.w, art.h, art.index.len()), (64, 16, 64 * 16));
        let inked = |x0: usize, x1: usize| art.index.iter().enumerate().filter(|(i, v)| (x0..x1).contains(&(i % 64)) && **v == 1).count();
        assert!(inked(32, 64) > inked(0, 32) * 2);
        // Invert hands the dark end the ink.
        let inv = develop(&ramp(64, 16), 64, Adjust { invert: true, ..Adjust::default() }, vec![PAPER, INK], Params::default());
        assert!(inv.index[0] == 1 && inv.index[63] == 0);
    }

    #[test]
    fn png_is_scaled_and_in_the_palette() {
        let art = develop(&ramp(16, 8), 16, Adjust::default(), vec![PAPER, INK], Params { algo: Algo::Bayer4, ..Params::default() });
        assert_eq!(art.lum.len(), 16 * 8);
        let bytes = crate::render::png(&art, 4, crate::render::Look::default()).unwrap();
        let img = image::load_from_memory(&bytes).unwrap().to_rgb8();
        assert_eq!(img.dimensions(), (64, 32));
        let lut: Vec<[u8; 3]> = art.colors.iter().map(|c| c.map(|v| (v * 255.).round() as u8)).collect();
        assert!(img.pixels().all(|p| lut.contains(&p.0)));
        // A 4×4 block is one art pixel.
        assert_eq!(img.get_pixel(0, 0), img.get_pixel(3, 3));
    }

    #[test]
    fn a_mask_splits_subject_and_background() {
        use crate::mask::{Kind, Spec};
        // A dark disc on white: border mask, background knocked out to paper.
        let rgb = (0..40 * 40).map(|i| if ((i % 40) as f32 - 19.5).powi(2) + ((i / 40) as f32 - 19.5).powi(2) < 144. { [0.3; 3] } else { [1.; 3] }).collect();
        let print = Print::from_rgb(40, 40, rgb);
        let r = Recipe { cols: 40, algo: Algo::Bayer4, mask: Spec { kind: Kind::Border, feather: 0., ..Spec::default() }, background: Background::Paper, ..Recipe::default() };
        let art = develop_recipe(&print, &r, Adjust::default(), vec![PAPER, INK], None, None);
        assert_eq!(art.layer.len(), 1600);
        assert_eq!((art.layer[0], art.layer[20 * 40 + 20]), (1, 0));
        // The white backdrop would be all ink; as paper it's bare.
        assert!(art.index.iter().zip(&art.layer).filter(|(_, l)| **l == 1).all(|(i, _)| *i == 0));
        // Unmasked, there are no layers and the backdrop is inked.
        let plain = develop_recipe(&print, &Recipe { mask: Spec::default(), ..r.clone() }, Adjust::default(), vec![PAPER, INK], None, None);
        assert!(plain.layer.is_empty() && plain.index[0] == 1);
        // A painted mask with nothing painted leaves only background.
        let blank = crate::mask::Paint::blank(40, 40);
        let painted = develop_recipe(&print, &Recipe { mask: Spec { kind: Kind::Paint, ..r.mask }, ..r }, Adjust::default(), vec![PAPER, INK], Some(&blank), None);
        assert!(painted.layer.iter().all(|l| *l == 1));
    }

    /// A dark, shaded figure on an off-white backdrop: the shape of the
    /// reference images (an engraved knight, a photographed hand).
    fn figure() -> Print {
        let (w, h) = (160u32, 160u32);
        let rgb = (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as f32 / w as f32, (i / w) as f32 / h as f32);
                let d = ((x - 0.5).powi(2) + (y - 0.55).powi(2)).sqrt();
                if d < 0.28 {
                    let shade = 0.15 + 0.5 * (x + y) / 2.;
                    [shade * 0.8, shade * 0.85, shade]
                } else {
                    [0.96, 0.95, 0.93]
                }
            })
            .collect();
        Print::from_rgb(w, h, rgb)
    }

    fn bundled(name: &str) -> Recipe {
        let src = crate::recipe::BUNDLED.iter().find(|(n, _)| *n == name).unwrap().1;
        Recipe::from_toml(src).unwrap()
    }

    #[test]
    fn reference_cutout_is_one_ink_on_clean_paper() {
        let r = bundled("Cut-out");
        let colors = r.palette_colors(PAPER, INK);
        let art = develop_recipe(&figure(), &r, Adjust { brightness: r.brightness, contrast: r.contrast, gamma: r.gamma, invert: r.invert, light_ink: false }, colors.clone(), None, None);
        let paper = render::paper_index(&colors, r.paper) as u8;
        let (w, h) = (art.w as usize, art.h as usize);
        // The backdrop is bare paper all round: the corners and edges.
        for (x, y) in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1), (w / 2, 0)] {
            assert_eq!(art.index[y * w + x], paper, "backdrop at ({x}, {y})");
        }
        // The figure is drawn in the ink, densely.
        let core: Vec<u8> = (h / 2 - 10..h / 2 + 10).flat_map(|y| (w / 2 - 10..w / 2 + 10).map(move |x| (x, y))).map(|(x, y)| art.index[y * w + x]).collect();
        let inked = core.iter().filter(|&&i| i != paper).count();
        assert!(inked * 2 > core.len(), "{inked} of {}", core.len());
        // Two colours only: paper and one ink.
        assert_eq!(colors.len(), 2);
    }

    #[test]
    fn reference_lattice_is_a_solid_figure_on_a_regular_grid() {
        let r = bundled("Lattice");
        let colors = r.palette_colors(PAPER, INK);
        let adjust = Adjust { brightness: r.brightness, contrast: r.contrast, gamma: r.gamma, invert: r.invert, light_ink: true };
        let art = develop_recipe(&figure(), &r, adjust, colors.clone(), None, None);
        let (w, h) = (art.w as usize, art.h as usize);
        // The backdrop is a regular lattice: it repeats every four pixels,
        // and it is neither empty nor solid.
        // A near-white backdrop lights one pixel in each 4×4, so look at a
        // band four rows deep.
        let band = h / 8..h / 8 + 4;
        for y in band.clone() {
            let bg: Vec<u8> = (0..w).map(|x| art.index[y * w + x]).collect();
            assert!((0..w - 4).all(|x| bg[x] == bg[x + 4]), "row {y}: {bg:?}");
        }
        let cells: Vec<u8> = band.clone().flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| art.index[y * w + x]).collect();
        assert!(cells.contains(&0) && cells.contains(&1));
        let (lx, row) = band.clone().flat_map(|y| (0..w).map(move |x| (x, y))).find(|&(x, y)| art.index[y * w + x] == 1).unwrap();
        // The figure is one solid colour (the threshold puts it all on one side).
        let centre = art.index[(h / 2) * w + w / 2];
        assert!((h / 2 - 5..h / 2 + 5).all(|y| (w / 2 - 5..w / 2 + 5).all(|x| art.index[y * w + x] == centre)));
        // And drawn with the lattice's gutter, it renders as separate dots.
        let (pw, _, px) = render::rgba(&art, 6, r.look());
        let lit = |x: usize, y: usize| px[(y * pw as usize + x) * 4] > 128;
        let (cx, cy) = (lx * 6, row * 6);
        assert!(lit(cx + 3, cy + 3) && !lit(cx, cy), "a dot with a gap around it");
    }

    #[test]
    fn ascii_has_the_width_asked_for() {
        let lines = ascii_lines(&sample(), 60, Adjust::default(), Charset::Classic, Fit::Tone);
        assert!(lines.len() > 5);
        assert!(lines.iter().all(|l| l.chars().count() == 60));
    }

    #[test]
    fn sample_has_range() {
        let s = sample();
        let (lo, hi) = s.lum.iter().fold((1f32, 0f32), |(lo, hi), &v| (lo.min(v), hi.max(v)));
        assert!(lo < 0.15 && hi > 0.95);
    }
}
