//! Moving pictures: a clip is one or more frames, each a working print with
//! its delay. Stills are one-frame clips, so everything downstream treats
//! both alike.
//!
//! - **In:** GIF, APNG and animated WebP, decoded a frame at a time and
//!   shrunk as they arrive, so a long clip never sits in memory at full size.
//! - **Develop:** every frame through the same recipe, with *temporal
//!   stability*: where the photo barely changed since the last frame, the
//!   last frame's pixels are kept, so error diffusion doesn't shimmer.
//! - **Out:** GIF in the palette's exact colours, APNG, or a sprite sheet.
//!
//! Pure apart from reading the file, like `studio`.

use std::collections::HashMap;
use std::io::BufReader;
use std::path::Path;

use image::AnimationDecoder;

use crate::engine::{Params, Rgb};
use crate::render::{self, Look};
use crate::studio::{self, Adjust, Art, Print};

/// An animation's working width: smaller than a still's, since there are
/// many of them.
const ANIM_W: u32 = 480;
/// Stop decoding past this many frames or this many working pixels in all.
const MAX_FRAMES: usize = 600;
const MAX_PIXELS: u64 = 40_000_000;

pub struct Frame {
    pub print: Print,
    pub delay_ms: u32,
}

pub struct Clip {
    pub frames: Vec<Frame>,
    /// Frames were dropped to stay within the limits.
    pub truncated: bool,
}

impl Clip {
    pub fn still(print: Print) -> Clip {
        Clip { frames: vec![Frame { print, delay_ms: 100 }], truncated: false }
    }

    pub fn is_animated(&self) -> bool {
        self.frames.len() > 1
    }
}

/// Open a photo or an animation.
pub fn load(path: &Path) -> Result<Clip, String> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let open = || std::fs::File::open(path).map(BufReader::new).map_err(|e| e.to_string());
    let frames = match ext.as_str() {
        "gif" => {
            let decoder = image::codecs::gif::GifDecoder::new(open()?).map_err(|e| e.to_string())?;
            Some(take(decoder.into_frames())?)
        }
        "png" | "apng" => {
            let decoder = image::codecs::png::PngDecoder::new(open()?).map_err(|e| e.to_string())?;
            if decoder.is_apng().unwrap_or(false) {
                Some(take(decoder.apng().map_err(|e| e.to_string())?.into_frames())?)
            } else {
                None
            }
        }
        "webp" => {
            let decoder = image::codecs::webp::WebPDecoder::new(open()?).map_err(|e| e.to_string())?;
            if decoder.has_animation() { Some(take(decoder.into_frames())?) } else { None }
        }
        _ => None,
    };
    match frames {
        // A one-frame "animation" is a still; reload it at a still's size.
        Some(clip) if clip.frames.len() > 1 => Ok(clip),
        _ => studio::load(path).map(Clip::still),
    }
}

/// Decode frames one at a time, shrinking each to the working width.
fn take(frames: image::Frames<'_>) -> Result<Clip, String> {
    let mut out = Vec::new();
    let mut pixels = 0u64;
    let mut truncated = false;
    for frame in frames {
        let frame = frame.map_err(|e| format!("couldn't decode a frame: {e}"))?;
        let (num, den) = frame.delay().numer_denom_ms();
        // Browsers treat delays under 20 ms as 100 ms; so do we.
        let delay_ms = match num / den.max(1) {
            d if d < 20 => 100,
            d => d,
        };
        let print = to_print(frame.into_buffer());
        pixels += (print.w * print.h) as u64;
        if out.len() == MAX_FRAMES || pixels > MAX_PIXELS {
            truncated = true;
            break;
        }
        out.push(Frame { print, delay_ms });
    }
    if out.is_empty() {
        return Err("the animation has no frames".into());
    }
    Ok(Clip { frames: out, truncated })
}

/// An RGBA frame over white, at most `ANIM_W` wide.
fn to_print(rgba: image::RgbaImage) -> Print {
    let (w, h) = rgba.dimensions();
    let rgb = rgba
        .pixels()
        .map(|p| {
            let a = p.0[3] as f32 / 255.;
            [0, 1, 2].map(|i| p.0[i] as f32 / 255. * a + (1. - a))
        })
        .collect();
    let print = Print::from_rgb(w.max(1), h.max(1), rgb);
    if w > ANIM_W { print.resize(ANIM_W, ((h as f32 / w as f32) * ANIM_W as f32).round().max(1.) as u32) } else { print }
}

/// Develop every frame. `stability` 0..1: how readily a pixel keeps last
/// frame's colour where the photo stood still (see `engine::Hold`).
pub fn develop_all(frames: &[Frame], cols: u32, adjust: Adjust, palette: &[Rgb], params: Params, stability: f32) -> Vec<Art> {
    let mut arts: Vec<Art> = Vec::with_capacity(frames.len());
    for frame in frames {
        let prev = arts.last().filter(|_| stability > 0.).map(|a| (a, stability));
        let art = studio::develop_held(&frame.print, cols, adjust, palette.to_vec(), params, prev);
        arts.push(art);
    }
    arts
}

/// Delays after a speed change, in ms.
pub fn delays(frames: &[Frame], speed: f32) -> Vec<u32> {
    frames.iter().map(|f| ((f.delay_ms as f32 / speed.max(0.05)).round() as u32).max(10)).collect()
}

/// An animated GIF in exactly the art's colours (plus a transparent slot
/// for transparent paper), looping forever.
pub fn gif(arts: &[Art], delays: &[u32], scale: u32, look: Look) -> Result<Vec<u8>, String> {
    let first = arts.first().ok_or("nothing to export")?;
    let (w, h, _) = render::rgba(first, scale, look);
    if w > u16::MAX as u32 || h > u16::MAX as u32 {
        return Err("too big for a GIF; use a smaller pixel size".into());
    }
    let mut colors: Vec<[u8; 4]> = first.colors.iter().map(|c| { let [r, g, b] = c.map(|v| (v.clamp(0., 1.) * 255.).round() as u8); [r, g, b, 255] }).collect();
    let transparent = look.transparent.then(|| {
        colors.push([0, 0, 0, 0]);
        (colors.len() - 1) as u8
    });
    if colors.len() > 256 {
        return Err("a GIF holds 256 colours; with transparent paper the palette can have 255".into());
    }
    let lookup: HashMap<[u8; 4], u8> = colors.iter().enumerate().map(|(i, c)| (*c, i as u8)).collect();
    let global: Vec<u8> = colors.iter().flat_map(|c| [c[0], c[1], c[2]]).collect();

    let mut out = Vec::new();
    {
        let mut enc = gif::Encoder::new(&mut out, w as u16, h as u16, &global).map_err(|e| e.to_string())?;
        enc.set_repeat(gif::Repeat::Infinite).map_err(|e| e.to_string())?;
        for (art, &delay) in arts.iter().zip(delays) {
            let (_, _, px) = render::rgba(art, scale, look);
            let index: Vec<u8> = px
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| {
                    let key = if p[3] == 0 { [0, 0, 0, 0] } else { [p[0], p[1], p[2], 255] };
                    lookup.get(&key).copied().unwrap_or(0)
                })
                .collect();
            let mut frame = gif::Frame::from_indexed_pixels(w as u16, h as u16, index, transparent);
            frame.delay = ((delay + 5) / 10).clamp(2, u16::MAX as u32) as u16;
            // Each frame covers the whole canvas.
            frame.dispose = gif::DisposalMethod::Background;
            enc.write_frame(&frame).map_err(|e| e.to_string())?;
        }
    }
    Ok(out)
}

/// An animated PNG: full colour and alpha, any palette size.
pub fn apng(arts: &[Art], delays: &[u32], scale: u32, look: Look) -> Result<Vec<u8>, String> {
    let first = arts.first().ok_or("nothing to export")?;
    let (w, h, _) = render::rgba(first, scale, look);
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_animated(arts.len() as u32, 0).map_err(|e| e.to_string())?;
        let mut writer = enc.write_header().map_err(|e| e.to_string())?;
        for (art, &delay) in arts.iter().zip(delays) {
            writer.set_frame_delay(delay.min(u16::MAX as u32) as u16, 1000).map_err(|e| e.to_string())?;
            let (_, _, px) = render::rgba(art, scale, look);
            writer.write_image_data(&px).map_err(|e| e.to_string())?;
        }
        writer.finish().map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// Every frame on one PNG, left to right then down, as near square as fits.
pub fn sprite_sheet(arts: &[Art], scale: u32, look: Look) -> Result<Vec<u8>, String> {
    let first = arts.first().ok_or("nothing to export")?;
    let (fw, fh, _) = render::rgba(first, scale, look);
    let per_row = (arts.len() as f32).sqrt().ceil() as u32;
    let rows = (arts.len() as u32).div_ceil(per_row);
    let mut sheet = image::RgbaImage::new(fw * per_row, fh * rows);
    for (i, art) in arts.iter().enumerate() {
        let (_, _, px) = render::rgba(art, scale, look);
        let tile = image::RgbaImage::from_raw(fw, fh, px).ok_or("frame size mismatch")?;
        let (x, y) = ((i as u32 % per_row) * fw, (i as u32 / per_row) * fh);
        image::imageops::replace(&mut sheet, &tile, x as i64, y as i64);
    }
    let mut out = std::io::Cursor::new(Vec::new());
    sheet.write_to(&mut out, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Algo;

    const BW: [Rgb; 2] = [[0., 0., 0.], [1., 1., 1.]];

    /// A bright square moving right across a dark field, `n` frames.
    fn moving(n: usize) -> Vec<Frame> {
        (0..n)
            .map(|k| {
                let (w, h) = (32u32, 16u32);
                let rgb = (0..w * h)
                    .map(|i| {
                        let (x, y) = (i % w, i / w);
                        let lit = x >= k as u32 * 4 && x < k as u32 * 4 + 6 && (4..12).contains(&y);
                        [if lit { 0.9 } else { 0.35 }; 3]
                    })
                    .collect();
                Frame { print: Print::from_rgb(w, h, rgb), delay_ms: 80 }
            })
            .collect()
    }

    fn params(algo: Algo) -> Params {
        Params { algo, ..Params::default() }
    }

    #[test]
    fn stability_calms_still_areas() {
        let frames = moving(6);
        let shimmer = |stability: f32| {
            let arts = develop_all(&frames, 32, Adjust::default(), &BW, params(Algo::FloydSteinberg), stability);
            // Pixels in the untouched bottom rows that change between frames.
            arts.windows(2).map(|p| (32 * 13..32 * 16).filter(|&i| p[0].index[i] != p[1].index[i]).count()).sum::<usize>()
        };
        assert!(shimmer(1.) * 2 <= shimmer(0.), "{} vs {}", shimmer(1.), shimmer(0.));
    }

    #[test]
    fn stability_leaves_no_trail() {
        // A white square crosses pure black; once it has gone, the black is clean.
        let frames: Vec<Frame> = (0..6)
            .map(|k| {
                let rgb = (0..32 * 16u32)
                    .map(|i| {
                        let (x, y) = (i % 32, i / 32);
                        let lit = k < 3 && x >= k * 4 && x < k * 4 + 8 && (4..12).contains(&y);
                        [if lit { 1. } else { 0. }; 3]
                    })
                    .collect();
                Frame { print: Print::from_rgb(32, 16, rgb), delay_ms: 80 }
            })
            .collect();
        let arts = develop_all(&frames, 32, Adjust::default(), &BW, params(Algo::FloydSteinberg), 1.);
        assert!(arts[5].index.iter().all(|&i| i == 0), "a trail was left behind");
    }

    #[test]
    fn gif_round_trips_frames_and_colours() {
        let frames = moving(3);
        let arts = develop_all(&frames, 32, Adjust::default(), &BW, params(Algo::Bayer4), 0.);
        let bytes = gif(&arts, &delays(&frames, 1.), 2, Look::default()).unwrap();
        let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes)).unwrap();
        let out = decoder.into_frames().collect_frames().unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].buffer().dimensions(), (64, 32));
        assert_eq!(out[0].delay().numer_denom_ms(), (80, 1));
        assert!(out.iter().all(|f| f.buffer().pixels().all(|p| p.0 == [0, 0, 0, 255] || p.0 == [255, 255, 255, 255])));
        // The art in the GIF is the art we developed.
        let (_, _, px) = render::rgba(&arts[1], 2, Look::default());
        assert_eq!(out[1].buffer().as_raw(), &px);
    }

    #[test]
    fn gif_transparent_paper() {
        let frames = moving(2);
        let arts = develop_all(&frames, 32, Adjust::default(), &BW, params(Algo::Bayer4), 0.);
        let look = Look { transparent: true, ..Look::default() };
        let bytes = gif(&arts, &delays(&frames, 1.), 1, look).unwrap();
        let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes)).unwrap();
        let out = decoder.into_frames().collect_frames().unwrap();
        assert!(out[0].buffer().pixels().any(|p| p.0[3] == 0));
    }

    #[test]
    fn apng_round_trips() {
        let frames = moving(3);
        let arts = develop_all(&frames, 32, Adjust::default(), &BW, params(Algo::Atkinson), 0.5);
        let bytes = apng(&arts, &delays(&frames, 2.), 1, Look::default()).unwrap();
        let decoder = image::codecs::png::PngDecoder::new(std::io::Cursor::new(bytes)).unwrap();
        assert!(decoder.is_apng().unwrap());
        let out = decoder.apng().unwrap().into_frames().collect_frames().unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(out[2].delay().numer_denom_ms(), (40, 1));
    }

    #[test]
    fn sprite_sheet_is_a_grid() {
        let frames = moving(5);
        let arts = develop_all(&frames, 32, Adjust::default(), &BW, params(Algo::Bayer4), 0.);
        let img = image::load_from_memory(&sprite_sheet(&arts, 1, Look::default()).unwrap()).unwrap();
        // Five frames: three to a row, two rows.
        assert_eq!((img.width(), img.height()), (96, 32));
    }

    #[test]
    fn decoding_shrinks_and_reads_delays() {
        // Write a two-frame GIF wider than ANIM_W, then load it back.
        let frames: Vec<Frame> = (0..2)
            .map(|k| Frame { print: Print::from_rgb(600, 20, vec![[k as f32; 3]; 600 * 20]), delay_ms: 120 })
            .collect();
        let arts: Vec<Art> = frames.iter().map(|f| studio::develop(&f.print, 600, Adjust::default(), BW.to_vec(), params(Algo::Threshold))).collect();
        let bytes = gif(&arts, &delays(&frames, 1.), 1, Look::default()).unwrap();
        let path = std::env::temp_dir().join(format!("darkroom-test-{}.gif", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        let clip = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(clip.is_animated() && !clip.truncated);
        assert_eq!(clip.frames.len(), 2);
        assert_eq!(clip.frames[0].print.w, ANIM_W);
        assert_eq!(clip.frames[1].delay_ms, 120);
        assert!(clip.frames[1].print.lum[0] > 0.9 && clip.frames[0].print.lum[0] < 0.1);
    }

    #[test]
    fn speed_scales_delays() {
        let frames = moving(2);
        assert_eq!(delays(&frames, 2.), vec![40, 40]);
        assert_eq!(delays(&frames, 0.5), vec![160, 160]);
    }
}
