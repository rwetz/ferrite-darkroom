//! Text art: a print as a grid of characters, each with an optional
//! foreground and background colour, and the ways out (plain text, ANSI,
//! HTML, SVG, and an HTML "film" for animations).
//!
//! Glyph sets:
//! - **Characters:** ferrite-design's thirteen character sets, fitted to
//!   the display face's real glyphs by shape or tone.
//! - **Braille** (2×4 dots a cell), **half blocks** (1×2) and **quadrants**
//!   (2×2): the picture is dithered at sub-cell resolution with any of the
//!   engine's algorithms and the dots packed into glyphs, so a cell carries
//!   up to eight pixels.
//! - **Colour half blocks:** `▀` with the top pixel as the foreground and
//!   the bottom as the background, dithered into the recipe's palette: two
//!   colour pixels a cell, the densest colour text art there is.
//!
//! Pure.

use crate::engine::{self, Rgb, oklab};
use crate::recipe::Recipe;
use crate::studio::{self, Adjust, Print};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Glyphs {
    Characters,
    Braille,
    HalfBlocks,
    Quadrants,
    ColorBlocks,
}

impl Glyphs {
    pub const ALL: [Glyphs; 5] = [Glyphs::Characters, Glyphs::Braille, Glyphs::HalfBlocks, Glyphs::Quadrants, Glyphs::ColorBlocks];

    pub fn key(self) -> &'static str {
        match self {
            Glyphs::Characters => "characters",
            Glyphs::Braille => "braille",
            Glyphs::HalfBlocks => "half-blocks",
            Glyphs::Quadrants => "quadrants",
            Glyphs::ColorBlocks => "colour-blocks",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Glyphs::Characters => "Characters",
            Glyphs::Braille => "Braille",
            Glyphs::HalfBlocks => "Half blocks",
            Glyphs::Quadrants => "Quadrants",
            Glyphs::ColorBlocks => "Colour half blocks",
        }
    }

    pub fn from_key(key: &str) -> Option<Glyphs> {
        Glyphs::ALL.into_iter().find(|g| g.key() == key)
    }

    /// Sub-cell pixels across and down.
    fn sub(self) -> (u32, u32) {
        match self {
            Glyphs::Characters => (1, 1),
            Glyphs::Braille => (2, 4),
            Glyphs::HalfBlocks | Glyphs::ColorBlocks => (1, 2),
            Glyphs::Quadrants => (2, 2),
        }
    }
}

/// How cells are coloured.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tint {
    /// One ink for everything (the scheme's, or the palette's lightest).
    Ink,
    /// Each cell the photo's own colour there.
    Photo,
    /// Each cell the nearest palette colour.
    Palette,
}

impl Tint {
    pub const ALL: [Tint; 3] = [Tint::Ink, Tint::Photo, Tint::Palette];

    pub fn key(self) -> &'static str {
        match self {
            Tint::Ink => "ink",
            Tint::Photo => "photo",
            Tint::Palette => "palette",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Tint::Ink => "Ink",
            Tint::Photo => "Photo",
            Tint::Palette => "Palette",
        }
    }

    pub fn from_key(key: &str) -> Option<Tint> {
        Tint::ALL.into_iter().find(|t| t.key() == key)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub ch: char,
    /// `None`: the ink.
    pub fg: Option<Rgb>,
    /// `None`: the paper.
    pub bg: Option<Rgb>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextArt {
    pub cols: usize,
    pub rows: usize,
    pub cells: Vec<Cell>,
    pub ink: Rgb,
    pub paper: Rgb,
}

const QUADRANTS: [char; 16] = [' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█'];
const HALVES: [char; 4] = [' ', '▀', '▄', '█'];

/// Rows for `cols` character cells over `print`: a cell is twice as tall
/// as it is wide.
pub fn rows_for(print: &Print, cols: usize) -> usize {
    ((cols as f32 * print.aspect() / 2.).round() as usize).max(1)
}

/// The print as text art. `palette` is the recipe's colours; `paper` and
/// `ink` the two the art sits in when it isn't coloured per cell.
pub fn make(print: &Print, r: &Recipe, adjust: Adjust, palette: &[Rgb], paper: Rgb, ink: Rgb) -> TextArt {
    let cols = r.ascii_cols as usize;
    let rows = rows_for(print, cols);
    let tint = |x: usize, y: usize, avg: &[Rgb]| -> Option<Rgb> {
        match r.ascii_tint {
            Tint::Ink => None,
            Tint::Photo => Some(avg[y * cols + x]),
            Tint::Palette => Some(nearest(palette, avg[y * cols + x])),
        }
    };
    let averages = || -> Vec<Rgb> { print.resize(cols as u32, rows as u32).rgb.iter().map(|&c| adjust.color(c)).collect() };

    let cells: Vec<Cell> = match r.glyphs {
        Glyphs::Characters => {
            let lines = studio::ascii_lines(print, cols, adjust, r.charset, r.fit);
            let avg = if r.ascii_tint == Tint::Ink { Vec::new() } else { averages() };
            let mut cells = Vec::with_capacity(cols * lines.len());
            for (y, line) in lines.iter().enumerate() {
                for (x, ch) in line.chars().chain(std::iter::repeat(' ')).take(cols).enumerate() {
                    cells.push(Cell { ch, fg: if avg.is_empty() || y >= rows { None } else { tint(x, y, &avg) }, bg: None });
                }
            }
            return TextArt { rows: cells.len() / cols.max(1), cols, cells, ink, paper };
        }
        Glyphs::ColorBlocks => {
            let (sw, sh) = (cols as u32, rows as u32 * 2);
            let pixels: Vec<Rgb> = print.resize(sw, sh).rgb.iter().map(|&c| adjust.color(c)).collect();
            let index = engine::dither(&pixels, sw, sh, palette, r.params());
            (0..rows)
                .flat_map(|y| (0..cols).map(move |x| (x, y)))
                .map(|(x, y)| {
                    let top = palette[index[(y * 2) * cols + x] as usize];
                    let bottom = palette[index[(y * 2 + 1) * cols + x] as usize];
                    Cell { ch: '▀', fg: Some(top), bg: Some(bottom) }
                })
                .collect()
        }
        g => {
            let (sx, sy) = g.sub();
            let (sw, sh) = (cols as u32 * sx, rows as u32 * sy);
            let pixels: Vec<Rgb> = print.resize(sw, sh).rgb.iter().map(|&c| adjust.color(c)).collect();
            // Dots go where the photo is like the ink: light ink on dark
            // paper inks the light parts, dark ink the dark parts.
            let index = engine::dither(&pixels, sw, sh, &[paper, ink], r.params());
            let on = |x: u32, y: u32| index[(y * sw + x) as usize] == 1;
            let avg = if r.ascii_tint == Tint::Ink { Vec::new() } else { averages() };
            (0..rows)
                .flat_map(|y| (0..cols).map(move |x| (x, y)))
                .map(|(x, y)| {
                    let (bx, by) = (x as u32 * sx, y as u32 * sy);
                    let ch = match g {
                        Glyphs::Braille => {
                            let dots = [(0, 0, 0x01), (0, 1, 0x02), (0, 2, 0x04), (1, 0, 0x08), (1, 1, 0x10), (1, 2, 0x20), (0, 3, 0x40), (1, 3, 0x80)];
                            let bits = dots.iter().filter(|&&(dx, dy, _)| on(bx + dx, by + dy)).fold(0u32, |b, d| b | d.2);
                            // An empty cell is a space, not the blank braille glyph,
                            // so the text stays light and copy-pastes cleanly.
                            if bits == 0 { ' ' } else { char::from_u32(0x2800 + bits).unwrap_or(' ') }
                        }
                        Glyphs::HalfBlocks => HALVES[on(bx, by) as usize | (on(bx, by + 1) as usize) << 1],
                        _ => QUADRANTS[on(bx, by) as usize | (on(bx + 1, by) as usize) << 1 | (on(bx, by + 1) as usize) << 2 | (on(bx + 1, by + 1) as usize) << 3],
                    };
                    Cell { ch, fg: if avg.is_empty() { None } else { tint(x, y, &avg) }, bg: None }
                })
                .collect()
        }
    };
    TextArt { cols, rows, cells, ink, paper }
}

fn nearest(palette: &[Rgb], c: Rgb) -> Rgb {
    let lab = oklab(c);
    *palette
        .iter()
        .min_by(|a, b| {
            let d = |p: &Rgb| {
                let o = oklab(*p);
                (o[0] - lab[0]).powi(2) + (o[1] - lab[1]).powi(2) + (o[2] - lab[2]).powi(2)
            };
            d(a).total_cmp(&d(b))
        })
        .unwrap_or(&c)
}

fn rgb8(c: Rgb) -> [u8; 3] {
    c.map(|v| (v.clamp(0., 1.) * 255.).round() as u8)
}

fn css(c: Rgb) -> String {
    let [r, g, b] = rgb8(c);
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// The colours an HTML page uses, numbered for classes `f<n>` (text) and
/// `b<n>` (background).
#[derive(Default)]
struct Colours {
    ids: std::collections::HashMap<[u8; 3], usize>,
    list: Vec<[u8; 3]>,
}

impl Colours {
    fn id(&mut self, c: Rgb) -> usize {
        let key = rgb8(c);
        *self.ids.entry(key).or_insert_with(|| {
            self.list.push(key);
            self.list.len() - 1
        })
    }

    fn css(&self) -> String {
        let mut s = String::from("i{font-style:normal}");
        for (n, [r, g, b]) in self.list.iter().enumerate() {
            s.push_str(&format!(".f{n}{{color:#{r:02x}{g:02x}{b:02x}}}.b{n}{{background:#{r:02x}{g:02x}{b:02x}}}"));
        }
        s
    }
}

/// A run of cells on one row sharing colours.
pub struct Run {
    pub text: String,
    pub fg: Rgb,
    pub bg: Option<Rgb>,
}

impl TextArt {
    pub fn row(&self, y: usize) -> &[Cell] {
        &self.cells[y * self.cols..(y + 1) * self.cols]
    }

    /// One row as runs of the same colours (what a view or HTML draws).
    pub fn runs(&self, y: usize) -> Vec<Run> {
        let mut runs: Vec<Run> = Vec::new();
        for cell in self.row(y) {
            let (fg, bg) = (cell.fg.unwrap_or(self.ink), cell.bg);
            match runs.last_mut() {
                Some(run) if run.fg == fg && run.bg == bg => run.text.push(cell.ch),
                _ => runs.push(Run { text: cell.ch.to_string(), fg, bg }),
            }
        }
        runs
    }

    /// Plain text, trailing spaces trimmed.
    pub fn text(&self) -> String {
        let lines: Vec<String> = (0..self.rows).map(|y| self.row(y).iter().map(|c| c.ch).collect::<String>().trim_end().to_string()).collect();
        let mut s = lines.join("\n");
        s.push('\n');
        s
    }

    /// Text with 24-bit ANSI colours, for a terminal (`cat print.ans`).
    pub fn ansi(&self) -> String {
        let mut s = String::new();
        for y in 0..self.rows {
            let mut cur: Option<(Rgb, Option<Rgb>)> = None;
            for cell in self.row(y) {
                let key = (cell.fg.unwrap_or(self.ink), cell.bg);
                if cur != Some(key) {
                    let [r, g, b] = rgb8(key.0);
                    s.push_str(&format!("\x1b[38;2;{r};{g};{b}m"));
                    match key.1 {
                        Some(bg) => {
                            let [r, g, b] = rgb8(bg);
                            s.push_str(&format!("\x1b[48;2;{r};{g};{b}m"));
                        }
                        None => s.push_str("\x1b[49m"),
                    }
                    cur = Some(key);
                }
                s.push(cell.ch);
            }
            s.push_str("\x1b[0m\n");
        }
        s
    }

    /// The rows as HTML spans (no wrapper), for `html` and the film. Colours
    /// are short classes from `colours`, shared across a film's frames, so
    /// a cell costs a few bytes rather than an inline style.
    fn html_body(&self, colours: &mut Colours) -> String {
        let mut s = String::new();
        for y in 0..self.rows {
            for run in self.runs(y) {
                let text = run.text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
                match run.bg {
                    Some(bg) => s.push_str(&format!("<i class=\"f{} b{}\">{text}</i>", colours.id(run.fg), colours.id(bg))),
                    None if run.fg == self.ink => s.push_str(&text),
                    None => s.push_str(&format!("<i class=\"f{}\">{text}</i>", colours.id(run.fg))),
                }
            }
            s.push('\n');
        }
        s
    }

    fn pre_style(&self) -> String {
        format!(
            "margin:0;padding:16px;background:{};color:{};font:16px/1 'JetBrains Mono','DejaVu Sans Mono',Menlo,Consolas,monospace;letter-spacing:0",
            css(self.paper),
            css(self.ink)
        )
    }

    /// A self-contained HTML page.
    pub fn html(&self, title: &str) -> String {
        let mut colours = Colours::default();
        let body = self.html_body(&mut colours);
        format!(
            "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>{}</title><style>{}</style></head>\n<body style=\"margin:0;background:{}\"><pre style=\"{}\">{}</pre></body></html>\n",
            title.replace('<', "&lt;"),
            colours.css(),
            css(self.paper),
            self.pre_style(),
            body
        )
    }

    /// An SVG: one text element per run, stretched to the cell grid so any
    /// monospace font lines up; backgrounds as rectangles.
    pub fn svg(&self) -> String {
        let (cw, ch) = (8usize, 16usize);
        let (w, h) = (self.cols * cw, self.rows * ch);
        let mut s = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\n<rect width=\"100%\" height=\"100%\" fill=\"{}\"/>\n<g font-family=\"JetBrains Mono, DejaVu Sans Mono, Menlo, Consolas, monospace\" font-size=\"14\" xml:space=\"preserve\">\n",
            css(self.paper)
        );
        for y in 0..self.rows {
            let mut x = 0;
            for run in self.runs(y) {
                let n = run.text.chars().count();
                if let Some(bg) = run.bg {
                    s.push_str(&format!("<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{ch}\" fill=\"{}\"/>\n", x * cw, y * ch, n * cw, css(bg)));
                }
                if run.text.chars().any(|c| c != ' ') {
                    let text = run.text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
                    s.push_str(&format!(
                        "<text x=\"{}\" y=\"{}\" textLength=\"{}\" lengthAdjust=\"spacingAndGlyphs\" fill=\"{}\">{text}</text>\n",
                        x * cw,
                        y * ch + 12,
                        n * cw,
                        css(run.fg)
                    ));
                }
                x += n;
            }
        }
        s.push_str("</g>\n</svg>\n");
        s
    }
}

/// Which quarters of a cell a block glyph fills: top-left, top-right,
/// bottom-left, bottom-right as bits 1, 2, 4, 8.
fn block_bits(ch: char) -> Option<u8> {
    QUADRANTS.iter().position(|&q| q == ch).map(|i| i as u8)
}

impl TextArt {
    /// Whether every glyph is braille, a block or a space: art that can be
    /// drawn exactly as pixels without a font.
    #[cfg(test)]
    pub fn is_drawable(&self) -> bool {
        self.cells.iter().all(|c| c.ch == ' ' || block_bits(c.ch).is_some() || ('\u{2801}'..='\u{28FF}').contains(&c.ch))
    }

    /// The art's size at one pixel per glyph pixel: 8×16 a cell.
    pub fn native(&self) -> (u32, u32) {
        (self.cols as u32 * 8, self.rows as u32 * 16)
    }

    /// The art drawn as pixels: cells of 8×16 × `scale`. RGBA.
    #[cfg(test)]
    pub fn raster(&self, scale: u32) -> (u32, u32, Vec<u8>) {
        let (w, h) = self.native();
        self.raster_sized(w * scale.max(1), h * scale.max(1))
    }

    /// The art drawn into exactly `w`×`h` pixels: braille dots as round
    /// dots, blocks as solid quarters, characters from the display face's
    /// own 8×16 bitmaps (nearest pixel). RGBA.
    pub fn raster_sized(&self, w: u32, h: u32) -> (u32, u32, Vec<u8>) {
        let (w, h) = (w.max(1), h.max(1));
        let mut out = vec![0u8; w as usize * h as usize * 4];
        let edge = |a: usize, n: usize, out: u32| (a as u64 * out as u64 / n.max(1) as u64) as u32;
        let bitmaps: std::collections::HashMap<char, [u8; 16]> = crate::glyphs::GLYPHS.iter().copied().collect();
        let rgba = |c: Rgb| {
            let [r, g, b] = rgb8(c);
            [r, g, b, 255]
        };
        let mut fill = |x0: u32, y0: u32, x1: u32, y1: u32, c: [u8; 4], round: Option<(f32, f32, f32)>| {
            for y in y0..y1.min(h) {
                for x in x0..x1.min(w) {
                    if let Some((cx, cy, r)) = round
                        && (x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2) > r * r
                    {
                        continue;
                    }
                    out[(y as usize * w as usize + x as usize) * 4..][..4].copy_from_slice(&c);
                }
            }
        };
        for (i, cell) in self.cells.iter().enumerate() {
            let (col, row) = (i % self.cols, i / self.cols);
            let (x0, y0) = (edge(col, self.cols, w), edge(row, self.rows, h));
            let (cw, ch) = (edge(col + 1, self.cols, w) - x0, edge(row + 1, self.rows, h) - y0);
            fill(x0, y0, x0 + cw, y0 + ch, rgba(cell.bg.unwrap_or(self.paper)), None);
            let ink = rgba(cell.fg.unwrap_or(self.ink));
            if let Some(bits) = block_bits(cell.ch) {
                for (q, (qx, qy)) in [(0, 0), (1, 0), (0, 1), (1, 1)].into_iter().enumerate() {
                    if bits & (1 << q) != 0 {
                        fill(x0 + qx * cw / 2, y0 + qy * ch / 2, x0 + (qx + 1) * cw / 2, y0 + (qy + 1) * ch / 2, ink, None);
                    }
                }
            } else if ('\u{2801}'..='\u{28FF}').contains(&cell.ch) {
                let bits = cell.ch as u32 - 0x2800;
                let dots = [(0, 0, 0x01), (0, 1, 0x02), (0, 2, 0x04), (1, 0, 0x08), (1, 1, 0x10), (1, 2, 0x20), (0, 3, 0x40), (1, 3, 0x80)];
                let r = (cw as f32 / 2.).min(ch as f32 / 4.) * 0.36;
                for (dx, dy, bit) in dots {
                    if bits & bit != 0 {
                        let (cx, cy) = (x0 as f32 + (dx as f32 + 0.5) * cw as f32 / 2., y0 as f32 + (dy as f32 + 0.5) * ch as f32 / 4.);
                        fill((cx - r).floor() as u32, (cy - r).floor() as u32, (cx + r).ceil() as u32, (cy + r).ceil() as u32, ink, Some((cx, cy, r)));
                    }
                }
            } else if let Some(rows) = bitmaps.get(&cell.ch) {
                for y in 0..ch {
                    let bits = rows[(y * 16 / ch.max(1)) as usize];
                    if bits == 0 {
                        continue;
                    }
                    for x in 0..cw {
                        if bits & (0x80 >> (x * 8 / cw.max(1))) != 0 {
                            fill(x0 + x, y0 + y, x0 + x + 1, y0 + y + 1, ink, None);
                        }
                    }
                }
            }
        }
        (w, h, out)
    }

    #[cfg(test)]
    pub fn png(&self, scale: u32) -> Result<Vec<u8>, String> {
        let (w, h, px) = self.raster(scale);
        crate::export::encode(w, h, px, crate::export::Format::Png, self.paper)
    }

    /// A picture of the art in any format at `size` (SVG: real text).
    pub fn image(&self, size: crate::export::Size, scale: u32, format: crate::export::Format) -> Result<Vec<u8>, String> {
        if format == crate::export::Format::Svg {
            return Ok(self.svg().into_bytes());
        }
        let (nw, nh) = self.native();
        let (w, h) = size.dims(nw, nh, scale);
        let (w, h, px) = self.raster_sized(w, h);
        crate::export::encode(w, h, px, format, self.paper)
    }
}

/// An animation as one HTML page that plays its frames in a loop.
pub fn film(frames: &[TextArt], delays: &[u32], title: &str) -> String {
    let Some(first) = frames.first() else { return String::new() };
    let json_str = |s: &str| {
        let mut out = String::from("\"");
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '<' => out.push_str("\\u003c"),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    };
    let mut colours = Colours::default();
    let bodies: Vec<String> = frames.iter().map(|f| json_str(&f.html_body(&mut colours))).collect();
    let delays: Vec<String> = delays.iter().map(|d| d.to_string()).collect();
    format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>{}</title><style>{}</style></head>\n<body style=\"margin:0;background:{}\"><pre id=\"f\" style=\"{}\"></pre>\n<script>\nconst frames=[{}];\nconst delays=[{}];\nlet i=0;const el=document.getElementById('f');\nfunction step(){{el.innerHTML=frames[i];const d=delays[i]||100;i=(i+1)%frames.length;setTimeout(step,d);}}\nstep();\n</script></body></html>\n",
        title.replace('<', "&lt;"),
        colours.css(),
        css(first.paper),
        first.pre_style(),
        bodies.join(","),
        delays.join(",")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Algo;

    const PAPER: Rgb = [0., 0., 0.];
    const INK: Rgb = [1., 1., 1.];

    /// Left half black, right half white.
    fn split() -> Print {
        Print::from_rgb(64, 64, (0..64 * 64).map(|i| if i % 64 < 32 { [0.; 3] } else { [1.; 3] }).collect())
    }

    fn recipe(glyphs: Glyphs) -> Recipe {
        Recipe { glyphs, ascii_cols: 16, algo: Algo::Threshold, ..Recipe::default() }
    }

    fn make_with(r: &Recipe) -> TextArt {
        make(&split(), r, Adjust::default(), &[PAPER, INK], PAPER, INK)
    }

    #[test]
    fn braille_packs_eight_dots() {
        let art = make_with(&recipe(Glyphs::Braille));
        assert_eq!((art.cols, art.rows), (16, 8));
        let row = art.row(3);
        // Dark side empty, light side full braille (all eight dots).
        assert_eq!(row[0].ch, ' ');
        assert_eq!(row[15].ch, '\u{28FF}');
    }

    #[test]
    fn blocks_follow_the_picture() {
        for g in [Glyphs::HalfBlocks, Glyphs::Quadrants] {
            let art = make_with(&recipe(g));
            assert_eq!(art.row(2)[0].ch, ' ', "{g:?}");
            assert_eq!(art.row(2)[15].ch, '█', "{g:?}");
        }
        // A quadrant straddling the edge is half on: the right column.
        let narrow = make(&split(), &Recipe { ascii_cols: 1, ..recipe(Glyphs::Quadrants) }, Adjust::default(), &[PAPER, INK], PAPER, INK);
        assert_eq!(narrow.row(0)[0].ch, '▐');
    }

    #[test]
    fn colour_blocks_carry_two_colours() {
        let r = Recipe { glyphs: Glyphs::ColorBlocks, ascii_cols: 16, algo: Algo::Threshold, ..Recipe::default() };
        let art = make(&split(), &r, Adjust::default(), &[[1., 0., 0.], [0., 0., 1.], [0., 0., 0.], [1., 1., 1.]], PAPER, INK);
        let cell = art.row(2)[15];
        assert_eq!((cell.ch, cell.fg, cell.bg), ('▀', Some([1., 1., 1.]), Some([1., 1., 1.])));
        assert_eq!(art.row(2)[0].fg, Some([0., 0., 0.]));
    }

    #[test]
    fn characters_and_tints() {
        let mut r = recipe(Glyphs::Characters);
        let plain = make_with(&r);
        assert_eq!(plain.cols, 16);
        assert!(plain.cells.iter().all(|c| c.fg.is_none()));
        r.ascii_tint = Tint::Photo;
        let photo = make_with(&r);
        assert_eq!(photo.row(1)[15].fg, Some([1.; 3]));
        r.ascii_tint = Tint::Palette;
        let pal = make(&split(), &r, Adjust::default(), &[[0.9, 0.1, 0.1], [0.1, 0.1, 0.1]], PAPER, INK);
        assert_eq!(pal.row(1)[15].fg, Some([0.9, 0.1, 0.1]));
    }

    #[test]
    fn exports() {
        let r = Recipe { glyphs: Glyphs::ColorBlocks, ascii_cols: 8, algo: Algo::Threshold, ..Recipe::default() };
        let art = make(&split(), &r, Adjust::default(), &[[0., 0., 0.], [1., 1., 1.]], PAPER, INK);
        let text = art.text();
        assert_eq!(text.lines().count(), art.rows);
        let ansi = art.ansi();
        assert!(ansi.contains("\x1b[38;2;255;255;255m") && ansi.contains("\x1b[48;2;0;0;0m") && ansi.ends_with("\x1b[0m\n"));
        let html = art.html("t<est");
        assert!(html.starts_with("<!doctype html>") && html.contains("{background:#ffffff}") && html.contains("t&lt;est") && html.contains("<i class=\"f"));
        let svg = art.svg();
        assert!(svg.starts_with("<svg") && svg.contains("width=\"64\"") && svg.contains("<rect x="));
        let film = film(&[art.clone(), art], &[80, 120], "loop");
        assert!(film.contains("const delays=[80,120]") && !film.contains("</pre>\\n"));
    }

    #[test]
    fn html_escapes_markup() {
        let art = TextArt { cols: 3, rows: 1, cells: "<&>".chars().map(|ch| Cell { ch, fg: None, bg: None }).collect(), ink: INK, paper: PAPER };
        assert!(art.html("x").contains("&lt;&amp;&gt;"));
        assert!(art.svg().contains("&lt;&amp;&gt;"));
        // The film keeps the entities; only markup '<' is escaped for the script.
        let film = film(&[art], &[100], "x");
        assert!(film.contains("&lt;&amp;&gt;") && !film.contains("const frames=[\"<"));
    }

    #[test]
    fn rasters_blocks_and_braille() {
        let cells = vec![Cell { ch: '▐', fg: None, bg: None }, Cell { ch: '\u{2801}', fg: Some([1., 0., 0.]), bg: None }, Cell { ch: '▀', fg: None, bg: Some([0., 0., 1.]) }];
        let art = TextArt { cols: 3, rows: 1, cells, ink: INK, paper: PAPER };
        assert!(art.is_drawable());
        let (w, h, px) = art.raster(1);
        assert_eq!((w, h), (24, 16));
        let at = |x: u32, y: u32| -> [u8; 4] { px[((y * w + x) * 4) as usize..][..4].try_into().unwrap() };
        // ▐: right half ink, left half paper.
        assert_eq!((at(6, 8), at(1, 8)), ([255, 255, 255, 255], [0, 0, 0, 255]));
        // Braille dot 1: top-left dot in red, the rest paper.
        assert_eq!((at(10, 2), at(14, 14)), ([255, 0, 0, 255], [0, 0, 0, 255]));
        // ▀ with a blue background: ink above, blue below.
        assert_eq!((at(20, 3), at(20, 12)), ([255, 255, 255, 255], [0, 0, 255, 255]));
        assert!(image::load_from_memory(&art.png(2).unwrap()).is_ok());
        let letters = TextArt { cols: 1, rows: 1, cells: vec![Cell { ch: 'A', fg: None, bg: None }], ink: INK, paper: PAPER };
        assert!(!letters.is_drawable());
    }

    #[test]
    fn rasters_characters_from_the_font() {
        let letters = TextArt { cols: 2, rows: 1, cells: "A ".chars().map(|ch| Cell { ch, fg: None, bg: None }).collect(), ink: INK, paper: PAPER };
        let (w, _, px) = letters.raster(1);
        let ink = |x0: u32, x1: u32| (x0..x1).flat_map(|x| (0..16).map(move |y| (x, y))).filter(|&(x, y)| px[((y * w + x) * 4) as usize] == 255).count();
        // 'A' has ink; the space has none.
        assert!(ink(0, 8) > 20 && ink(8, 16) == 0);
        // Any size: 8K-style odd sizes keep the glyph.
        let (sw, sh, big) = letters.raster_sized(37, 29);
        assert_eq!((sw, sh), (37, 29));
        assert!(big.as_chunks::<4>().0.iter().any(|p| p[0] == 255));
        for f in crate::export::Format::ALL {
            assert!(letters.image(crate::export::Size::Long(320), 1, f).is_ok(), "{f:?}");
        }
    }

    #[test]
    fn keys_round_trip() {
        for g in Glyphs::ALL {
            assert_eq!(Glyphs::from_key(g.key()), Some(g));
        }
        for t in Tint::ALL {
            assert_eq!(Tint::from_key(t.key()), Some(t));
        }
    }
}
