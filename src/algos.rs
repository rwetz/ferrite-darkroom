//! The engine's less common algorithms, each on vectors in the matching
//! space with a `nearest` that picks a palette index:
//!
//! - **Riemersma:** error diffusion along a Hilbert curve, carrying a short,
//!   fading history of errors instead of pushing error to fixed neighbours.
//!   No directional artefacts.
//! - **Dot diffusion** (Knuth): pixels are processed in the order of an 8×8
//!   class matrix, and each passes its error only to neighbours of a later
//!   class: error diffusion that parallelises like an ordered dither.
//! - **Knoll** and **Yliluoma**: pattern dithers for many-colour palettes.
//!   For each pixel, build a plan of palette colours whose mix matches it,
//!   sort the plan by lightness, and let a Bayer matrix choose from it.
//! - **CMYK halftone:** the picture separated into cyan, magenta, yellow and
//!   black, each screened with round dots at its own angle, then mixed.
//! - **Stippling:** points placed by tone, then relaxed by weighted Lloyd
//!   iterations (weighted Voronoi stippling), so they space out evenly.
//!
//! Pure.

use std::collections::{HashMap, VecDeque};

use crate::engine::Rgb;

pub type Vec3 = [f32; 3];

fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: Vec3, k: f32) -> Vec3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn d2(a: Vec3, b: Vec3) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

// ── Riemersma ────────────────────────────────────────────────────────────

/// The point `d` along a Hilbert curve filling an `n`×`n` square (`n` a
/// power of two).
fn hilbert_point(n: u32, d: u32) -> (u32, u32) {
    let (mut x, mut y, mut t) = (0u32, 0u32, d);
    let mut s = 1;
    while s < n {
        let rx = 1 & (t / 2);
        let ry = 1 & (t ^ rx);
        if ry == 0 {
            if rx == 1 {
                x = s - 1 - x;
                y = s - 1 - y;
            }
            std::mem::swap(&mut x, &mut y);
        }
        x += s * rx;
        y += s * ry;
        t /= 4;
        s *= 2;
    }
    (x, y)
}

/// Every pixel of a `w`×`h` picture in Hilbert-curve order.
pub fn hilbert(w: u32, h: u32) -> impl Iterator<Item = (u32, u32)> {
    let n = w.max(h).max(1).next_power_of_two();
    (0..n * n).map(move |d| hilbert_point(n, d)).filter(move |&(x, y)| x < w && y < h)
}

pub fn riemersma(work: &[Vec3], w: u32, h: u32, pal: &[Vec3], nearest: &dyn Fn(Vec3) -> usize, strength: f32) -> Vec<u8> {
    const HISTORY: usize = 16;
    // The newest error weighs 1, the oldest 1/16, falling off exponentially.
    let weights: Vec<f32> = (0..HISTORY).map(|i| 16f32.powf((i as f32 - (HISTORY - 1) as f32) / (HISTORY - 1) as f32)).collect();
    let total: f32 = weights.iter().sum();
    let mut history: VecDeque<Vec3> = VecDeque::from(vec![[0.; 3]; HISTORY]);
    let mut out = vec![0u8; work.len()];
    for (x, y) in hilbert(w, h) {
        let at = (y * w + x) as usize;
        let carried = history.iter().zip(&weights).fold([0.; 3], |acc, (e, &k)| add(acc, scale(*e, k / total)));
        let v = add(work[at], scale(carried, strength));
        let q = nearest(v);
        out[at] = q as u8;
        history.pop_front();
        history.push_back(sub(v, pal[q]));
    }
    out
}

// ── Dot diffusion ────────────────────────────────────────────────────────

/// Knuth's class matrix: each 8×8 position's turn, 0 first.
const CLASSES: [[u8; 8]; 8] = [
    [34, 48, 40, 32, 29, 15, 23, 31],
    [42, 58, 56, 53, 21, 5, 7, 10],
    [50, 62, 61, 45, 13, 1, 2, 18],
    [38, 46, 54, 37, 25, 17, 9, 26],
    [28, 14, 22, 30, 35, 49, 41, 33],
    [20, 4, 6, 11, 43, 59, 57, 52],
    [12, 0, 3, 19, 51, 63, 60, 44],
    [24, 16, 8, 27, 39, 47, 55, 36],
];

pub fn dot_diffusion(work: &[Vec3], w: u32, h: u32, pal: &[Vec3], nearest: &dyn Fn(Vec3) -> usize, strength: f32) -> Vec<u8> {
    let mut work = work.to_vec();
    let mut out = vec![0u8; work.len()];
    let class = |x: u32, y: u32| CLASSES[(y % 8) as usize][(x % 8) as usize];
    // Where each class sits in the tile, so a pass visits only its pixels.
    let mut at_class: Vec<(u32, u32)> = vec![(0, 0); 64];
    for (ty, row) in CLASSES.iter().enumerate() {
        for (tx, &c) in row.iter().enumerate() {
            at_class[c as usize] = (tx as u32, ty as u32);
        }
    }
    for (k, &(tx, ty)) in at_class.iter().enumerate() {
        for y in (ty..h).step_by(8) {
            for x in (tx..w).step_by(8) {
                let at = (y * w + x) as usize;
                let v = work[at];
                let q = nearest(v);
                out[at] = q as u8;
                let err = scale(sub(v, pal[q]), strength);
                // Pass the error to later-class neighbours: 2 for edges, 1 for corners.
                let mut taps: Vec<(usize, f32)> = Vec::with_capacity(8);
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                        if (dx, dy) == (0, 0) || nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                            continue;
                        }
                        if class(nx as u32, ny as u32) as usize > k {
                            taps.push(((ny as u32 * w + nx as u32) as usize, if dx == 0 || dy == 0 { 2. } else { 1. }));
                        }
                    }
                }
                let sum: f32 = taps.iter().map(|t| t.1).sum();
                for (i, wt) in taps {
                    work[i] = add(work[i], scale(err, wt / sum));
                }
            }
        }
    }
    out
}

// ── Pattern dithers ──────────────────────────────────────────────────────

const PLAN: usize = 16;

fn bayer4_rank(x: u32, y: u32) -> usize {
    const M: [[usize; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
    M[(y % 4) as usize][(x % 4) as usize]
}

/// Knoll's pattern dither: build the plan by repeatedly picking the nearest
/// colour to the pixel plus the error so far.
pub fn knoll(work: &[Vec3], w: u32, pal: &[Vec3], nearest: &dyn Fn(Vec3) -> usize, luma: &dyn Fn(Vec3) -> f32, strength: f32) -> Vec<u8> {
    let multiplier = 0.5 * strength.max(0.05);
    work.iter()
        .enumerate()
        .map(|(i, &v)| {
            let mut err = [0.; 3];
            let mut plan = [0usize; PLAN];
            for slot in &mut plan {
                let c = nearest(add(v, scale(err, multiplier)));
                *slot = c;
                err = add(err, sub(v, pal[c]));
            }
            plan.sort_by(|&a, &b| luma(pal[a]).total_cmp(&luma(pal[b])));
            plan[bayer4_rank(i as u32 % w, i as u32 / w)] as u8
        })
        .collect()
}

/// Yliluoma's algorithm 2: build the plan by adding, each step, the colour
/// that brings the plan's running mean closest to the pixel.
pub fn yliluoma(work: &[Vec3], w: u32, pal: &[Vec3], luma: &dyn Fn(Vec3) -> f32) -> Vec<u8> {
    let mut plans: HashMap<[i16; 3], [u8; PLAN]> = HashMap::new();
    work.iter()
        .enumerate()
        .map(|(i, &v)| {
            let key = v.map(|c| (c * 256.).round() as i16);
            let plan = *plans.entry(key).or_insert_with(|| {
                let mut sum = [0.; 3];
                let mut plan = [0u8; PLAN];
                for (count, slot) in plan.iter_mut().enumerate() {
                    let n = (count + 1) as f32;
                    let best = (0..pal.len()).min_by(|&a, &b| d2(scale(add(sum, pal[a]), 1. / n), v).total_cmp(&d2(scale(add(sum, pal[b]), 1. / n), v))).unwrap_or(0);
                    *slot = best as u8;
                    sum = add(sum, pal[best]);
                }
                plan.sort_by(|&a, &b| luma(pal[a as usize]).total_cmp(&luma(pal[b as usize])));
                plan
            });
            plan[bayer4_rank(i as u32 % w, i as u32 / w)]
        })
        .collect()
}

// ── CMYK halftone ────────────────────────────────────────────────────────

/// Separate into CMYK (full black replacement), screen each ink with round
/// dots at the classic angles (C 15°, M 75°, Y 0°, K 45°) and `period`
/// pixels apart, and mix the inks back to a colour.
pub fn cmyk(pixels: &[Rgb], w: u32, period: f32) -> Vec<Rgb> {
    let angles = [15f32, 75., 0., 45.].map(f32::to_radians);
    let tau = std::f32::consts::TAU;
    let screen = |x: f32, y: f32, a: f32| {
        let (s, c) = a.sin_cos();
        let (u, v) = ((x * c + y * s) / period, (-x * s + y * c) / period);
        // 0 at a dot's centre, 1 between dots: the dot grows with the ink.
        0.5 - 0.25 * ((tau * u).cos() + (tau * v).cos())
    };
    pixels
        .iter()
        .enumerate()
        .map(|(i, &[r, g, b])| {
            let (x, y) = ((i as u32 % w) as f32 + 0.5, (i as u32 / w) as f32 + 0.5);
            let (c, m, ye) = (1. - r, 1. - g, 1. - b);
            let k = c.min(m).min(ye);
            let sep = |v: f32| if k < 1. { (v - k) / (1. - k) } else { 0. };
            let inks = [sep(c), sep(m), sep(ye), k];
            let on: Vec<bool> = inks.iter().zip(angles).map(|(&amount, a)| amount > screen(x, y, a)).collect();
            if on[3] {
                [0., 0., 0.]
            } else {
                [!on[0] as u8 as f32, !on[1] as u8 as f32, !on[2] as u8 as f32]
            }
        })
        .collect()
}

// ── Stippling ────────────────────────────────────────────────────────────

fn hash(i: u32, seed: u32) -> f32 {
    let mut h = i.wrapping_mul(0x9e37_79b1) ^ seed.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// Weighted Voronoi stippling: one point per unit of `level` (0..1 a
/// pixel), placed by rejection sampling, then moved to the weighted
/// centroid of its cell a few times. Returns which pixels hold a point.
pub fn stipple(level: &[f32], w: u32, h: u32, seed: u32) -> Vec<bool> {
    let n = level.len();
    let total: f32 = level.iter().sum();
    let count = (total.round() as usize).min(n);
    let mut out = vec![false; n];
    if count == 0 {
        return out;
    }
    let mut points: Vec<(f32, f32)> = Vec::with_capacity(count);
    let mut tries = 0u32;
    while points.len() < count && tries < (count as u32).saturating_mul(60) {
        let i = (hash(tries * 2, seed) * n as f32) as usize % n;
        if hash(tries * 2 + 1, seed) < level[i] {
            points.push(((i as u32 % w) as f32 + 0.5, (i as u32 / w) as f32 + 0.5));
        }
        tries += 1;
    }
    // A grid of buckets about two points wide, for nearest-point lookups.
    let spacing = ((w * h) as f32 / points.len().max(1) as f32).sqrt().max(1.);
    let cell = spacing * 2.;
    let (gw, gh) = (((w as f32) / cell).ceil() as usize + 1, ((h as f32) / cell).ceil() as usize + 1);
    for _ in 0..4 {
        let mut grid: Vec<Vec<usize>> = vec![Vec::new(); gw * gh];
        for (k, &(px, py)) in points.iter().enumerate() {
            grid[(py / cell) as usize * gw + (px / cell) as usize].push(k);
        }
        let mut acc = vec![(0f32, 0f32, 0f32); points.len()];
        for (i, &weight) in level.iter().enumerate() {
            if weight <= 0. {
                continue;
            }
            let (x, y) = ((i as u32 % w) as f32 + 0.5, (i as u32 / w) as f32 + 0.5);
            let (gx, gy) = ((x / cell) as i32, (y / cell) as i32);
            let mut best = (usize::MAX, f32::MAX);
            for ring in 1..=3 {
                for by in gy - ring..=gy + ring {
                    for bx in gx - ring..=gx + ring {
                        if bx < 0 || by < 0 || bx as usize >= gw || by as usize >= gh {
                            continue;
                        }
                        for &k in &grid[by as usize * gw + bx as usize] {
                            let d = (points[k].0 - x).powi(2) + (points[k].1 - y).powi(2);
                            if d < best.1 {
                                best = (k, d);
                            }
                        }
                    }
                }
                if best.0 != usize::MAX {
                    break;
                }
            }
            if best.0 != usize::MAX {
                let a = &mut acc[best.0];
                *a = (a.0 + x * weight, a.1 + y * weight, a.2 + weight);
            }
        }
        for (p, a) in points.iter_mut().zip(&acc) {
            if a.2 > 0. {
                *p = (a.0 / a.2, a.1 / a.2);
            }
        }
    }
    for (px, py) in points {
        let (x, y) = ((px as u32).min(w - 1), (py as u32).min(h - 1));
        out[(y * w + x) as usize] = true;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hilbert_visits_every_pixel_once_in_steps_of_one() {
        let (w, h) = (13, 7);
        let path: Vec<(u32, u32)> = hilbert(w, h).collect();
        assert_eq!(path.len(), (w * h) as usize);
        let mut seen = std::collections::HashSet::new();
        assert!(path.iter().all(|p| seen.insert(*p)));
        // On a full power-of-two square, every step moves to a neighbour.
        let full: Vec<(u32, u32)> = hilbert(8, 8).collect();
        assert!(full.windows(2).all(|p| p[0].0.abs_diff(p[1].0) + p[0].1.abs_diff(p[1].1) == 1));
    }

    #[test]
    fn classes_are_a_permutation() {
        let mut all: Vec<u8> = CLASSES.iter().flatten().copied().collect();
        all.sort();
        assert_eq!(all, (0..64).collect::<Vec<u8>>());
    }

    #[test]
    fn cmyk_separates() {
        let px = vec![[1., 1., 1.], [0., 0., 0.], [0., 1., 1.]];
        let out = cmyk(&px, 3, 6.);
        assert_eq!(out[0], [1., 1., 1.], "white takes no ink");
        assert_eq!(out[1], [0., 0., 0.], "black is all black ink");
        // Pure cyan: everywhere cyan or bare paper, never magenta or yellow.
        assert!(out[2] == [0., 1., 1.] || out[2] == [1., 1., 1.]);
    }

    #[test]
    fn stipple_places_as_many_points_as_tone() {
        let (w, h) = (40, 40);
        let level: Vec<f32> = (0..w * h).map(|i| if i % w < 20 { 0.05 } else { 0.3 }).collect();
        let pts = stipple(&level, w, h, 3);
        let (left, right) = pts.iter().enumerate().fold((0, 0), |(l, r), (i, &on)| if !on { (l, r) } else if (i as u32 % w) < 20 { (l + 1, r) } else { (l, r + 1) });
        assert!(right > left * 3, "{left} vs {right}");
        let expected = level.iter().sum::<f32>();
        let got = (left + right) as f32;
        assert!((got - expected).abs() / expected < 0.2, "{got} vs {expected}");
    }
}
