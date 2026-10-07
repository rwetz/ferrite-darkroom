//! Darkroom's settings — the look, and the recipe last used — saved as
//! plain `key = value` lines in `<config>/ferrite/darkroom.conf`.

use std::path::PathBuf;

use crate::recipe::{self, Recipe};

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub scheme: String,
    /// `dark`, `light` or `system`.
    pub appearance: String,
    pub fps: u32,
    pub recipe: Recipe,
}

impl Default for Settings {
    fn default() -> Self {
        Self { scheme: "ferrite".into(), appearance: "dark".into(), fps: 240, recipe: Recipe::default() }
    }
}

impl Settings {
    pub fn parse(src: &str) -> Self {
        let mut s = Self::default();
        for (k, value) in src.trim_start_matches('\u{feff}').lines().filter_map(recipe::parse_line) {
            if s.recipe.apply(&k, &value) {
                continue;
            }
            let (recipe::Value::Bare(v) | recipe::Value::Text(v)) = value else { continue };
            match k.as_str() {
                "scheme" if !v.is_empty() => s.scheme = v,
                "appearance" if matches!(v.as_str(), "dark" | "light" | "system") => s.appearance = v,
                "fps" => s.fps = v.parse().map(|f: u32| f.clamp(12, 240)).unwrap_or(s.fps),
                _ => {}
            }
        }
        s
    }

    pub fn serialize(&self) -> String {
        format!(
            "# Darkroom settings.\nscheme = {}\nappearance = {}\nfps = {}\n{}",
            self.scheme,
            self.appearance,
            self.fps,
            self.recipe.flat_lines()
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
    use crate::engine::Algo;
    use crate::recipe::Mode;
    use ferrite_design::ascii::Charset;

    #[test]
    fn round_trips() {
        let s = Settings {
            scheme: "cyanotype".into(),
            appearance: "light".into(),
            fps: 25,
            recipe: Recipe { mode: Mode::Ascii, algo: Algo::Stucki, palette: "gameboy".into(), cols: 320, gamma: 1.8, ..Recipe::default() },
        };
        assert_eq!(Settings::parse(&s.serialize()), s);
    }

    #[test]
    fn every_charset_round_trips() {
        for c in Charset::ALL {
            let s = Settings { recipe: Recipe { charset: c, ..Recipe::default() }, ..Settings::default() };
            assert_eq!(Settings::parse(&s.serialize()).recipe.charset, c);
        }
    }

    #[test]
    fn reads_0_1_settings() {
        let s = Settings::parse("scheme = phosphor\nmode = dither\npattern = blue-noise\ncols = 200\nink = text\n");
        assert_eq!((s.scheme.as_str(), s.recipe.algo, s.recipe.cols, s.recipe.accent_ink), ("phosphor", Algo::BlueNoise, 200, false));
        assert_eq!(Settings::parse("pattern = bayer\n").recipe.algo, Algo::Bayer4);
        assert_eq!(Settings::parse("pattern = atkinson\n").recipe.algo, Algo::Atkinson);
    }

    #[test]
    fn inline_comments_as_in_the_readme() {
        let s = Settings::parse("scheme = ferrite      # ferrite mono graphite\nappearance = light   # dark | light\n");
        assert_eq!((s.scheme.as_str(), s.appearance.as_str()), ("ferrite", "light"));
    }

    #[test]
    fn junk_is_clamped_or_ignored() {
        let s = Settings::parse("cols = 99999\nscale = 0\nalgorithm = plaid\ncontrast = -4\npalette = nope\nstrength = 9\nfps = 1\n");
        let r = &s.recipe;
        assert_eq!((r.cols, r.scale, r.algo, r.contrast, r.palette.as_str(), r.strength, s.fps), (640, 1, Algo::Atkinson, 0.25, "scheme", 2., 12));
    }
}
