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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Size {
    /// The recipe's pixels per art pixel.
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
