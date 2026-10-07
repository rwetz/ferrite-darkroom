//! A recipe: everything about how a print is developed, and nothing about
//! the window. It's what the settings file remembers between runs and what
//! a `.toml` recipe file shares: save one, open it later or on another
//! machine, and the same photo develops the same way.
//!
//! Both files are flat `key = value` lines, read by the same code. The
//! settings file writes values bare; a recipe file writes valid TOML
//! (quoted strings, arrays). The reader takes either, and ignores `#`
//! comments, unknown keys and values out of range.

use ferrite_design::ascii::{Charset, Fit};

use crate::engine::{Algo, Params, Rgb, Space, Tile};
use crate::mask::{self, Kind};
use crate::palettes;
use crate::render::{Cells, Look, Paper, Shape};
use crate::textart::{Glyphs, Tint};

/// What the background layer (where the mask isn't) becomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Background {
    /// Developed like the subject, drawn with the background's cells.
    Same,
    /// Bare paper: a cut-out (with the lattice, if set).
    Paper,
    /// Its own algorithm, strength, threshold and cells.
    Own,
}

impl Background {
    pub const ALL: [Background; 3] = [Background::Same, Background::Paper, Background::Own];

    pub fn key(self) -> &'static str {
        match self {
            Background::Same => "same",
            Background::Paper => "paper",
            Background::Own => "own",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Background::Same => "Same",
            Background::Paper => "Paper",
            Background::Own => "Its own",
        }
    }

    pub fn from_key(key: &str) -> Option<Background> {
        Background::ALL.into_iter().find(|b| b.key() == key)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Dither,
    Ascii,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Recipe {
    pub mode: Mode,
    pub algo: Algo,
    /// A preset key from `palettes::PRESETS`, or `palettes::CUSTOM`.
    pub palette: String,
    /// The custom palette's colours (imported or pasted).
    pub colors: Vec<Rgb>,
    /// 0..2: error carried, or how far the threshold map reaches.
    pub strength: f32,
    /// -1..1: where ink starts.
    pub bias: f32,
    pub serpentine: bool,
    /// Where colours are matched.
    pub space: Space,
    /// For the random and stippling algorithms.
    pub seed: u32,
    /// For the custom-tile algorithm.
    pub tile: Tile,
    /// Animations: 0..1, how firmly unchanged pixels hold between frames.
    pub stability: f32,
    /// Animations: playback and export speed, 0.25..4.
    pub speed: f32,
    /// Dither width in pixels.
    pub cols: u32,
    /// Each pixel's size in the exported PNG.
    pub scale: u32,
    /// How each pixel is drawn.
    pub shape: Shape,
    pub gutter: f32,
    pub modulate: bool,
    pub paper: Paper,
    pub transparent: bool,
    pub lattice: f32,
    /// Which pixels are the subject.
    pub mask: mask::Spec,
    pub background: Background,
    /// The background layer's own treatment (for `Background::Own`; its
    /// lattice also dots a `Paper` background).
    pub bg_algo: Algo,
    pub bg_strength: f32,
    pub bg_bias: f32,
    pub bg_cells: Cells,
    pub brightness: f32,
    pub contrast: f32,
    pub gamma: f32,
    pub invert: bool,
    /// The Scheme palette inks in the accent rather than the text colour.
    pub accent_ink: bool,
    pub charset: Charset,
    pub fit: Fit,
    /// ASCII width in characters.
    pub ascii_cols: u32,
    /// ASCII: characters, braille or blocks.
    pub glyphs: Glyphs,
    /// ASCII: how cells are coloured.
    pub ascii_tint: Tint,
}

impl Default for Recipe {
    fn default() -> Self {
        Recipe {
            mode: Mode::Dither,
            algo: Algo::Atkinson,
            palette: palettes::SCHEME.into(),
            colors: Vec::new(),
            strength: 1.,
            bias: 0.,
            serpentine: true,
            space: Space::Oklab,
            seed: 1,
            tile: Tile::default(),
            stability: 0.5,
            speed: 1.,
            cols: 160,
            scale: 4,
            shape: Shape::Square,
            gutter: 0.,
            modulate: false,
            paper: Paper::First,
            transparent: false,
            lattice: 0.,
            mask: mask::Spec::default(),
            background: Background::Paper,
            bg_algo: Algo::Bayer4,
            bg_strength: 1.,
            bg_bias: 0.,
            bg_cells: Cells::default(),
            brightness: 0.,
            contrast: 1.,
            gamma: 1.,
            invert: false,
            accent_ink: true,
            charset: Charset::Full,
            fit: Fit::Shape,
            ascii_cols: 80,
            glyphs: Glyphs::Characters,
            ascii_tint: Tint::Ink,
        }
    }
}

/// A value as written: bare (numbers, flags), text, or a list.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bare(String),
    Text(String),
    List(Vec<String>),
}

impl Value {
    fn as_str(&self) -> &str {
        match self {
            Value::Bare(s) | Value::Text(s) => s,
            Value::List(_) => "",
        }
    }

    /// Settings-file form: everything bare, lists space-separated.
    fn flat(&self) -> String {
        match self {
            Value::Bare(s) | Value::Text(s) => s.clone(),
            Value::List(items) => items.join(" "),
        }
    }

    /// TOML form.
    fn toml(&self) -> String {
        let quote = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
        match self {
            Value::Bare(s) => s.clone(),
            Value::Text(s) => quote(s),
            Value::List(items) => format!("[{}]", items.iter().map(|s| quote(s)).collect::<Vec<_>>().join(", ")),
        }
    }
}

/// Cut a trailing `# comment`: a `#` after whitespace, outside quotes.
fn strip_comment(v: &str) -> &str {
    let mut quoted = false;
    let mut prev_space = true;
    for (i, ch) in v.char_indices() {
        match ch {
            '"' => quoted = !quoted,
            '#' if !quoted && prev_space => return v[..i].trim_end(),
            _ => {}
        }
        prev_space = ch.is_whitespace();
    }
    v
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    match s.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        Some(inner) => inner.replace("\\\"", "\"").replace("\\\\", "\\"),
        None => s.to_string(),
    }
}

/// Read one line into `(key, value)`, or `None` for blanks and comments.
pub fn parse_line(line: &str) -> Option<(String, Value)> {
    let line = line.trim();
    if line.starts_with('#') || (line.starts_with('[') && !line.contains('=')) {
        return None;
    }
    let (k, v) = line.split_once('=')?;
    let v = strip_comment(v.trim());
    let value = if let Some(inner) = v.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
        Value::List(inner.split(',').map(unquote).filter(|s| !s.is_empty()).collect())
    } else if v.starts_with('"') {
        Value::Text(unquote(v))
    } else {
        Value::Bare(v.to_string())
    };
    Some((k.trim().to_string(), value))
}

fn charset_key(c: Charset) -> String {
    c.name().replace(' ', "-")
}

fn flag(v: &str, default: bool) -> bool {
    match v {
        "true" | "on" | "yes" => true,
        "false" | "off" | "no" => false,
        _ => default,
    }
}

/// `#rrggbb`, `rrggbb` or `0xrrggbb` → a colour.
pub fn parse_hex(s: &str) -> Option<Rgb> {
    let s = s.trim().trim_start_matches('#').trim_start_matches("0x");
    if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some([(v >> 16) & 0xff, (v >> 8) & 0xff, v & 0xff].map(|c| c as f32 / 255.))
}

pub fn hex(c: Rgb) -> String {
    let [r, g, b] = c.map(|v| (v.clamp(0., 1.) * 255.).round() as u8);
    format!("#{r:02x}{g:02x}{b:02x}")
}

impl Recipe {
    /// Apply one `key = value`. Returns whether the key is a recipe key.
    pub fn apply(&mut self, key: &str, value: &Value) -> bool {
        let v = value.as_str();
        let f = |lo: f32, hi: f32, cur: f32| v.parse().map(|x: f32| x.clamp(lo, hi)).unwrap_or(cur);
        let u = |lo: u32, hi: u32, cur: u32| v.parse().map(|x: u32| x.clamp(lo, hi)).unwrap_or(cur);
        match key {
            "mode" => self.mode = if v == "ascii" { Mode::Ascii } else { Mode::Dither },
            // `pattern` is 0.1's name; its three values are algorithm keys.
            "algorithm" | "pattern" => self.algo = Algo::from_key(v).unwrap_or(self.algo),
            "palette" => {
                if palettes::by_key(v).is_some() || v == palettes::CUSTOM {
                    self.palette = v.into();
                }
            }
            "colors" => {
                let items: Vec<String> = match value {
                    Value::List(items) => items.clone(),
                    other => other.as_str().split_whitespace().map(str::to_string).collect(),
                };
                let colors: Vec<Rgb> = items.iter().filter_map(|s| parse_hex(s)).collect();
                if (2..=256).contains(&colors.len()) {
                    self.colors = colors;
                }
            }
            "strength" => self.strength = f(0., 2., self.strength),
            "bias" => self.bias = f(-1., 1., self.bias),
            "serpentine" => self.serpentine = flag(v, self.serpentine),
            "match" => {
                self.space = match v {
                    "rgb" => Space::Rgb,
                    "linear" => Space::Linear,
                    _ => Space::Oklab,
                }
            }
            "seed" => self.seed = v.parse().unwrap_or(self.seed),
            "tile" => self.tile = Tile::parse(v).unwrap_or(self.tile),
            "stability" => self.stability = f(0., 1., self.stability),
            "speed" => self.speed = f(0.25, 4., self.speed),
            "cols" => self.cols = u(16, 640, self.cols),
            "scale" => self.scale = u(1, 16, self.scale),
            "shape" => self.shape = Shape::from_key(v).unwrap_or(self.shape),
            "gutter" => self.gutter = f(0., 0.9, self.gutter),
            "modulate" => self.modulate = flag(v, self.modulate),
            "paper" => self.paper = Paper::from_key(v).unwrap_or(self.paper),
            "transparent" => self.transparent = flag(v, self.transparent),
            "lattice" => self.lattice = f(0., 1., self.lattice),
            "mask" => self.mask.kind = Kind::from_key(v).unwrap_or(self.mask.kind),
            "mask_low" => self.mask.low = f(0., 1., self.mask.low),
            "mask_high" => self.mask.high = f(0., 1., self.mask.high),
            "mask_colour" => self.mask.color = parse_hex(v).unwrap_or(self.mask.color),
            "mask_tolerance" => self.mask.tolerance = f(0., 1., self.mask.tolerance),
            "mask_feather" => self.mask.feather = f(0., 1., self.mask.feather),
            "mask_invert" => self.mask.invert = flag(v, self.mask.invert),
            "background" => self.background = Background::from_key(v).unwrap_or(self.background),
            "bg_algorithm" => self.bg_algo = Algo::from_key(v).unwrap_or(self.bg_algo),
            "bg_strength" => self.bg_strength = f(0., 2., self.bg_strength),
            "bg_bias" => self.bg_bias = f(-1., 1., self.bg_bias),
            "bg_shape" => self.bg_cells.shape = Shape::from_key(v).unwrap_or(self.bg_cells.shape),
            "bg_gutter" => self.bg_cells.gutter = f(0., 0.9, self.bg_cells.gutter),
            "bg_modulate" => self.bg_cells.modulate = flag(v, self.bg_cells.modulate),
            "bg_lattice" => self.bg_cells.lattice = f(0., 1., self.bg_cells.lattice),
            "brightness" => self.brightness = f(-1., 1., self.brightness),
            "contrast" => self.contrast = f(0.25, 3., self.contrast),
            "gamma" => self.gamma = f(0.2, 5., self.gamma),
            "invert" => self.invert = flag(v, self.invert),
            "ink" => self.accent_ink = v != "text",
            "charset" => self.charset = Charset::ALL.into_iter().find(|c| charset_key(*c) == v).unwrap_or(self.charset),
            "fit" => self.fit = if v == "tone" { Fit::Tone } else { Fit::Shape },
            "ascii_cols" => self.ascii_cols = u(16, 240, self.ascii_cols),
            "glyphs" => self.glyphs = Glyphs::from_key(v).unwrap_or(self.glyphs),
            "ascii_colour" => self.ascii_tint = Tint::from_key(v).unwrap_or(self.ascii_tint),
            _ => return false,
        }
        true
    }

    /// Every key, in file order.
    pub fn pairs(&self) -> Vec<(&'static str, Value)> {
        let bare = |s: String| Value::Bare(s);
        let text = |s: &str| Value::Text(s.to_string());
        let mut out = vec![
            ("mode", text(if self.mode == Mode::Ascii { "ascii" } else { "dither" })),
            ("algorithm", text(self.algo.key())),
            ("palette", text(&self.palette)),
        ];
        if !self.colors.is_empty() {
            out.push(("colors", Value::List(self.colors.iter().map(|&c| hex(c)).collect())));
        }
        out.extend([
            ("strength", bare(format!("{:.2}", self.strength))),
            ("bias", bare(format!("{:.2}", self.bias))),
            ("serpentine", bare(self.serpentine.to_string())),
            (
                "match",
                text(match self.space {
                    Space::Rgb => "rgb",
                    Space::Linear => "linear",
                    Space::Oklab => "oklab",
                }),
            ),
            ("seed", bare(self.seed.to_string())),
            ("tile", text(&self.tile.text())),
            ("stability", bare(format!("{:.2}", self.stability))),
            ("speed", bare(format!("{:.2}", self.speed))),
            ("cols", bare(self.cols.to_string())),
            ("scale", bare(self.scale.to_string())),
            ("shape", text(self.shape.key())),
            ("gutter", bare(format!("{:.2}", self.gutter))),
            ("modulate", bare(self.modulate.to_string())),
            ("paper", text(self.paper.key())),
            ("transparent", bare(self.transparent.to_string())),
            ("lattice", bare(format!("{:.2}", self.lattice))),
            ("mask", text(self.mask.kind.key())),
            ("mask_low", bare(format!("{:.2}", self.mask.low))),
            ("mask_high", bare(format!("{:.2}", self.mask.high))),
            ("mask_colour", text(&hex(self.mask.color))),
            ("mask_tolerance", bare(format!("{:.2}", self.mask.tolerance))),
            ("mask_feather", bare(format!("{:.2}", self.mask.feather))),
            ("mask_invert", bare(self.mask.invert.to_string())),
            ("background", text(self.background.key())),
            ("bg_algorithm", text(self.bg_algo.key())),
            ("bg_strength", bare(format!("{:.2}", self.bg_strength))),
            ("bg_bias", bare(format!("{:.2}", self.bg_bias))),
            ("bg_shape", text(self.bg_cells.shape.key())),
            ("bg_gutter", bare(format!("{:.2}", self.bg_cells.gutter))),
            ("bg_modulate", bare(self.bg_cells.modulate.to_string())),
            ("bg_lattice", bare(format!("{:.2}", self.bg_cells.lattice))),
            ("brightness", bare(format!("{:.2}", self.brightness))),
            ("contrast", bare(format!("{:.2}", self.contrast))),
            ("gamma", bare(format!("{:.2}", self.gamma))),
            ("invert", bare(self.invert.to_string())),
            ("ink", text(if self.accent_ink { "accent" } else { "text" })),
            ("charset", text(&charset_key(self.charset))),
            ("fit", text(if self.fit == Fit::Tone { "tone" } else { "shape" })),
            ("ascii_cols", bare(self.ascii_cols.to_string())),
            ("glyphs", text(self.glyphs.key())),
            ("ascii_colour", text(self.ascii_tint.key())),
        ]);
        out
    }

    /// The settings-file lines (bare values; `#` dropped from colours so
    /// they can't be mistaken for comments).
    pub fn flat_lines(&self) -> String {
        self.pairs()
            .into_iter()
            .map(|(k, v)| {
                let v = if k == "colors" || k == "mask_colour" { v.flat().replace('#', "") } else { v.flat() };
                format!("{k} = {v}\n")
            })
            .collect()
    }

    /// A recipe file: valid TOML.
    pub fn to_toml(&self) -> String {
        let mut s = String::from("# A Darkroom recipe. Open it in Darkroom (Ctrl+Shift+O, or drop it on the print).\n");
        for (k, v) in self.pairs() {
            s.push_str(&format!("{k} = {}\n", v.toml()));
        }
        s
    }

    /// Read a recipe file. Starts from the defaults, so a partial recipe
    /// is a complete one.
    pub fn from_toml(src: &str) -> Result<Recipe, String> {
        let mut r = Recipe::default();
        let mut known = 0;
        for (k, v) in src.trim_start_matches('\u{feff}').lines().filter_map(parse_line) {
            if r.apply(&k, &v) {
                known += 1;
            }
        }
        if known == 0 {
            return Err("there's no Darkroom recipe in that file".into());
        }
        Ok(r)
    }

    pub fn look(&self) -> Look {
        let cells = Cells { shape: self.shape, gutter: self.gutter, modulate: self.modulate, lattice: self.lattice };
        let bg = match self.background {
            Background::Same => cells,
            Background::Paper | Background::Own => self.bg_cells,
        };
        Look { cells, bg, paper: self.paper, transparent: self.transparent }
    }

    /// Whether a mask splits the print into layers.
    pub fn masked(&self) -> bool {
        self.mode == Mode::Dither && self.mask.kind != Kind::None
    }

    /// The background layer's dither settings.
    pub fn bg_params(&self) -> Params {
        Params { algo: self.bg_algo, strength: self.bg_strength, bias: self.bg_bias, ..self.params() }
    }

    /// Whether the custom palette is chosen and has its colours.
    pub fn uses_custom(&self) -> bool {
        self.palette == palettes::CUSTOM && self.colors.len() >= 2
    }

    /// The colours the art develops in. `paper` and `ink` are the scheme's,
    /// for the Scheme palette.
    pub fn palette_colors(&self, paper: Rgb, ink: Rgb) -> Vec<Rgb> {
        if self.uses_custom() {
            return self.colors.clone();
        }
        palettes::by_key(&self.palette).unwrap_or(&palettes::PRESETS[0]).colors(paper, ink)
    }

    pub fn params(&self) -> Params {
        Params { algo: self.algo, strength: self.strength, bias: self.bias, serpentine: self.serpentine, space: self.space, seed: self.seed, tile: self.tile }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fancy() -> Recipe {
        Recipe {
            mode: Mode::Ascii,
            algo: Algo::Stucki,
            palette: palettes::CUSTOM.into(),
            colors: vec![[0., 0., 0.], [1., 0.4, 0.2], [1., 1., 1.]],
            strength: 0.75,
            bias: -0.3,
            serpentine: false,
            space: Space::Rgb,
            seed: 42,
            tile: Tile::parse("3 1 / 0 2").unwrap(),
            stability: 0.8,
            speed: 1.5,
            cols: 320,
            scale: 2,
            shape: Shape::Diamond,
            gutter: 0.35,
            modulate: true,
            paper: Paper::Lightest,
            transparent: true,
            lattice: 0.25,
            mask: mask::Spec { kind: Kind::Color, low: 0.1, high: 0.7, color: [1., 0.4, 0.2], tolerance: 0.3, feather: 0.4, invert: true },
            background: Background::Own,
            bg_algo: Algo::Halftone,
            bg_strength: 0.5,
            bg_bias: 0.25,
            bg_cells: Cells { shape: Shape::Plus, gutter: 0.2, modulate: true, lattice: 0.3 },
            brightness: -0.25,
            contrast: 1.5,
            gamma: 1.8,
            invert: true,
            accent_ink: false,
            charset: Charset::Box,
            fit: Fit::Tone,
            ascii_cols: 120,
            glyphs: Glyphs::Braille,
            ascii_tint: Tint::Palette,
        }
    }

    fn close(a: &Recipe, b: &Recipe) -> bool {
        // Colours go through 8-bit hex.
        let colors = a.colors.len() == b.colors.len() && a.colors.iter().zip(&b.colors).all(|(x, y)| x.iter().zip(y).all(|(p, q)| (p - q).abs() < 0.003));
        colors && Recipe { colors: vec![], ..a.clone() } == Recipe { colors: vec![], ..b.clone() }
    }

    #[test]
    fn toml_round_trips() {
        let r = fancy();
        let back = Recipe::from_toml(&r.to_toml()).unwrap();
        assert!(close(&r, &back), "{back:?}");
    }

    #[test]
    fn flat_round_trips() {
        let r = fancy();
        let mut back = Recipe::default();
        for (k, v) in r.flat_lines().lines().filter_map(parse_line) {
            assert!(back.apply(&k, &v), "{k}");
        }
        assert!(close(&r, &back), "{back:?}");
    }

    #[test]
    fn toml_is_toml_shaped() {
        let t = fancy().to_toml();
        assert!(t.contains("algorithm = \"stucki\""));
        assert!(t.contains("colors = [\"#000000\", \"#ff6633\", \"#ffffff\"]"));
        assert!(t.contains("strength = 0.75"));
        assert!(t.contains("serpentine = false"));
    }

    #[test]
    fn comments_and_junk() {
        let r = Recipe::from_toml("# hi\n[recipe]\nalgorithm = \"sierra\"   # a comment\ncols = 99999\nnonsense = 3\npalette = \"nope\"\n").unwrap();
        assert_eq!((r.algo, r.cols, r.palette.as_str()), (Algo::Sierra, 640, "scheme"));
        assert!(Recipe::from_toml("hello = world\n").is_err());
        assert!(Recipe::from_toml("").is_err());
    }

    #[test]
    fn every_algorithm_and_palette_round_trips() {
        for algo in Algo::ALL {
            let r = Recipe { algo, ..Recipe::default() };
            assert_eq!(Recipe::from_toml(&r.to_toml()).unwrap().algo, algo);
        }
        for p in palettes::PRESETS {
            let r = Recipe { palette: p.key.into(), ..Recipe::default() };
            assert_eq!(Recipe::from_toml(&r.to_toml()).unwrap().palette, p.key);
        }
    }

    #[test]
    fn byte_order_mark_is_ignored() {
        // Notepad and PowerShell write one; the first key must still count.
        let r = Recipe::from_toml("\u{feff}algorithm = \"bayer-8\"\n").unwrap();
        assert_eq!(r.algo, Algo::Bayer8);
    }

    #[test]
    fn bundled_recipes_load() {
        let gb = Recipe::from_toml(include_str!("../recipes/gameboy.toml")).unwrap();
        assert_eq!((gb.algo, gb.palette.as_str()), (Algo::Bayer4, "gameboy"));
        let news = Recipe::from_toml(include_str!("../recipes/newsprint.toml")).unwrap();
        assert_eq!((news.algo, news.palette.as_str(), news.cols), (Algo::Halftone, "bw", 320));
        let sunset = Recipe::from_toml(include_str!("../recipes/sunset.toml")).unwrap();
        assert_eq!((sunset.algo, sunset.palette.as_str(), sunset.colors.len(), sunset.bias), (Algo::Bayer8, palettes::CUSTOM, 7, 0.2));
        let engraving = Recipe::from_toml(include_str!("../recipes/engraving.toml")).unwrap();
        assert_eq!((engraving.algo, engraving.accent_ink, engraving.scale), (Algo::Atkinson, false, 3));
        let cutout = Recipe::from_toml(include_str!("../recipes/cutout.toml")).unwrap();
        assert_eq!((cutout.mask.kind, cutout.background, cutout.paper, cutout.colors.len()), (Kind::Border, Background::Paper, Paper::Lightest, 2));
        let lattice = Recipe::from_toml(include_str!("../recipes/lattice.toml")).unwrap();
        assert_eq!((lattice.background, lattice.bg_algo, lattice.bg_cells.gutter), (Background::Own, Algo::Bayer4, 0.35));
        let dots = Recipe::from_toml(include_str!("../recipes/dots.toml")).unwrap();
        assert_eq!((dots.shape, dots.modulate, dots.gutter, dots.scale), (Shape::Circle, true, 0.15, 8));
    }

    #[test]
    fn hex_parsing() {
        assert_eq!(parse_hex("#ff8000"), Some([1., 128. / 255., 0.]));
        assert_eq!(parse_hex("0x000000"), Some([0.; 3]));
        assert_eq!(parse_hex("fff"), None);
        assert_eq!(parse_hex("zzzzzz"), None);
        assert_eq!(hex([1., 0.5, 0.]), "#ff8000");
    }
}
