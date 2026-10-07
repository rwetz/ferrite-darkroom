//! The darkroom's process, all pure: a photo becomes a grey working print,
//! the print is adjusted, then it's developed either as dither (Ferrite's
//! own Bayer, blue-noise and Atkinson masks) or as ASCII (Ferrite's glyph-
//! fitted character sets). Preview and export run the same code, so what
//! you see is what you save.

use std::path::Path;

use ferrite_design::ascii::{self, ArtStyle, Charset, Fit};
use ferrite_design::dither::{self, Pattern, Picture};

/// The working print is at most this wide; more detail than any export uses.
const WORK_W: u32 = 960;

/// A greyscale image, 0 = black, 1 = white.
#[derive(Clone, Debug, PartialEq)]
pub struct Print {
    pub w: u32,
    pub h: u32,
    pub lum: Vec<f32>,
}

impl Print {
    pub fn aspect(&self) -> f32 {
        self.h as f32 / self.w.max(1) as f32
    }

    /// Area-averaged resample to `w`×`h`.
    pub fn resize(&self, w: u32, h: u32) -> Print {
        let (w, h) = (w.max(1), h.max(1));
        let (sx, sy) = (self.w as f32 / w as f32, self.h as f32 / h as f32);
        let mut lum = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            let (y0, y1) = ((y as f32 * sy) as u32, (((y + 1) as f32 * sy).ceil() as u32).clamp(1, self.h));
            for x in 0..w {
                let (x0, x1) = ((x as f32 * sx) as u32, (((x + 1) as f32 * sx).ceil() as u32).clamp(1, self.w));
                let (mut sum, mut n) = (0f32, 0f32);
                for yy in y0.min(y1 - 1)..y1 {
                    for xx in x0.min(x1 - 1)..x1 {
                        sum += self.lum[(yy * self.w + xx) as usize];
                        n += 1.;
                    }
                }
                lum.push(sum / n.max(1.));
            }
        }
        Print { w, h, lum }
    }
}

/// Open a photo from disk as a working print.
pub fn load(path: &Path) -> Result<Print, String> {
    let img = image::open(path).map_err(|e| match e {
        image::ImageError::Unsupported(_) => "that isn't an image Darkroom can read (PNG, JPEG, GIF, BMP, WebP)".to_string(),
        image::ImageError::IoError(e) => e.to_string(),
        e => format!("couldn't read the image: {e}"),
    })?;
    let luma = img.to_luma8();
    let (w, h) = luma.dimensions();
    if w == 0 || h == 0 {
        return Err("the image is empty".into());
    }
    let print = Print { w, h, lum: luma.as_raw().iter().map(|&v| v as f32 / 255.).collect() };
    Ok(if w > WORK_W { print.resize(WORK_W, ((h as f32 / w as f32) * WORK_W as f32).round() as u32) } else { print })
}

/// A test print to work on before you've brought a photo: a lit sphere on
/// a horizon under a low sun — smooth ramps and hard edges both.
pub fn sample() -> Print {
    let (w, h) = (640u32, 400u32);
    let lum = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x as f32 / w as f32, y as f32 / h as f32)))
        .map(|(u, v)| {
            let horizon = 0.62;
            // Sky: dark overhead, a glow around a low sun.
            let (sun_u, sun_v) = (0.76, 0.5);
            let d_sun = ((u - sun_u).powi(2) + ((v - sun_v) * 0.62).powi(2)).sqrt();
            let glow = (-d_sun * 7.).exp();
            let mut l = if v < horizon { 0.06 + 0.3 * v / horizon + 0.6 * glow } else { 0.1 + 0.2 * (1. - (v - horizon) / (1. - horizon)) };
            if d_sun < 0.05 && v < horizon {
                l = 1.;
            }
            // Ground stripes running to the horizon.
            if v >= horizon && (((u - 0.5) / (v - horizon + 0.05)) * 2.).rem_euclid(1.) < 0.12 {
                l += 0.18;
            }
            // The sphere, lit from the sun's side.
            let (cx, cy, r) = (0.32, 0.52, 0.2);
            let (dx, dy) = ((u - cx) / r, (v - cy) / (r * 1.6));
            let d2 = dx * dx + dy * dy;
            if d2 < 1. {
                let dz = (1. - d2).sqrt();
                let (lx, ly, lz) = (0.62, -0.3, 0.72);
                l = 0.04 + 0.96 * (dx * lx + dy * ly + dz * lz).max(0.).powf(1.4);
            }
            l.clamp(0., 1.)
        })
        .collect();
    Print { w, h, lum }
}

/// How the print is adjusted before it's developed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Adjust {
    /// -1..1
    pub brightness: f32,
    /// 0.25..3, 1 = unchanged
    pub contrast: f32,
    pub invert: bool,
    /// Whether the ink is lighter than the paper (a dark scheme): then the
    /// light parts of the photo get the ink.
    pub light_ink: bool,
}

impl Default for Adjust {
    fn default() -> Self {
        Adjust { brightness: 0., contrast: 1., invert: false, light_ink: true }
    }
}

/// Ink levels (0 = paper, 255 = ink) for a print, quantised once so the
/// preview and the export see identical numbers.
pub fn ink_levels(print: &Print, adjust: Adjust) -> Vec<u8> {
    print
        .lum
        .iter()
        .map(|&l| {
            let l = ((l - 0.5) * adjust.contrast + 0.5 + adjust.brightness * 0.5).clamp(0., 1.);
            let ink = if adjust.light_ink != adjust.invert { l } else { 1. - l };
            (ink * 255.).round() as u8
        })
        .collect()
}

/// Rows for `cols` columns of square pixels.
pub fn rows_for(print: &Print, cols: u32) -> u32 {
    ((cols as f32 * print.aspect()).round() as u32).max(1)
}

/// The print at `cols`×rows, adjusted, as a dither picture: one sample per
/// output pixel.
pub fn picture(print: &Print, cols: u32, adjust: Adjust) -> Picture {
    let small = print.resize(cols, rows_for(print, cols));
    Picture::new(small.w, small.h, ink_levels(&small, adjust))
}

/// Which pixels are inked: exactly what the preview draws for `picture`.
pub fn mask(picture: &Picture, pattern: Pattern) -> Vec<bool> {
    let (w, h) = picture.size();
    let levels: Vec<f32> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| picture.sample((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32))
        .collect();
    dither::mask(pattern, w, h, &levels)
}

/// Encode the dithered picture as a PNG, each pixel `scale`×`scale`, in
/// the given ink and paper (RGB).
pub fn png(picture: &Picture, pattern: Pattern, scale: u32, ink: [u8; 3], paper: [u8; 3]) -> Result<Vec<u8>, String> {
    let (w, h) = picture.size();
    let scale = scale.max(1);
    let mask = mask(picture, pattern);
    let img = image::RgbImage::from_fn(w * scale, h * scale, |x, y| image::Rgb(if mask[((y / scale) * w + x / scale) as usize] { ink } else { paper }));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

/// The print as text, `cols` characters wide.
pub fn ascii_lines(print: &Print, cols: usize, adjust: Adjust, charset: Charset, fit: Fit) -> std::rc::Rc<Vec<String>> {
    // Fitting samples sub-character detail: give it ~8 samples per column.
    let w = (cols as u32 * 8).min(print.w).max(1);
    let small = print.resize(w, rows_for(print, w));
    let pic = Picture::new(small.w, small.h, ink_levels(&small, adjust));
    let style = ArtStyle { fit, ..ArtStyle::new(charset) };
    ascii::picture_art(&pic, cols, style)
}

pub fn text(lines: &[String]) -> String {
    let mut s = lines.iter().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n");
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(w: u32, h: u32) -> Print {
        Print { w, h, lum: (0..h).flat_map(|_| (0..w).map(move |x| x as f32 / (w - 1) as f32)).collect() }
    }

    #[test]
    fn resize_averages_area() {
        let p = Print { w: 4, h: 2, lum: vec![0., 1., 0., 1., 0., 1., 0., 1.] };
        let s = p.resize(2, 1);
        assert_eq!((s.w, s.h), (2, 1));
        assert!(s.lum.iter().all(|v| (v - 0.5).abs() < 1e-6), "{:?}", s.lum);
        let up = p.resize(8, 4);
        assert_eq!(up.lum.len(), 32);
    }

    #[test]
    fn ink_follows_the_scheme() {
        let p = Print { w: 2, h: 1, lum: vec![0., 1.] };
        // Dark scheme: light ink, so white in the photo is ink.
        assert_eq!(ink_levels(&p, Adjust::default()), vec![0, 255]);
        // Light scheme: dark ink, so black in the photo is ink.
        assert_eq!(ink_levels(&p, Adjust { light_ink: false, ..Adjust::default() }), vec![255, 0]);
        // Invert flips either.
        assert_eq!(ink_levels(&p, Adjust { invert: true, ..Adjust::default() }), vec![255, 0]);
    }

    #[test]
    fn brightness_and_contrast() {
        let p = Print { w: 3, h: 1, lum: vec![0.25, 0.5, 0.75] };
        let flat = ink_levels(&p, Adjust { contrast: 0.25, ..Adjust::default() });
        assert!(flat[2] - flat[0] < 40);
        let bright = ink_levels(&p, Adjust { brightness: 1., ..Adjust::default() });
        assert_eq!(bright[1], 255);
    }

    #[test]
    fn every_pattern_tracks_tone() {
        let print = ramp(64, 16);
        let pic = picture(&print, 64, Adjust::default());
        assert_eq!(pic.size(), (64, 16));
        for pattern in [Pattern::Bayer4, Pattern::BlueNoise, Pattern::Atkinson] {
            let m = mask(&pic, pattern);
            let left = m.iter().enumerate().filter(|(i, on)| **on && (*i as u32 % 64) < 32).count();
            let right = m.iter().enumerate().filter(|(i, on)| **on && (*i as u32 % 64) >= 32).count();
            assert!(right > left * 2, "{pattern:?}: {left} vs {right}");
        }
    }

    #[test]
    fn png_is_scaled_and_two_coloured() {
        let pic = picture(&ramp(16, 8), 16, Adjust::default());
        let bytes = png(&pic, Pattern::Bayer4, 4, [255, 200, 0], [10, 10, 12]).unwrap();
        let img = image::load_from_memory(&bytes).unwrap().to_rgb8();
        assert_eq!(img.dimensions(), (64, 32));
        assert!(img.pixels().all(|p| p.0 == [255, 200, 0] || p.0 == [10, 10, 12]));
        // A 4×4 block is one picture pixel.
        assert_eq!(img.get_pixel(0, 0), img.get_pixel(3, 3));
    }

    #[test]
    fn ascii_has_the_width_asked_for() {
        let lines = ascii_lines(&sample(), 60, Adjust::default(), Charset::Classic, Fit::Tone);
        assert!(lines.len() > 5);
        assert!(lines.iter().all(|l| l.chars().count() == 60));
        let t = text(&lines);
        assert!(t.ends_with('\n') && !t.contains(" \n"));
    }

    #[test]
    fn sample_has_range() {
        let s = sample();
        let (lo, hi) = s.lum.iter().fold((1f32, 0f32), |(lo, hi), &v| (lo.min(v), hi.max(v)));
        assert!(lo < 0.15 && hi > 0.95);
    }
}
