//! Palettes a print can be developed in: the scheme's own paper and ink,
//! the presets (classic machines and a few looks), and custom palettes
//! imported from a file (Lospec's formats, GIMP, JASC, a palette image) or
//! pasted as hex.

use crate::engine::Rgb;
use crate::recipe::parse_hex;

/// A preset: key (settings file), name (UI), colours as `0xRRGGBB`.
/// `scheme` has no fixed colours: it's the current scheme's paper and ink.
pub struct Preset {
    pub key: &'static str,
    pub name: &'static str,
    colors: &'static [u32],
}

pub const SCHEME: &str = "scheme";

/// Not a preset: the recipe's own imported colours.
pub const CUSTOM: &str = "custom";

/// The most colours a palette may have (indices are a byte).
pub const MAX: usize = 256;

pub const PRESETS: &[Preset] = &[
    Preset { key: SCHEME, name: "Scheme", colors: &[] },
    Preset { key: "bw", name: "Black & white", colors: &[0x000000, 0xffffff] },
    Preset { key: "rgb", name: "RGB", colors: &[0x000000, 0xff0000, 0x00ff00, 0x0000ff, 0xffffff] },
    Preset { key: "cmyk", name: "CMYK", colors: &[0xffffff, 0x00ffff, 0xff00ff, 0xffff00, 0x000000] },
    Preset { key: "3-bit", name: "3-bit", colors: &[0x000000, 0xff0000, 0x00ff00, 0x0000ff, 0xffff00, 0xff00ff, 0x00ffff, 0xffffff] },
    Preset { key: "gameboy", name: "Game Boy", colors: &[0x0f380f, 0x306230, 0x8bac0f, 0x9bbc0f] },
    // Teletext's eight, in its own order and slightly softened for a CRT.
    Preset { key: "teletext", name: "Teletext", colors: &[0x000000, 0xf00000, 0x00f000, 0xf0f000, 0x0000f0, 0xf000f0, 0x00f0f0, 0xf0f0f0] },
    // Apple II high-resolution colours.
    Preset { key: "apple-ii", name: "Apple II", colors: &[0x000000, 0xffffff, 0x14f53c, 0xff44fd, 0xff6a3c, 0x14cfff] },
    // Commodore 64, Pepto's measured palette.
    Preset {
        key: "c64",
        name: "Commodore 64",
        colors: &[
            0x000000, 0xffffff, 0x68372b, 0x70a4b2, 0x6f3d86, 0x588d43, 0x352879, 0xb8c76f, 0x6f4f25, 0x433900, 0x9a6759, 0x444444, 0x6c6c6c, 0x9ad284,
            0x6c5eb5, 0x959595,
        ],
    },
    Preset {
        key: "zx-spectrum",
        name: "ZX Spectrum",
        colors: &[
            0x000000, 0x0000d7, 0xd70000, 0xd700d7, 0x00d700, 0x00d7d7, 0xd7d700, 0xd7d7d7, 0x0000ff, 0xff0000, 0xff00ff, 0x00ff00, 0x00ffff, 0xffff00,
            0xffffff,
        ],
    },
    // 6-bit RGB is generated: four levels per channel.
    Preset { key: "6-bit", name: "6-bit RGB", colors: &[] },
    Preset { key: "2-bit-grey", name: "2-bit grey", colors: &[0x000000, 0x555555, 0xaaaaaa, 0xffffff] },
    Preset { key: "vaporwave", name: "Vaporwave", colors: &[0x241734, 0xff71ce, 0x01cdfe, 0x05ffa1, 0xb967ff, 0xfffb96] },
    Preset { key: "hacker", name: "Hacker", colors: &[0x000000, 0x003b00, 0x008f11, 0x00ff41] },
];

pub fn by_key(key: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.key == key)
}

fn rgb(hex: u32) -> Rgb {
    [(hex >> 16) & 0xff, (hex >> 8) & 0xff, hex & 0xff].map(|c| c as f32 / 255.)
}

impl Preset {
    /// The colours; `scheme` gives `[paper, ink]`.
    pub fn colors(&self, paper: Rgb, ink: Rgb) -> Vec<Rgb> {
        match self.key {
            SCHEME => vec![paper, ink],
            "6-bit" => (0..64u32).map(|i| [i >> 4, (i >> 2) & 3, i & 3].map(|c| c as f32 / 3.)).collect(),
            _ => self.colors.iter().map(|&c| rgb(c)).collect(),
        }
    }
}

/// Colours from a palette file's text. Reads GIMP (`.gpl`), JASC-PAL
/// (`.pal`, as Lospec exports it), and anything with hex colours in it:
/// Lospec's `.hex`, paint.net's `.txt` (`AARRGGBB`), CSS, or a pasted list.
pub fn parse(text: &str) -> Result<Vec<Rgb>, String> {
    let first = text.lines().next().unwrap_or("").trim();
    let triple = |line: &str| -> Option<Rgb> {
        let mut nums = line.split_whitespace().map(|n| n.parse::<u8>());
        match (nums.next(), nums.next(), nums.next()) {
            (Some(Ok(r)), Some(Ok(g)), Some(Ok(b))) => Some([r, g, b].map(|c| c as f32 / 255.)),
            _ => None,
        }
    };
    let colors: Vec<Rgb> = if first.starts_with("GIMP Palette") {
        text.lines().skip(1).filter(|l| !l.trim_start().starts_with('#')).filter_map(triple).collect()
    } else if first.starts_with("JASC-PAL") {
        text.lines().skip(3).filter_map(triple).collect()
    } else {
        text.split(|c: char| !(c.is_ascii_hexdigit() || c == '#'))
            .filter_map(|tok| {
                let t = tok.trim_start_matches('#');
                match t.len() {
                    6 => parse_hex(t),
                    // paint.net: alpha first.
                    8 if !tok.starts_with('#') => parse_hex(&t[2..]),
                    _ => None,
                }
            })
            .collect()
    };
    finish(colors)
}

/// The distinct colours of a palette image (Lospec's PNG swatches), in
/// reading order.
pub fn from_pixels(pixels: impl IntoIterator<Item = [u8; 3]>) -> Result<Vec<Rgb>, String> {
    let mut seen: Vec<[u8; 3]> = Vec::new();
    for p in pixels {
        if !seen.contains(&p) {
            if seen.len() == MAX {
                return Err(format!("that image has more than {MAX} colours; a palette image has one swatch per colour"));
            }
            seen.push(p);
        }
    }
    finish(seen.into_iter().map(|p| p.map(|c| c as f32 / 255.)).collect())
}

/// A palette from a file: an image's distinct colours, or a palette file.
pub fn load(path: &std::path::Path) -> Result<Vec<Rgb>, String> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if matches!(ext.as_str(), "png" | "gif" | "bmp" | "webp" | "jpg" | "jpeg") {
        let img = image::open(path).map_err(|e| format!("couldn't read the image: {e}"))?.to_rgb8();
        if img.width() as u64 * img.height() as u64 > 1 << 22 {
            return Err("that image is too big to be a palette".into());
        }
        return from_pixels(img.pixels().map(|p| p.0));
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse(&text)
}

fn finish(colors: Vec<Rgb>) -> Result<Vec<Rgb>, String> {
    let mut out: Vec<Rgb> = Vec::new();
    for c in colors {
        if !out.contains(&c) {
            out.push(c);
        }
    }
    match out.len() {
        0 => Err("no colours found (expected hex like #0f380f, a GIMP .gpl or a JASC .pal)".into()),
        1 => Err("a palette needs at least two colours".into()),
        n if n > MAX => Err(format!("{n} colours; a palette can have at most {MAX}")),
        _ => Ok(out),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: [Rgb; 4] = [
        [15. / 255., 56. / 255., 15. / 255.],
        [48. / 255., 98. / 255., 48. / 255.],
        [139. / 255., 172. / 255., 15. / 255.],
        [155. / 255., 188. / 255., 15. / 255.],
    ];

    #[test]
    fn reads_lospec_hex() {
        assert_eq!(parse("0f380f\n306230\n8bac0f\n9bbc0f\n").unwrap(), GB);
        assert_eq!(parse("#0f380f, #306230, #8bac0f #9bbc0f").unwrap(), GB);
    }

    #[test]
    fn reads_gimp_and_jasc() {
        let gpl = "GIMP Palette\nName: gb\nColumns: 4\n#\n 15  56  15\tdarkest\n 48  98  48\n139 172  15\n155 188  15\n";
        assert_eq!(parse(gpl).unwrap(), GB);
        let pal = "JASC-PAL\n0100\n4\n15 56 15\n48 98 48\n139 172 15\n155 188 15\n";
        assert_eq!(parse(pal).unwrap(), GB);
    }

    #[test]
    fn reads_paint_net() {
        let txt = ";paint.net Palette File\n;Colors: 4\nFF0F380F\nFF306230\nFF8BAC0F\nFF9BBC0F\n";
        assert_eq!(parse(txt).unwrap(), GB);
    }

    #[test]
    fn rejects_what_isnt_a_palette() {
        assert!(parse("hello there").is_err());
        assert!(parse("#ffffff #ffffff").is_err(), "one distinct colour");
        assert!(from_pixels([[0, 0, 0]; 9]).is_err());
        assert_eq!(from_pixels([[0, 0, 0], [255, 255, 255], [0, 0, 0]]).unwrap().len(), 2);
        assert!(from_pixels((0..300u32).map(|i| [(i % 256) as u8, (i / 256) as u8, 0])).is_err());
    }

    #[test]
    fn every_preset_is_usable() {
        let mut keys = Vec::new();
        for p in PRESETS {
            let colors = p.colors([0.; 3], [1.; 3]);
            assert!((2..=256).contains(&colors.len()), "{}", p.key);
            assert!(colors.iter().flatten().all(|c| (0. ..=1.).contains(c)));
            assert_eq!(by_key(p.key).map(|q| q.name), Some(p.name));
            keys.push(p.key);
        }
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), PRESETS.len());
    }

    #[test]
    fn six_bit_is_the_full_cube() {
        let c = by_key("6-bit").unwrap().colors([0.; 3], [1.; 3]);
        assert_eq!(c.len(), 64);
        assert!(c.contains(&[1., 1., 1.]) && c.contains(&[0., 0., 0.]) && c.contains(&[1., 0., 1. / 3.]));
    }
}
