//! Darkroom's settings — the look and the last process used — saved as
//! plain `key = value` lines in `<config>/ferrite/darkroom.conf`.

use std::path::PathBuf;

use ferrite_design::ascii::{Charset, Fit};

use crate::engine::{Algo, Space};
use crate::palettes;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Dither,
    Ascii,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub scheme: String,
    /// `dark`, `light` or `system`.
    pub appearance: String,
    pub fps: u32,
    pub mode: Mode,
    pub algo: Algo,
    /// A preset key from `palettes::PRESETS`.
    pub palette: String,
    /// 0..2: error carried, or how far the threshold map reaches.
    pub strength: f32,
    pub serpentine: bool,
    /// Where colours are matched.
    pub space: Space,
    /// Dither width in pixels.
    pub cols: u32,
    /// Each pixel's size in the exported PNG.
    pub scale: u32,
    pub brightness: f32,
    pub contrast: f32,
    pub gamma: f32,
    pub invert: bool,
    /// Ink in the accent rather than the text colour.
    pub accent_ink: bool,
    pub charset: Charset,
    pub fit: Fit,
    /// ASCII width in characters.
    pub ascii_cols: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            scheme: "ferrite".into(),
            appearance: "dark".into(),
            fps: 240,
            mode: Mode::Dither,
            algo: Algo::Atkinson,
            palette: palettes::SCHEME.into(),
            strength: 1.,
            serpentine: true,
            space: Space::Oklab,
            cols: 160,
            scale: 4,
            brightness: 0.,
            contrast: 1.,
            gamma: 1.,
            invert: false,
            accent_ink: true,
            charset: Charset::Full,
            fit: Fit::Shape,
            ascii_cols: 80,
        }
    }
}

fn charset_key(c: Charset) -> String {
    c.name().replace(' ', "-")
}

impl Settings {
    pub fn parse(src: &str) -> Self {
        let mut s = Self::default();
        let flag = |v: &str, default: bool| match v {
            "true" | "on" | "yes" => true,
            "false" | "off" | "no" => false,
            _ => default,
        };
        for line in src.lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "scheme" if !v.is_empty() => s.scheme = v.into(),
                "appearance" if matches!(v, "dark" | "light" | "system") => s.appearance = v.into(),
                "fps" => s.fps = v.parse().map(|f: u32| f.clamp(12, 240)).unwrap_or(s.fps),
                "mode" => s.mode = if v == "ascii" { Mode::Ascii } else { Mode::Dither },
                // `pattern` is 0.1's name; its three values are algorithm keys.
                "algorithm" | "pattern" => s.algo = Algo::from_key(v).unwrap_or(s.algo),
                "palette" if palettes::by_key(v).is_some() => s.palette = v.into(),
                "strength" => s.strength = v.parse().map(|c: f32| c.clamp(0., 2.)).unwrap_or(s.strength),
                "serpentine" => s.serpentine = flag(v, s.serpentine),
                "match" => s.space = if v == "rgb" { Space::Rgb } else { Space::Oklab },
                "cols" => s.cols = v.parse().map(|c: u32| c.clamp(16, 640)).unwrap_or(s.cols),
                "scale" => s.scale = v.parse().map(|c: u32| c.clamp(1, 16)).unwrap_or(s.scale),
                "brightness" => s.brightness = v.parse().map(|c: f32| c.clamp(-1., 1.)).unwrap_or(s.brightness),
                "contrast" => s.contrast = v.parse().map(|c: f32| c.clamp(0.25, 3.)).unwrap_or(s.contrast),
                "gamma" => s.gamma = v.parse().map(|c: f32| c.clamp(0.2, 5.)).unwrap_or(s.gamma),
                "invert" => s.invert = flag(v, s.invert),
                "ink" => s.accent_ink = v != "text",
                "charset" => s.charset = Charset::ALL.into_iter().find(|c| charset_key(*c) == v).unwrap_or(s.charset),
                "fit" => s.fit = if v == "tone" { Fit::Tone } else { Fit::Shape },
                "ascii_cols" => s.ascii_cols = v.parse().map(|c: u32| c.clamp(16, 240)).unwrap_or(s.ascii_cols),
                _ => {}
            }
        }
        s
    }

    pub fn serialize(&self) -> String {
        format!(
            "# Darkroom settings.\nscheme = {}\nappearance = {}\nfps = {}\nmode = {}\nalgorithm = {}\npalette = {}\nstrength = {:.2}\n\
             serpentine = {}\nmatch = {}\ncols = {}\nscale = {}\nbrightness = {:.2}\ncontrast = {:.2}\ngamma = {:.2}\ninvert = {}\nink = {}\n\
             charset = {}\nfit = {}\nascii_cols = {}\n",
            self.scheme,
            self.appearance,
            self.fps,
            if self.mode == Mode::Ascii { "ascii" } else { "dither" },
            self.algo.key(),
            self.palette,
            self.strength,
            self.serpentine,
            if self.space == Space::Rgb { "rgb" } else { "oklab" },
            self.cols,
            self.scale,
            self.brightness,
            self.contrast,
            self.gamma,
            self.invert,
            if self.accent_ink { "accent" } else { "text" },
            charset_key(self.charset),
            if self.fit == Fit::Tone { "tone" } else { "shape" },
            self.ascii_cols,
        )
    }

    pub fn path() -> Option<PathBuf> {
        let base = if cfg!(windows) {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else if cfg!(target_os = "macos") {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        };
        base.map(|b| b.join("ferrite").join("darkroom.conf"))
    }

    pub fn load() -> Self {
        Self::path().and_then(|p| std::fs::read_to_string(p).ok()).map(|s| Self::parse(&s)).unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or("no config directory")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, self.serialize()).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let s = Settings {
            scheme: "cyanotype".into(),
            appearance: "light".into(),
            fps: 25,
            mode: Mode::Ascii,
            algo: Algo::Stucki,
            palette: "gameboy".into(),
            strength: 0.75,
            serpentine: false,
            space: Space::Rgb,
            cols: 320,
            scale: 2,
            brightness: -0.25,
            contrast: 1.5,
            gamma: 1.8,
            invert: true,
            accent_ink: false,
            charset: Charset::Box,
            fit: Fit::Tone,
            ascii_cols: 120,
        };
        assert_eq!(Settings::parse(&s.serialize()), s);
    }

    #[test]
    fn every_charset_round_trips() {
        for c in Charset::ALL {
            let s = Settings { charset: c, ..Settings::default() };
            assert_eq!(Settings::parse(&s.serialize()).charset, c);
        }
    }

    #[test]
    fn every_algorithm_and_palette_round_trips() {
        for algo in Algo::ALL {
            let s = Settings { algo, ..Settings::default() };
            assert_eq!(Settings::parse(&s.serialize()).algo, algo);
        }
        for p in palettes::PRESETS {
            let s = Settings { palette: p.key.into(), ..Settings::default() };
            assert_eq!(Settings::parse(&s.serialize()).palette, p.key);
        }
    }

    #[test]
    fn reads_0_1_settings() {
        assert_eq!(Settings::parse("pattern = blue-noise\n").algo, Algo::BlueNoise);
        assert_eq!(Settings::parse("pattern = bayer\n").algo, Algo::Bayer4);
        assert_eq!(Settings::parse("pattern = atkinson\n").algo, Algo::Atkinson);
    }

    #[test]
    fn junk_is_clamped_or_ignored() {
        let s = Settings::parse("cols = 99999\nscale = 0\nalgorithm = plaid\ncontrast = -4\npalette = nope\nstrength = 9\n");
        assert_eq!((s.cols, s.scale, s.algo, s.contrast, s.palette.as_str(), s.strength), (640, 1, Algo::Atkinson, 0.25, "scheme", 2.));
    }
}
