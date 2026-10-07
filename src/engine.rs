//! The dither engine: a colour picture and a palette in, one palette index
//! per pixel out. Pure, so the preview and every export run the same code.
//!
//! Three families share one entry point, [`dither`]:
//!
//! - **Ordered** (threshold maps): plain threshold, white noise, Bayer 2–16,
//!   Ferrite's blue-noise tile, interleaved gradient noise, a halftone dot.
//! - **Error diffusion**: eleven kernels from Floyd–Steinberg to Shiau–Fan,
//!   optionally serpentine.
//!
//! Colours are matched in Oklab by default (perceptual), or in plain sRGB.
//! A two-colour palette is dithered along the line between its colours, by
//! lightness, which is what makes an ink-on-paper duotone keep every tone.

use std::sync::OnceLock;

/// An sRGB colour, each channel 0..1.
pub type Rgb = [f32; 3];

/// Error-diffusion taps `(dx, dy, weight)` and the divisor of the weights.
type Kernel = (&'static [(i32, i32, f32)], f32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Algo {
    Threshold,
    Random,
    Bayer2,
    Bayer4,
    Bayer8,
    Bayer16,
    BlueNoise,
    Gradient,
    Halftone,
    FloydSteinberg,
    FalseFloydSteinberg,
    JarvisJudiceNinke,
    Stucki,
    Burkes,
    Sierra,
    SierraTwoRow,
    SierraLite,
    Atkinson,
    ShiauFan,
    ShiauFan2,
}

impl Algo {
    pub const ALL: [Algo; 20] = [
        Algo::FloydSteinberg,
        Algo::Atkinson,
        Algo::JarvisJudiceNinke,
        Algo::Stucki,
        Algo::Burkes,
        Algo::Sierra,
        Algo::SierraTwoRow,
        Algo::SierraLite,
        Algo::FalseFloydSteinberg,
        Algo::ShiauFan,
        Algo::ShiauFan2,
        Algo::Bayer2,
        Algo::Bayer4,
        Algo::Bayer8,
        Algo::Bayer16,
        Algo::BlueNoise,
        Algo::Gradient,
        Algo::Halftone,
        Algo::Random,
        Algo::Threshold,
    ];

    /// The settings-file key.
    pub fn key(self) -> &'static str {
        match self {
            Algo::Threshold => "threshold",
            Algo::Random => "random",
            Algo::Bayer2 => "bayer-2",
            Algo::Bayer4 => "bayer",
            Algo::Bayer8 => "bayer-8",
            Algo::Bayer16 => "bayer-16",
            Algo::BlueNoise => "blue-noise",
            Algo::Gradient => "gradient-noise",
            Algo::Halftone => "halftone",
            Algo::FloydSteinberg => "floyd-steinberg",
            Algo::FalseFloydSteinberg => "false-floyd-steinberg",
            Algo::JarvisJudiceNinke => "jarvis-judice-ninke",
            Algo::Stucki => "stucki",
            Algo::Burkes => "burkes",
            Algo::Sierra => "sierra",
            Algo::SierraTwoRow => "sierra-two-row",
            Algo::SierraLite => "sierra-lite",
            Algo::Atkinson => "atkinson",
            Algo::ShiauFan => "shiau-fan",
            Algo::ShiauFan2 => "shiau-fan-2",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Algo::Threshold => "Threshold",
            Algo::Random => "Random",
            Algo::Bayer2 => "Bayer 2×2",
            Algo::Bayer4 => "Bayer 4×4",
            Algo::Bayer8 => "Bayer 8×8",
            Algo::Bayer16 => "Bayer 16×16",
            Algo::BlueNoise => "Blue noise",
            Algo::Gradient => "Gradient noise",
            Algo::Halftone => "Halftone dot",
            Algo::FloydSteinberg => "Floyd–Steinberg",
            Algo::FalseFloydSteinberg => "False Floyd–Steinberg",
            Algo::JarvisJudiceNinke => "Jarvis–Judice–Ninke",
            Algo::Stucki => "Stucki",
            Algo::Burkes => "Burkes",
            Algo::Sierra => "Sierra",
            Algo::SierraTwoRow => "Sierra two-row",
            Algo::SierraLite => "Sierra Lite",
            Algo::Atkinson => "Atkinson",
            Algo::ShiauFan => "Shiau–Fan",
            Algo::ShiauFan2 => "Shiau–Fan 2",
        }
    }

    pub fn from_key(key: &str) -> Option<Algo> {
        Algo::ALL.into_iter().find(|a| a.key() == key)
    }

    /// Error diffusion (serial, carries error) rather than a threshold map.
    pub fn diffuses(self) -> bool {
        self.kernel().is_some()
    }

    /// `(dx, dy, weight)` taps and their divisor, for error diffusion.
    fn kernel(self) -> Option<Kernel> {
        Some(match self {
            Algo::FloydSteinberg => (&[(1, 0, 7.), (-1, 1, 3.), (0, 1, 5.), (1, 1, 1.)], 16.),
            Algo::FalseFloydSteinberg => (&[(1, 0, 3.), (0, 1, 3.), (1, 1, 2.)], 8.),
            Algo::JarvisJudiceNinke => (
                &[(1, 0, 7.), (2, 0, 5.), (-2, 1, 3.), (-1, 1, 5.), (0, 1, 7.), (1, 1, 5.), (2, 1, 3.), (-2, 2, 1.), (-1, 2, 3.), (0, 2, 5.), (1, 2, 3.), (2, 2, 1.)],
                48.,
            ),
            Algo::Stucki => (
                &[(1, 0, 8.), (2, 0, 4.), (-2, 1, 2.), (-1, 1, 4.), (0, 1, 8.), (1, 1, 4.), (2, 1, 2.), (-2, 2, 1.), (-1, 2, 2.), (0, 2, 4.), (1, 2, 2.), (2, 2, 1.)],
                42.,
            ),
            Algo::Burkes => (&[(1, 0, 8.), (2, 0, 4.), (-2, 1, 2.), (-1, 1, 4.), (0, 1, 8.), (1, 1, 4.), (2, 1, 2.)], 32.),
            Algo::Sierra => (&[(1, 0, 5.), (2, 0, 3.), (-2, 1, 2.), (-1, 1, 4.), (0, 1, 5.), (1, 1, 4.), (2, 1, 2.), (-1, 2, 2.), (0, 2, 3.), (1, 2, 2.)], 32.),
            Algo::SierraTwoRow => (&[(1, 0, 4.), (2, 0, 3.), (-2, 1, 1.), (-1, 1, 2.), (0, 1, 3.), (1, 1, 2.), (2, 1, 1.)], 16.),
            Algo::SierraLite => (&[(1, 0, 2.), (-1, 1, 1.), (0, 1, 1.)], 4.),
            // Atkinson spreads only 6/8 of the error: the lost quarter is its
            // high-contrast look.
            Algo::Atkinson => (&[(1, 0, 1.), (2, 0, 1.), (-1, 1, 1.), (0, 1, 1.), (1, 1, 1.), (0, 2, 1.)], 8.),
            Algo::ShiauFan => (&[(1, 0, 4.), (-2, 1, 1.), (-1, 1, 1.), (0, 1, 2.)], 8.),
            Algo::ShiauFan2 => (&[(1, 0, 8.), (-3, 1, 1.), (-2, 1, 1.), (-1, 1, 2.), (0, 1, 4.)], 16.),
            _ => return None,
        })
    }

    /// The threshold, 0..1, an ordered algorithm uses at `(x, y)`.
    #[allow(clippy::excessive_precision)]
    fn threshold(self, x: u32, y: u32, seed: u32) -> f32 {
        match self {
            Algo::Random => hash01(x, y, seed),
            Algo::Bayer2 => bayer(1, x, y),
            Algo::Bayer4 => bayer(2, x, y),
            Algo::Bayer8 => bayer(3, x, y),
            Algo::Bayer16 => bayer(4, x, y),
            Algo::BlueNoise => ferrite_design::dither::blue_noise_threshold(x, y),
            Algo::Gradient => {
                // Jimenez's interleaved gradient noise.
                let f = 0.067_110_56 * x as f32 + 0.005_837_15 * y as f32;
                (52.982_918 * f.fract()).fract()
            }
            Algo::Halftone => {
                let m = halftone_matrix();
                m[((y % HALFTONE) * HALFTONE + x % HALFTONE) as usize]
            }
            _ => 0.5,
        }
    }
}

/// Bayer threshold for a 2^`order` matrix, from the recursive rule
/// `M(2n) = [[4M, 4M+2], [4M+3, 4M+1]]` without storing it: each bit pair
/// of (x^y, y) is a base-4 digit, the lowest coordinate bits the most
/// significant.
fn bayer(order: u32, x: u32, y: u32) -> f32 {
    let mut v = 0u32;
    for bit in 0..order {
        let (xb, yb) = ((x >> bit) & 1, (y >> bit) & 1);
        v = (v << 2) | ((xb ^ yb) << 1) | yb;
    }
    (v as f32 + 0.5) / (1u32 << (2 * order)) as f32
}

const HALFTONE: u32 = 8;

/// An 8×8 clustered-dot screen: the cells ranked by a cosine spot function,
/// so dots grow from the centre and every level is used exactly once.
fn halftone_matrix() -> &'static [f32; 64] {
    static M: OnceLock<[f32; 64]> = OnceLock::new();
    M.get_or_init(|| {
        let tau = std::f32::consts::TAU;
        let n = HALFTONE as f32;
        let spot = |i: usize| {
            let (x, y) = ((i % 8) as f32 + 0.5, (i / 8) as f32 + 0.5);
            -((tau * x / n).cos() + (tau * y / n).cos())
        };
        let mut order: Vec<usize> = (0..64).collect();
        order.sort_by(|&a, &b| spot(a).total_cmp(&spot(b)).then(a.cmp(&b)));
        let mut m = [0f32; 64];
        for (rank, &i) in order.iter().enumerate() {
            m[i] = (rank as f32 + 0.5) / 64.;
        }
        m
    })
}

fn hash01(x: u32, y: u32, seed: u32) -> f32 {
    let mut h = x.wrapping_mul(0x8da6_b343) ^ y.wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

// ── Colour ───────────────────────────────────────────────────────────────

fn to_linear(c: f32) -> f32 {
    if c <= 0.040_45 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn to_srgb(c: f32) -> f32 {
    let c = c.max(0.);
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1. / 2.4) - 0.055 }
}

/// sRGB → Oklab (Björn Ottosson's matrices, kept digit for digit as published).
#[allow(clippy::excessive_precision)]
pub fn oklab([r, g, b]: Rgb) -> [f32; 3] {
    let (r, g, b) = (to_linear(r), to_linear(g), to_linear(b));
    let l = (0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

/// Oklab → sRGB, clamped to the gamut.
#[allow(clippy::excessive_precision)]
pub fn from_oklab([l, a, b]: [f32; 3]) -> Rgb {
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let (l3, m3, s3) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    let r = 4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_94 * s3;
    let g = -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_38 * s3;
    let b = -0.004_196_086_3 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3;
    [to_srgb(r).min(1.), to_srgb(g).min(1.), to_srgb(b).min(1.)]
}

/// Where colours are compared and error is carried.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Space {
    Oklab,
    Rgb,
}

/// Everything about a dither except the picture and the palette.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    pub algo: Algo,
    /// 0..2. Error diffusion: how much error is carried (0 = posterise).
    /// Ordered: how far the threshold map reaches.
    pub strength: f32,
    /// -1..1: shifts where ink starts. Ordered algorithms move the tone;
    /// error diffusion mostly changes the texture, since the carried error
    /// pulls the tone back.
    pub bias: f32,
    /// Error diffusion runs alternate rows right to left.
    pub serpentine: bool,
    pub space: Space,
    /// For [`Algo::Random`].
    pub seed: u32,
}

impl Default for Params {
    fn default() -> Self {
        Params { algo: Algo::FloydSteinberg, strength: 1., bias: 0., serpentine: true, space: Space::Oklab, seed: 1 }
    }
}

/// The previous frame, for temporal stability: its indices, which pixels'
/// source stood still since then, and how readily to keep a pixel's old
/// colour (0..1).
pub struct Hold<'a> {
    pub prev: &'a [u8],
    pub still: &'a [bool],
    pub margin: f32,
}

impl Hold<'_> {
    /// The previous index at `at`, if its source stood still there and
    /// that colour is nearly as good a match (`d_prev` vs `d_fresh`, squared
    /// distances) as the fresh choice. `gap2` is the palette's typical
    /// squared step, so the margin means the same on any palette.
    fn keep(&self, at: usize, d_prev: f32, d_fresh: f32, gap2: f32) -> bool {
        self.still[at] && d_prev <= d_fresh + self.margin * 0.5 * gap2
    }
}

/// How far a full bias moves the threshold, as a share of the tonal range.
const BIAS_REACH: f32 = 0.45;

/// Dither `pixels` (`w`×`h`, row-major sRGB) to `palette`. Returns one
/// palette index per pixel. An empty palette gives all zeros.
pub fn dither(pixels: &[Rgb], w: u32, h: u32, palette: &[Rgb], p: Params) -> Vec<u8> {
    dither_held(pixels, w, h, palette, p, None)
}

/// [`dither`], keeping the previous frame's colours where they still fit.
/// Holding happens inside error diffusion, so a kept pixel's error still
/// reaches its neighbours: tone stays right, and dots left behind by
/// something that moved away get cleared instead of frozen. Ordered
/// algorithms are stable by nature and ignore `hold`.
pub fn dither_held(pixels: &[Rgb], w: u32, h: u32, palette: &[Rgb], p: Params, hold: Option<&Hold>) -> Vec<u8> {
    let n = (w * h) as usize;
    debug_assert_eq!(pixels.len(), n);
    if palette.len() < 2 {
        return vec![0; n];
    }
    let pal_lab: Vec<[f32; 3]> = palette.iter().map(|&c| oklab(c)).collect();
    if palette.len() == 2 {
        return duotone(pixels, w, h, &pal_lab, p, hold);
    }

    // Fit the picture's lightness into the palette's, so a palette with no
    // true black or white (Game Boy) still gets every tone.
    let (lo, hi) = pal_lab.iter().fold((f32::MAX, f32::MIN), |(lo, hi), c| (lo.min(c[0]), hi.max(c[0])));
    let fitted = pixels.iter().map(|&c| {
        let mut lab = oklab(c);
        lab[0] = lo + lab[0].clamp(0., 1.) * (hi - lo);
        lab
    });
    let (mut work, pal): (Vec<[f32; 3]>, Vec<[f32; 3]>) = match p.space {
        Space::Oklab => (fitted.collect(), pal_lab),
        Space::Rgb => (fitted.map(from_oklab).collect(), palette.to_vec()),
    };
    // The bias nudges which colour is picked, not the value carried on.
    let lean = p.bias * BIAS_REACH * (hi - lo).max(0.05);
    let nearest = |v: [f32; 3]| -> usize {
        let v = match p.space {
            Space::Oklab => [v[0] + lean, v[1], v[2]],
            Space::Rgb => [v[0] + lean, v[1] + lean, v[2] + lean],
        };
        let mut best = (0, f32::MAX);
        for (i, c) in pal.iter().enumerate() {
            let d = (v[0] - c[0]).powi(2) + (v[1] - c[1]).powi(2) + (v[2] - c[2]).powi(2);
            if d < best.1 {
                best = (i, d);
            }
        }
        best.0
    };

    if let Some((taps, div)) = p.algo.kernel() {
        let d2 = |a: [f32; 3], b: [f32; 3]| (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2);
        // The palette's typical step: each colour's distance to its nearest other.
        let gap2 = pal.iter().map(|&a| pal.iter().filter(|&&b| b != a).map(|&b| d2(a, b)).fold(f32::MAX, f32::min)).sum::<f32>() / pal.len() as f32;
        let mut out = vec![0u8; n];
        let (wi, hi_) = (w as i32, h as i32);
        for y in 0..hi_ {
            let rtl = p.serpentine && y % 2 == 1;
            for i in 0..wi {
                let x = if rtl { wi - 1 - i } else { i };
                let at = (y * wi + x) as usize;
                let v = work[at];
                let mut q = nearest(v);
                if let Some(h) = hold {
                    let pq = h.prev[at] as usize;
                    if pq < pal.len() && pq != q && h.keep(at, d2(v, pal[pq]), d2(v, pal[q]), gap2) {
                        q = pq;
                    }
                }
                out[at] = q as u8;
                let c = pal[q];
                let err = [(v[0] - c[0]) * p.strength, (v[1] - c[1]) * p.strength, (v[2] - c[2]) * p.strength];
                for &(dx, dy, wt) in taps {
                    let (tx, ty) = (if rtl { x - dx } else { x + dx }, y + dy);
                    if !(0..wi).contains(&tx) || ty >= hi_ {
                        continue;
                    }
                    let t = &mut work[(ty * wi + tx) as usize];
                    let k = wt / div;
                    t[0] += err[0] * k;
                    t[1] += err[1] * k;
                    t[2] += err[2] * k;
                }
            }
        }
        return out;
    }

    // Ordered: nudge each pixel by the threshold map, then take the nearest.
    // The reach is about one palette step; more colours, smaller steps.
    let spread = p.strength * (hi - lo).max(0.05) / (palette.len() as f32).sqrt();
    let ordered = p.algo != Algo::Threshold;
    work.iter()
        .enumerate()
        .map(|(i, &v)| {
            let (x, y) = (i as u32 % w, i as u32 / w);
            let o = if ordered { (p.algo.threshold(x, y, p.seed) - 0.5) * spread } else { 0. };
            let v = match p.space {
                Space::Oklab => [v[0] + o, v[1], v[2]],
                Space::Rgb => [v[0] + o, v[1] + o, v[2] + o],
            };
            nearest(v) as u8
        })
        .collect()
}

/// Two colours: dither the lightness between them as a single level, which
/// keeps the full tonal range whatever the two colours are.
fn duotone(pixels: &[Rgb], w: u32, h: u32, pal: &[[f32; 3]], p: Params, hold: Option<&Hold>) -> Vec<u8> {
    let (dark, light) = if pal[0][0] <= pal[1][0] { (0u8, 1u8) } else { (1, 0) };
    let mut level: Vec<f32> = pixels.iter().map(|&c| oklab(c)[0].clamp(0., 1.)).collect();
    let cut = 0.5 - p.bias * BIAS_REACH;
    if let Some((taps, div)) = p.algo.kernel() {
        let mut out = vec![dark; level.len()];
        let (wi, hi) = (w as i32, h as i32);
        for y in 0..hi {
            let rtl = p.serpentine && y % 2 == 1;
            for i in 0..wi {
                let x = if rtl { wi - 1 - i } else { i };
                let at = (y * wi + x) as usize;
                let v = level[at];
                let mut on = v > cut;
                if let Some(h) = hold {
                    let was = h.prev[at] == light;
                    let d = |lit: bool| (v - if lit { 1. } else { 0. }).powi(2);
                    if was != on && h.keep(at, d(was), d(on), 1.) {
                        on = was;
                    }
                }
                out[at] = if on { light } else { dark };
                let err = (v - if on { 1. } else { 0. }) * p.strength;
                for &(dx, dy, wt) in taps {
                    let (tx, ty) = (if rtl { x - dx } else { x + dx }, y + dy);
                    if (0..wi).contains(&tx) && ty < hi {
                        level[(ty * wi + tx) as usize] += err * wt / div;
                    }
                }
            }
        }
        return out;
    }
    level
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let (x, y) = (i as u32 % w, i as u32 / w);
            let t = cut + (p.algo.threshold(x, y, p.seed) - 0.5) * p.strength.min(1.);
            if v > t { light } else { dark }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BW: [Rgb; 2] = [[0., 0., 0.], [1., 1., 1.]];

    fn grey_ramp(w: u32, h: u32) -> Vec<Rgb> {
        (0..h).flat_map(|_| (0..w).map(move |x| [x as f32 / (w - 1) as f32; 3])).collect()
    }

    fn coverage(out: &[u8], w: u32, x0: u32, x1: u32, ink: u8) -> f32 {
        let hits = out.iter().enumerate().filter(|(i, v)| (*i as u32 % w) >= x0 && (*i as u32 % w) < x1 && **v == ink).count();
        hits as f32 / (out.len() as u32 / w * (x1 - x0)) as f32
    }

    #[test]
    fn bayer_matrices_use_every_level_once() {
        for order in 1..=4 {
            let side = 1u32 << order;
            let mut seen: Vec<f32> = (0..side * side).map(|i| bayer(order, i % side, i / side)).collect();
            seen.sort_by(f32::total_cmp);
            for (i, v) in seen.iter().enumerate() {
                assert!((v - (i as f32 + 0.5) / (side * side) as f32).abs() < 1e-6, "order {order}");
            }
        }
        // The classic 2×2: [[0, 2], [3, 1]].
        assert_eq!([bayer(1, 0, 0), bayer(1, 1, 0), bayer(1, 0, 1), bayer(1, 1, 1)], [0.125, 0.625, 0.875, 0.375]);
        // The 4×4 is ferrite-design's signature matrix.
        for y in 0..4 {
            for x in 0..4 {
                assert_eq!(bayer(2, x, y), ferrite_design::dither::threshold(x, y), "({x}, {y})");
            }
        }
    }

    #[test]
    fn kernels_sum_to_their_divisor() {
        for algo in Algo::ALL {
            if let Some((taps, div)) = algo.kernel() {
                let sum: f32 = taps.iter().map(|t| t.2).sum();
                let expect = if algo == Algo::Atkinson { 6. } else { div };
                assert_eq!(sum, expect, "{algo:?}");
                assert!(taps.iter().all(|&(dx, dy, _)| dy > 0 || dx > 0), "{algo:?} reaches back");
            }
        }
    }

    #[test]
    fn keys_round_trip_and_are_unique() {
        for a in Algo::ALL {
            assert_eq!(Algo::from_key(a.key()), Some(a));
        }
        let mut keys: Vec<_> = Algo::ALL.iter().map(|a| a.key()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), Algo::ALL.len());
    }

    #[test]
    fn every_algorithm_tracks_tone() {
        let (w, h) = (64, 32);
        let px = grey_ramp(w, h);
        for algo in Algo::ALL {
            let out = dither(&px, w, h, &BW, Params { algo, ..Params::default() });
            let (dark, light) = (coverage(&out, w, 0, 16, 1), coverage(&out, w, 48, 64, 1));
            assert!(light > dark + 0.5, "{algo:?}: {dark} vs {light}");
        }
    }

    #[test]
    fn diffusion_matches_mean_tone() {
        let (w, h) = (64, 64);
        let px = vec![[0.5f32; 3]; (w * h) as usize];
        for algo in Algo::ALL.into_iter().filter(|a| a.diffuses() && *a != Algo::Atkinson) {
            let out = dither(&px, w, h, &BW, Params { algo, ..Params::default() });
            let lit = out.iter().filter(|v| **v == 1).count() as f32 / out.len() as f32;
            // Oklab L of sRGB 0.5 grey is ~0.6.
            assert!((lit - 0.6).abs() < 0.05, "{algo:?}: {lit}");
        }
    }

    #[test]
    fn strength_zero_posterises() {
        let (w, h) = (32, 8);
        let out = dither(&grey_ramp(w, h), w, h, &BW, Params { strength: 0., ..Params::default() });
        // Each row is a clean step: dark then light, no speckle.
        for row in out.chunks(w as usize) {
            let flips = row.windows(2).filter(|p| p[0] != p[1]).count();
            assert_eq!(flips, 1);
        }
    }

    #[test]
    fn duotone_follows_lightness_not_order() {
        // Light ink listed first: still the bright pixels get it.
        let pal = [[1., 0.7, 0.2], [0.08, 0.08, 0.1]];
        let px = vec![[1., 1., 1.], [0., 0., 0.]];
        assert_eq!(dither(&px, 2, 1, &pal, Params::default()), vec![0, 1]);
    }

    #[test]
    fn palette_colours_are_found() {
        let pal = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.], [1., 1., 1.]];
        for space in [Space::Oklab, Space::Rgb] {
            let out = dither(&pal, 5, 1, &pal, Params { algo: Algo::Threshold, space, ..Params::default() });
            assert_eq!(out, vec![0, 1, 2, 3, 4], "{space:?}");
        }
    }

    #[test]
    fn oklab_round_trips() {
        for c in [[0.2, 0.5, 0.9], [1., 1., 1.], [0., 0., 0.], [0.9, 0.1, 0.3]] {
            let back = from_oklab(oklab(c));
            assert!(back.iter().zip(c).all(|(b, c)| (b - c).abs() < 1e-3), "{c:?} → {back:?}");
        }
        assert!((oklab([1., 1., 1.])[0] - 1.).abs() < 1e-3);
    }

    #[test]
    fn bias_moves_ordered_tone() {
        let (w, h) = (32, 32);
        let px = vec![[0.5f32; 3]; (w * h) as usize];
        let lit = |bias: f32| {
            let out = dither(&px, w, h, &BW, Params { algo: Algo::Bayer8, bias, ..Params::default() });
            out.iter().filter(|v| **v == 1).count()
        };
        assert!(lit(0.5) > lit(0.) && lit(0.) > lit(-0.5), "{} {} {}", lit(-0.5), lit(0.), lit(0.5));
        // Diffusion: the texture changes but stays a dither of the same grey.
        let fs = dither(&px, w, h, &BW, Params { bias: 0.5, ..Params::default() });
        let share = fs.iter().filter(|v| **v == 1).count() as f32 / fs.len() as f32;
        assert!((share - 0.6).abs() < 0.1, "{share}");
    }

    #[test]
    fn random_is_repeatable_per_seed() {
        let px = grey_ramp(16, 16);
        let p = Params { algo: Algo::Random, seed: 7, ..Params::default() };
        assert_eq!(dither(&px, 16, 16, &BW, p), dither(&px, 16, 16, &BW, p));
        assert_ne!(dither(&px, 16, 16, &BW, p), dither(&px, 16, 16, &BW, Params { seed: 8, ..p }));
    }
}
