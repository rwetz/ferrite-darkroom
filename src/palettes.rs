//! Palettes a print can be developed in: the scheme's own paper and ink,
//! and the presets (classic machines and a few looks).

use crate::engine::Rgb;

/// A preset: key (settings file), name (UI), colours as `0xRRGGBB`.
/// `scheme` has no fixed colours: it's the current scheme's paper and ink.
pub struct Preset {
    pub key: &'static str,
    pub name: &'static str,
    colors: &'static [u32],
}

pub const SCHEME: &str = "scheme";

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

#[cfg(test)]
mod tests {
    use super::*;

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
