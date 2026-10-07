//! Still exports: which file format, and how big.
//!
//! The size is either a whole number of pixels per art pixel (crisp, the
//! recipe's `scale`) or a target long edge (1080p … 8K, or any width up to
//! [`MAX_EDGE`]). Pure, so the app and the command line agree.

use crate::engine::Rgb;

/// The largest long edge an export may have (8K and a little).
pub const MAX_EDGE: u32 = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Format {
    #[default]
    Png,
    Jpeg,
    Webp,
    Bmp,
    Tiff,
    Svg,
}

impl Format {
    pub const ALL: [Format; 6] = [Format::Png, Format::Jpeg, Format::Webp, Format::Bmp, Format::Tiff, Format::Svg];

    pub fn key(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpeg",
            Format::Webp => "webp",
            Format::Bmp => "bmp",
            Format::Tiff => "tiff",
            Format::Svg => "svg",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Format::Png => "PNG",
            Format::Jpeg => "JPEG",
            Format::Webp => "WebP",
            Format::Bmp => "BMP",
            Format::Tiff => "TIFF",
            Format::Svg => "SVG",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Format::Png => "Lossless, keeps transparency. The best default",
            Format::Jpeg => "Small, lossy: smears hard pixel edges. For sharing photos",
            Format::Webp => "Lossless and smaller than PNG; keeps transparency",
            Format::Bmp => "Uncompressed, for old tools",
            Format::Tiff => "Lossless, for print and photo editors",
            Format::Svg => "Vector: sharp at any size, for plotters and print",
        }
    }

    pub fn ext(self) -> &'static str {
        match self {
            Format::Jpeg => "jpg",
            Format::Tiff => "tif",
            f => f.key(),
        }
    }

    pub fn from_key(key: &str) -> Option<Format> {
        match key {
            "jpg" => Some(Format::Jpeg),
            "tif" => Some(Format::Tiff),
            _ => Format::ALL.into_iter().find(|f| f.key() == key),
        }
    }

    /// Whether the format keeps an alpha channel.
    pub fn alpha(self) -> bool {
        matches!(self, Format::Png | Format::Webp | Format::Tiff | Format::Svg)
    }
}

/// How big an export is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Size {
    /// The recipe's pixels per art pixel.
    #[default]
    Scale,
    /// This long edge in pixels.
    Long(u32),
}

/// The named sizes, by long edge.
pub const PRESETS: [(&str, u32); 6] = [("HD", 1280), ("Full HD", 1920), ("QHD", 2560), ("4K", 3840), ("5K", 5120), ("8K", 7680)];

impl Size {
    pub fn key(self) -> String {
        match self {
            Size::Scale => "scale".into(),
            Size::Long(n) => n.to_string(),
        }
    }

    /// `scale`, a pixel count, or a preset name (`4k`, `8K`, `1080p`).
    pub fn parse(s: &str) -> Option<Size> {
        let s = s.trim().to_lowercase();
        let long = match s.as_str() {
            "scale" => return Some(Size::Scale),
            "720p" | "hd" => 1280,
            "1080p" | "fhd" => 1920,
            "1440p" | "qhd" | "2k" => 2560,
            "2160p" | "4k" | "uhd" => 3840,
            "5k" => 5120,
            "4320p" | "8k" => 7680,
            n => n.trim_end_matches("px").parse().ok()?,
        };
        Some(Size::Long(long.clamp(16, MAX_EDGE)))
    }

    /// The output size for content `w`×`h` units across (art pixels, or a
    /// text art's 8×16 cells). A long edge never goes below one pixel per
    /// unit, so nothing is dropped.
    pub fn dims(self, w: u32, h: u32, scale: u32) -> (u32, u32) {
        let (w, h) = (w.max(1), h.max(1));
        match self {
            Size::Scale => (w * scale.max(1), h * scale.max(1)),
            Size::Long(long) => {
                let long = long.max(w.max(h)) as f32;
                let k = long / w.max(h) as f32;
                (((w as f32 * k).round() as u32).max(1), ((h as f32 * k).round() as u32).max(1))
            }
        }
    }
}

/// The shapes a picture can be framed to, by name: screens, phones, prints.
pub const ASPECTS: [(&str, (u32, u32)); 8] = [
    ("16:9 screen", (16, 9)),
    ("16:10 screen", (16, 10)),
    ("21:9 ultrawide", (21, 9)),
    ("32:9 super ultrawide", (32, 9)),
    ("4:3", (4, 3)),
    ("1:1 square", (1, 1)),
    ("4:5 portrait", (4, 5)),
    ("9:16 phone", (9, 16)),
];

/// What shape the picture comes out: the photo's own, or a fixed aspect,
/// either filled (the photo cropped to it before developing, so nothing
/// is wasted) or fitted (the whole picture, padded with paper).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Frame {
    pub aspect: Option<(u32, u32)>,
    pub fill: bool,
}

impl Frame {
    pub fn key(self) -> String {
        match self.aspect {
            None => "photo".into(),
            Some((w, h)) => format!("{w}:{h}{}", if self.fill { "" } else { "-fit" }),
        }
    }

    /// `photo`, `16:9` (fill), `16:9-fit`, or `21x9`.
    pub fn parse(s: &str) -> Option<Frame> {
        let s = s.trim().to_lowercase();
        if s == "photo" || s.is_empty() {
            return Some(Frame::default());
        }
        let (ratio, fill) = match s.strip_suffix("-fit") {
            Some(r) => (r, false),
            None => (s.strip_suffix("-fill").unwrap_or(&s), true),
        };
        let (w, h) = ratio.split_once([':', 'x'])?;
        let (w, h): (u32, u32) = (w.trim().parse().ok()?, h.trim().parse().ok()?);
        (w > 0 && h > 0 && w <= 100 && h <= 100).then_some(Frame { aspect: Some((w, h)), fill })
    }

    /// The part of a `w`×`h` picture (0..1 each way: x, y, width, height)
    /// that a fill keeps: the largest centred piece of the frame's shape.
    pub fn crop(self, w: u32, h: u32) -> [f32; 4] {
        let Some((aw, ah)) = self.aspect.filter(|_| self.fill) else { return [0., 0., 1., 1.] };
        let (want, have) = (aw as f32 / ah as f32, w.max(1) as f32 / h.max(1) as f32);
        if have > want {
            let k = want / have;
            [(1. - k) / 2., 0., k, 1.]
        } else {
            let k = have / want;
            [0., (1. - k) / 2., 1., k]
        }
    }

    /// The canvas for content `w`×`h` units at `size`, and where the
    /// content sits on it (x, y, width, height in pixels). Framed pictures
    /// come out at exactly the frame's shape: filled content is stretched
    /// the last pixel or two to meet it, fitted content is centred.
    pub fn canvas(self, w: u32, h: u32, size: Size, scale: u32) -> ((u32, u32), [u32; 4]) {
        let (cw, ch) = size.dims(w, h, scale);
        let Some((aw, ah)) = self.aspect else { return ((cw, ch), [0, 0, cw, ch]) };
        let long = cw.max(ch) as f32;
        let (fw, fh) = if aw >= ah { (long, long * ah as f32 / aw as f32) } else { (long * aw as f32 / ah as f32, long) };
        let (fw, fh) = ((fw.round() as u32).max(1), (fh.round() as u32).max(1));
        if self.fill {
            return ((fw, fh), [0, 0, fw, fh]);
        }
        // Fit: the content as big as goes inside, centred.
        let k = (fw as f32 / cw as f32).min(fh as f32 / ch as f32);
        let (iw, ih) = (((cw as f32 * k).round() as u32).clamp(1, fw), ((ch as f32 * k).round() as u32).clamp(1, fh));
        ((fw, fh), [(fw - iw) / 2, (fh - ih) / 2, iw, ih])
    }
}

/// `content` (RGBA, `rect`'s size) on a `w`×`h` canvas of `pad`.
pub fn place(content: Vec<u8>, (w, h): (u32, u32), rect: [u32; 4], pad: [u8; 4]) -> Vec<u8> {
    if rect == [0, 0, w, h] {
        return content;
    }
    let mut out: Vec<u8> = std::iter::repeat_n(pad, (w * h) as usize).flatten().collect();
    let [x, y, cw, ch] = rect;
    for row in 0..ch.min(h - y) {
        let src = &content[(row * cw * 4) as usize..][..(cw.min(w - x) * 4) as usize];
        out[(((y + row) * w + x) * 4) as usize..][..src.len()].copy_from_slice(src);
    }
    out
}

/// `#rrggbb`.
pub fn hex(c: Rgb) -> String {
    let [r, g, b] = c.map(|v| (v.clamp(0., 1.) * 255.).round() as u8);
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// RGBA pixels as `format`. JPEG and BMP have no alpha: transparent pixels
/// are laid on `matte`.
pub fn encode(w: u32, h: u32, mut rgba: Vec<u8>, format: Format, matte: Rgb) -> Result<Vec<u8>, String> {
    use image::ImageFormat as F;
    let mut out = std::io::Cursor::new(Vec::new());
    let target = match format {
        Format::Png => F::Png,
        Format::Jpeg => F::Jpeg,
        Format::Webp => F::WebP,
        Format::Bmp => F::Bmp,
        Format::Tiff => F::Tiff,
        Format::Svg => return Err("SVG is drawn, not encoded".into()),
    };
    if !format.alpha() {
        let m = matte.map(|v| v.clamp(0., 1.) * 255.);
        let rgb: Vec<u8> = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| {
                let a = p[3] as f32 / 255.;
                [0, 1, 2].map(|i| (p[i] as f32 * a + m[i] * (1. - a)).round() as u8)
            })
            .collect();
        let img = image::RgbImage::from_raw(w, h, rgb).ok_or("image size mismatch")?;
        if format == Format::Jpeg {
            // High quality: dither patterns are all edges.
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 95).encode_image(&img).map_err(|e| e.to_string())?;
        } else {
            img.write_to(&mut out, target).map_err(|e| e.to_string())?;
        }
        return Ok(out.into_inner());
    }
    rgba.truncate((w * h * 4) as usize);
    let img = image::RgbaImage::from_raw(w, h, rgba).ok_or("image size mismatch")?;
    img.write_to(&mut out, target).map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(Size::Scale.dims(160, 90, 4), (640, 360));
        assert_eq!(Size::Long(7680).dims(160, 90, 4), (7680, 4320));
        // Portrait: the long edge is the height.
        assert_eq!(Size::Long(3840).dims(90, 160, 1), (2160, 3840));
        // Never fewer pixels than art pixels.
        assert_eq!(Size::Long(100).dims(400, 200, 1), (400, 200));
    }

    #[test]
    fn parses_names() {
        assert_eq!(Size::parse("8K"), Some(Size::Long(7680)));
        assert_eq!(Size::parse("1080p"), Some(Size::Long(1920)));
        assert_eq!(Size::parse("3000px"), Some(Size::Long(3000)));
        assert_eq!(Size::parse("99999"), Some(Size::Long(MAX_EDGE)));
        assert_eq!(Size::parse("scale"), Some(Size::Scale));
        assert_eq!(Size::parse("big"), None);
        for f in Format::ALL {
            assert_eq!(Format::from_key(f.key()), Some(f));
            assert_eq!(Format::from_key(f.ext()), Some(f));
        }
    }

    #[test]
    fn frames() {
        let wide = Frame { aspect: Some((16, 9)), fill: true };
        // A 3:2 photo filled to 16:9 loses some top and bottom.
        let [x, y, w, h] = wide.crop(1200, 800);
        assert!(x == 0. && w == 1. && y > 0. && (h * 800. * 16. / 9. - 1200.).abs() < 1.);
        // 4K, exactly.
        assert_eq!(wide.canvas(160, 91, Size::Long(3840), 1), ((3840, 2160), [0, 0, 3840, 2160]));
        // Fit pads: a square in a 16:9 frame sits in the middle.
        let fit = Frame { aspect: Some((16, 9)), fill: false };
        assert_eq!(fit.canvas(100, 100, Size::Long(1920), 1), ((1920, 1080), [420, 0, 1080, 1080]));
        assert_eq!(Frame::default().canvas(10, 5, Size::Scale, 2), ((20, 10), [0, 0, 20, 10]));
        for f in [wide, fit, Frame::default(), Frame { aspect: Some((9, 16)), fill: true }] {
            assert_eq!(Frame::parse(&f.key()), Some(f));
        }
        assert_eq!(Frame::parse("21x9"), Some(Frame { aspect: Some((21, 9)), fill: true }));
        assert_eq!(Frame::parse("wide"), None);
    }

    #[test]
    fn placing_pads() {
        let px = place(vec![9; 4], (3, 1), [1, 0, 1, 1], [0, 0, 0, 255]);
        assert_eq!(px, vec![0, 0, 0, 255, 9, 9, 9, 9, 0, 0, 0, 255]);
    }

    #[test]
    fn every_raster_format_decodes() {
        let px = vec![255, 0, 0, 255, 0, 0, 0, 0];
        for f in Format::ALL.into_iter().filter(|f| *f != Format::Svg) {
            let bytes = encode(2, 1, px.clone(), f, [1., 1., 1.]).unwrap();
            let img = image::load_from_memory(&bytes).unwrap().to_rgba8();
            assert_eq!(img.dimensions(), (2, 1), "{f:?}");
            let p = img.get_pixel(1, 0).0;
            if f.alpha() {
                assert_eq!(p[3], 0, "{f:?} keeps alpha");
            } else if f != Format::Jpeg {
                assert_eq!(p, [255, 255, 255, 255], "{f:?} on the matte");
            }
        }
    }
}
