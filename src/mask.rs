//! Masks: which art pixels are the subject (1) and which the background (0).
//!
//! - **Brightness:** a lightness range is the subject.
//! - **Colour:** pixels near a key colour (picked from the photo).
//! - **Border:** everything reachable from the image's edge without crossing
//!   a change of colour is background: automatic cut-outs for subjects on a
//!   plain backdrop (a product shot, an engraving on white).
//! - **Painted:** a brush mask the person paints, kept with the photo and
//!   saved or loaded as a greyscale PNG.
//!
//! Masks are soft (0..1); `feather` softens the edge further and the
//! develop step dithers the soft edge with blue noise, so a cut-out's edge
//! is a dither too rather than a jagged line. Pure.

use std::collections::VecDeque;

use crate::engine::{Rgb, oklab};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    None,
    Luma,
    Color,
    Border,
    Paint,
}

impl Kind {
    pub const ALL: [Kind; 5] = [Kind::None, Kind::Luma, Kind::Color, Kind::Border, Kind::Paint];

    pub fn key(self) -> &'static str {
        match self {
            Kind::None => "none",
            Kind::Luma => "brightness",
            Kind::Color => "colour",
            Kind::Border => "border",
            Kind::Paint => "painted",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::None => "None",
            Kind::Luma => "Brightness",
            Kind::Color => "Colour",
            Kind::Border => "Border (auto)",
            Kind::Paint => "Painted",
        }
    }

    pub fn from_key(key: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.key() == key)
    }
}

/// Everything about a mask except its pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spec {
    pub kind: Kind,
    /// Brightness: the subject's lightness range, 0..1.
    pub low: f32,
    pub high: f32,
    /// Colour: the key colour.
    pub color: Rgb,
    /// Colour and border: how far a colour may be and still match, 0..1.
    pub tolerance: f32,
    /// 0..1: how soft the edge is.
    pub feather: f32,
    pub invert: bool,
}

impl Default for Spec {
    fn default() -> Self {
        Spec { kind: Kind::None, low: 0.5, high: 1., color: [1., 1., 1.], tolerance: 0.15, feather: 0.2, invert: false }
    }
}

/// A painted mask, at the photo's working resolution.
#[derive(Clone, Debug, PartialEq)]
pub struct Paint {
    pub w: u32,
    pub h: u32,
    pub v: Vec<f32>,
}

impl Paint {
    pub fn blank(w: u32, h: u32) -> Paint {
        Paint { w, h, v: vec![0.; (w * h) as usize] }
    }

    /// Paint a round dab at `(x, y)` (0..1 across the image) of radius
    /// `r` (a share of the width): `on` paints subject, off erases.
    pub fn dab(&mut self, x: f32, y: f32, r: f32, on: bool) {
        let (cx, cy, rad) = (x * self.w as f32, y * self.h as f32, (r * self.w as f32).max(0.5));
        let (x0, x1) = (((cx - rad).floor().max(0.)) as u32, ((cx + rad).ceil() as u32).min(self.w));
        let (y0, y1) = (((cy - rad).floor().max(0.)) as u32, ((cy + rad).ceil() as u32).min(self.h));
        for py in y0..y1 {
            for px in x0..x1 {
                let d = ((px as f32 + 0.5 - cx).powi(2) + (py as f32 + 0.5 - cy).powi(2)).sqrt();
                if d <= rad {
                    self.v[(py * self.w + px) as usize] = if on { 1. } else { 0. };
                }
            }
        }
    }

    /// Area-averaged to `w`×`h`.
    pub fn resize(&self, w: u32, h: u32) -> Vec<f32> {
        let (sx, sy) = (self.w as f32 / w as f32, self.h as f32 / h as f32);
        let mut out = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            let (y0, y1) = ((y as f32 * sy) as u32, (((y + 1) as f32 * sy).ceil() as u32).clamp(1, self.h));
            for x in 0..w {
                let (x0, x1) = ((x as f32 * sx) as u32, (((x + 1) as f32 * sx).ceil() as u32).clamp(1, self.w));
                let (mut sum, mut n) = (0., 0.);
                for yy in y0.min(y1 - 1)..y1 {
                    for xx in x0.min(x1 - 1)..x1 {
                        sum += self.v[(yy * self.w + xx) as usize];
                        n += 1.;
                    }
                }
                out.push(sum / f32::max(n, 1.));
            }
        }
        out
    }

    /// The painting moved from one crop of the photo to another (`[x, y,
    /// w, h]`, 0..1 of the whole photo), at `w`×`h`. Where the old crop
    /// didn't reach is unpainted.
    pub fn reframe(&self, from: [f32; 4], to: [f32; 4], w: u32, h: u32) -> Paint {
        let mut v = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                // Here on the whole photo, then on the old crop.
                let (u, t) = (to[0] + (x as f32 + 0.5) / w as f32 * to[2], to[1] + (y as f32 + 0.5) / h as f32 * to[3]);
                let (ou, ot) = ((u - from[0]) / from[2], (t - from[1]) / from[3]);
                v.push(if (0. ..1.).contains(&ou) && (0. ..1.).contains(&ot) {
                    self.v[((ot * self.h as f32) as u32).min(self.h - 1) as usize * self.w as usize + ((ou * self.w as f32) as u32).min(self.w - 1) as usize]
                } else {
                    0.
                });
            }
        }
        Paint { w, h, v }
    }

    /// From a greyscale picture: light is subject.
    pub fn from_image(img: &image::GrayImage, w: u32, h: u32) -> Paint {
        let src = Paint { w: img.width().max(1), h: img.height().max(1), v: img.pixels().map(|p| p.0[0] as f32 / 255.).collect() };
        Paint { w, h, v: src.resize(w, h) }
    }

    /// As a greyscale PNG.
    pub fn png(&self) -> Result<Vec<u8>, String> {
        let img = image::GrayImage::from_raw(self.w, self.h, self.v.iter().map(|v| (v.clamp(0., 1.) * 255.).round() as u8).collect()).ok_or("mask size mismatch")?;
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).map_err(|e| e.to_string())?;
        Ok(out.into_inner())
    }

    pub fn is_empty(&self) -> bool {
        self.v.iter().all(|v| *v <= 0.)
    }
}

fn smooth(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge1 <= edge0 {
        return if x >= edge0 { 1. } else { 0. };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}

/// The soft mask for `pixels` (`w`×`h`, the adjusted art-size picture).
/// `paint` is the painted mask already at `w`×`h`.
pub fn compute(spec: &Spec, pixels: &[Rgb], w: u32, h: u32, paint: Option<&[f32]>) -> Vec<f32> {
    let n = (w * h) as usize;
    let mut m: Vec<f32> = match spec.kind {
        Kind::None => vec![1.; n],
        Kind::Luma => {
            let edge = 0.04;
            pixels
                .iter()
                .map(|&c| {
                    let l = oklab(c)[0];
                    smooth(spec.low - edge, spec.low + edge, l) * (1. - smooth(spec.high - edge, spec.high + edge, l))
                })
                .collect()
        }
        Kind::Color => {
            let key = oklab(spec.color);
            let tol = spec.tolerance.max(0.005);
            pixels
                .iter()
                .map(|&c| {
                    let lab = oklab(c);
                    let d = ((lab[0] - key[0]).powi(2) + (lab[1] - key[1]).powi(2) + (lab[2] - key[2]).powi(2)).sqrt();
                    1. - smooth(tol, tol * 1.5, d)
                })
                .collect()
        }
        Kind::Border => border(pixels, w, h, spec.tolerance.max(0.005)),
        Kind::Paint => match paint {
            Some(p) if p.len() == n => p.to_vec(),
            _ => vec![0.; n],
        },
    };
    if spec.invert && spec.kind != Kind::None {
        for v in &mut m {
            *v = 1. - *v;
        }
    }
    let radius = (spec.feather.clamp(0., 1.) * 6.).round() as usize;
    if radius > 0 && spec.kind != Kind::None {
        m = blur(&blur(&m, w as usize, h as usize, radius), w as usize, h as usize, radius);
    }
    m
}

/// Background is whatever floods in from the edge through pixels close to
/// the colour the flood started from; the rest is subject.
fn border(pixels: &[Rgb], w: u32, h: u32, tol: f32) -> Vec<f32> {
    let (w, h) = (w as usize, h as usize);
    let lab: Vec<[f32; 3]> = pixels.iter().map(|&c| oklab(c)).collect();
    let close = |a: [f32; 3], b: [f32; 3]| (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2) <= tol * tol;
    let mut seed: Vec<Option<[f32; 3]>> = vec![None; w * h];
    let mut queue = VecDeque::new();
    for x in 0..w {
        for y in [0, h - 1] {
            queue.push_back((x, y, lab[y * w + x]));
        }
    }
    for y in 0..h {
        for x in [0, w - 1] {
            queue.push_back((x, y, lab[y * w + x]));
        }
    }
    while let Some((x, y, from)) = queue.pop_front() {
        let at = y * w + x;
        if seed[at].is_some() || !close(lab[at], from) {
            continue;
        }
        seed[at] = Some(from);
        if x > 0 {
            queue.push_back((x - 1, y, from));
        }
        if x + 1 < w {
            queue.push_back((x + 1, y, from));
        }
        if y > 0 {
            queue.push_back((x, y - 1, from));
        }
        if y + 1 < h {
            queue.push_back((x, y + 1, from));
        }
    }
    seed.iter().map(|s| if s.is_some() { 0. } else { 1. }).collect()
}

/// A box blur of radius `r`, rows then columns.
fn blur(m: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let pass = |src: &[f32], len: usize, count: usize, at: &dyn Fn(usize, usize) -> usize| {
        let mut out = vec![0.; src.len()];
        for line in 0..count {
            for i in 0..len {
                let (lo, hi) = (i.saturating_sub(r), (i + r).min(len - 1));
                let sum: f32 = (lo..=hi).map(|j| src[at(line, j)]).sum();
                out[at(line, i)] = sum / (hi - lo + 1) as f32;
            }
        }
        out
    };
    let rows = pass(m, w, h, &|line, i| line * w + i);
    pass(&rows, h, w, &|line, i| i * w + line)
}

/// Which pixels are subject: the soft mask dithered with blue noise, so
/// soft edges become a dither rather than a jagged line.
pub fn select(soft: &[f32], w: u32) -> Vec<bool> {
    soft.iter()
        .enumerate()
        .map(|(i, &v)| v > ferrite_design::dither::blue_noise_threshold(i as u32 % w, i as u32 / w))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A dark disc on a white backdrop, 20×20.
    fn disc() -> Vec<Rgb> {
        (0..400).map(|i| if ((i % 20) as f32 - 9.5).powi(2) + ((i / 20) as f32 - 9.5).powi(2) < 36. { [0.1, 0.1, 0.4] } else { [1.; 3] }).collect()
    }

    fn spec(kind: Kind) -> Spec {
        Spec { kind, feather: 0., ..Spec::default() }
    }

    #[test]
    fn reframing_keeps_the_painting_in_place() {
        // Paint the right half of a 10×10 photo, then crop to its right half.
        let mut p = Paint::blank(10, 10);
        p.v.iter_mut().enumerate().filter(|(i, _)| i % 10 >= 5).for_each(|(_, v)| *v = 1.);
        let r = p.reframe([0., 0., 1., 1.], [0.5, 0., 0.5, 1.], 5, 10);
        assert!(r.v.iter().all(|&v| v == 1.));
        // And back: the left half was never painted.
        let back = r.reframe([0.5, 0., 0.5, 1.], [0., 0., 1., 1.], 10, 10);
        assert_eq!(back.v, p.v);
    }

    #[test]
    fn border_cuts_out_the_subject() {
        let m = compute(&spec(Kind::Border), &disc(), 20, 20, None);
        assert_eq!(m[0], 0.);
        assert_eq!(m[10 * 20 + 10], 1.);
        // Inverted, the backdrop is the subject.
        let inv = compute(&Spec { invert: true, ..spec(Kind::Border) }, &disc(), 20, 20, None);
        assert_eq!((inv[0], inv[210]), (1., 0.));
    }

    #[test]
    fn border_keeps_enclosed_backdrop_colour_as_subject() {
        // A white hole inside a dark ring is not reachable from the edge.
        let px: Vec<Rgb> = (0..400)
            .map(|i| {
                let d = ((i % 20) as f32 - 9.5).powi(2) + ((i / 20) as f32 - 9.5).powi(2);
                if (9. ..49.).contains(&d) { [0.; 3] } else { [1.; 3] }
            })
            .collect();
        let m = compute(&spec(Kind::Border), &px, 20, 20, None);
        assert_eq!(m[10 * 20 + 10], 1.);
    }

    #[test]
    fn luma_and_colour() {
        let dark = compute(&Spec { low: 0., high: 0.5, ..spec(Kind::Luma) }, &disc(), 20, 20, None);
        assert!(dark[210] > 0.9 && dark[0] < 0.1);
        let blue = compute(&Spec { color: [0.1, 0.1, 0.4], ..spec(Kind::Color) }, &disc(), 20, 20, None);
        assert!(blue[210] > 0.9 && blue[0] < 0.1);
    }

    #[test]
    fn feather_softens_the_edge() {
        let hard = compute(&spec(Kind::Border), &disc(), 20, 20, None);
        let soft = compute(&Spec { feather: 0.5, ..spec(Kind::Border) }, &disc(), 20, 20, None);
        let partial = |m: &[f32]| m.iter().filter(|v| **v > 0.05 && **v < 0.95).count();
        assert_eq!(partial(&hard), 0);
        assert!(partial(&soft) > 20);
        // Selection dithers the soft edge but keeps the solid parts solid.
        let sel = select(&soft, 20);
        assert!(!sel[0] && sel[210]);
    }

    #[test]
    fn painting_and_png_round_trip() {
        let mut p = Paint::blank(40, 20);
        assert!(p.is_empty());
        p.dab(0.5, 0.5, 0.1, true);
        assert_eq!(p.v[(10 * 40 + 20) as usize], 1.);
        assert_eq!(p.v[0], 0.);
        p.dab(0.5, 0.5, 0.05, false);
        assert_eq!(p.v[(10 * 40 + 20) as usize], 0.);
        let png = p.png().unwrap();
        let back = Paint::from_image(&image::load_from_memory(&png).unwrap().to_luma8(), 40, 20);
        assert_eq!(back, p);
        // Painted masks follow the art's size.
        let small = p.resize(20, 10);
        assert_eq!(small.len(), 200);
        let m = compute(&spec(Kind::Paint), &vec![[0.5; 3]; 200], 20, 10, Some(&small));
        assert_eq!(m, small);
    }

    #[test]
    fn keys_round_trip() {
        for k in Kind::ALL {
            assert_eq!(Kind::from_key(k.key()), Some(k));
        }
    }
}
