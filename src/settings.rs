//! Darkroom's settings — the look and the last process used — saved as
//! plain `key = value` lines in `<config>/ferrite/darkroom.conf`.

use std::path::PathBuf;

use ferrite_design::ascii::{Charset, Fit};
use ferrite_design::dither::Pattern;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Dither,
    Ascii,
}

pub const PATTERNS: [(Pattern, &str, &str); 3] =
    [(Pattern::Bayer4, "bayer", "Bayer"), (Pattern::BlueNoise, "blue-noise", "Blue noise"), (Pattern::Atkinson, "atkinson", "Atkinson")];

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub scheme: String,
    /// `dark`, `light` or `system`.
    pub appearance: String,
    pub fps: u32,
    pub mode: Mode,
    pub pattern: Pattern,
    /// Dither width in pixels.
    pub cols: u32,
    /// Each pixel's size in the exported PNG.
    pub scale: u32,
    pub brightness: f32,
    pub contrast: f32,
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
            pattern: Pattern::Atkinson,
            cols: 160,
            scale: 4,
            brightness: 0.,
            contrast: 1.,
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
                "pattern" => s.pattern = PATTERNS.iter().find(|p| p.1 == v).map(|p| p.0).unwrap_or(s.pattern),
                "cols" => s.cols = v.parse().map(|c: u32| c.clamp(16, 640)).unwrap_or(s.cols),
                "scale" => s.scale = v.parse().map(|c: u32| c.clamp(1, 16)).unwrap_or(s.scale),
                "brightness" => s.brightness = v.parse().map(|c: f32| c.clamp(-1., 1.)).unwrap_or(s.brightness),
                "contrast" => s.contrast = v.parse().map(|c: f32| c.clamp(0.25, 3.)).unwrap_or(s.contrast),
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
            "# Darkroom settings.\nscheme = {}\nappearance = {}\nfps = {}\nmode = {}\npattern = {}\ncols = {}\nscale = {}\n\
             brightness = {:.2}\ncontrast = {:.2}\ninvert = {}\nink = {}\ncharset = {}\nfit = {}\nascii_cols = {}\n",
            self.scheme,
            self.appearance,
            self.fps,
            if self.mode == Mode::Ascii { "ascii" } else { "dither" },
            PATTERNS.iter().find(|p| p.0 == self.pattern).map(|p| p.1).unwrap_or("bayer"),
            self.cols,
            self.scale,
            self.brightness,
            self.contrast,
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
            pattern: Pattern::BlueNoise,
            cols: 320,
            scale: 2,
            brightness: -0.25,
            contrast: 1.5,
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
    fn junk_is_clamped_or_ignored() {
        let s = Settings::parse("cols = 99999\nscale = 0\npattern = plaid\ncontrast = -4\n");
        assert_eq!((s.cols, s.scale, s.pattern, s.contrast), (640, 1, Pattern::Atkinson, 0.25));
    }
}
