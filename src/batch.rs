//! Developing files without looking at them: one file or a whole folder,
//! through a recipe, to the format the output's extension asks for. The
//! command line and the app's batch command both run this.

use std::path::{Path, PathBuf};

use crate::engine::{Rgb, oklab};
use crate::export::{Format, Size};
use crate::recipe::Recipe;
use crate::studio::{self, Adjust};
use crate::{render, sequence, textart};

/// What to develop with: the recipe, and the scheme's paper and ink (for
/// the Scheme palette and for ASCII).
#[derive(Clone)]
pub struct Job {
    pub recipe: Recipe,
    pub paper: Rgb,
    pub ink: Rgb,
    /// A painted mask, for recipes with `mask = "painted"`.
    pub paint: Option<crate::mask::Paint>,
    /// How big pictures come out.
    pub size: Size,
}

/// A folder batch's result: the files written, and the inputs that failed
/// with why.
pub type Outcome = (Vec<PathBuf>, Vec<(PathBuf, String)>);

/// The files a folder batch picks up.
const INPUTS: [&str; 14] = ["png", "jpg", "jpeg", "gif", "bmp", "webp", "apng", "mp4", "mov", "webm", "mkv", "avi", "m4v", "mpg"];

impl Job {
    /// The scheme's colours by key and appearance, without a window.
    pub fn with_scheme(recipe: Recipe, scheme: &str, light: bool) -> Job {
        use ferrite_design::tokens::Tone;
        let scheme = ferrite_design::schemes::by_key(scheme).unwrap_or(&ferrite_design::schemes::SCHEMES[0]);
        let p = scheme.palette(if light { Tone::Light } else { Tone::Dark });
        let rgb = |hex: u32| [(hex >> 16) & 0xff, (hex >> 8) & 0xff, hex & 0xff].map(|c| c as f32 / 255.);
        let ink = if recipe.accent_ink { p.accent } else { p.fg };
        Job { recipe, paper: rgb(p.bg), ink: rgb(ink), paint: None, size: Size::Scale }
    }

    fn adjust(&self) -> Adjust {
        let r = &self.recipe;
        let light_ink = oklab(self.ink)[0] > oklab(self.paper)[0];
        Adjust { brightness: r.brightness, contrast: r.contrast, gamma: r.gamma, invert: r.invert, light_ink }
    }

    /// Develop `input` into `output`. `.gif` makes a GIF (a still makes a
    /// one-frame GIF), `.png` a PNG (an APNG for an animation), `.jpg`,
    /// `.webp`, `.bmp` and `.tif` a still of the first frame; `.txt`,
    /// `.ans`, `.html` and `.svg` make text art of the first frame (an
    /// animation's `.html` is a film that plays every frame).
    pub fn develop_file(&self, input: &Path, output: &Path) -> Result<(), String> {
        let ext = output.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let clip = sequence::load(input)?;
        let r = &self.recipe;
        let adjust = self.adjust();
        let colors = r.palette_colors(self.paper, self.ink);
        let size = self.size;
        let bytes = match ext.as_str() {
            "jpg" | "jpeg" | "webp" | "bmp" | "tif" | "tiff" => {
                let format = Format::from_key(&ext).unwrap_or_default();
                match r.mode {
                    crate::recipe::Mode::Ascii => textart::make(&clip.frames[0].print, r, adjust, &colors, self.paper, self.ink).image(size, r.scale, format)?,
                    crate::recipe::Mode::Dither => {
                        let art = sequence::develop_all(&clip.frames[..1], r, adjust, &colors, self.paint.as_ref()).remove(0);
                        render::still(&art, size, r.scale, format, r.look())?
                    }
                }
            }
            "gif" | "png" | "apng" | "mp4" => {
                let arts = sequence::develop_all(&clip.frames, r, adjust, &colors, self.paint.as_ref());
                let delays = sequence::delays(&clip.frames, r.speed);
                match ext.as_str() {
                    "gif" => sequence::gif(&arts, &delays, size, r.scale, r.look())?,
                    "mp4" => sequence::mp4(&arts, &delays, size, r.scale, r.look())?,
                    _ if clip.is_animated() => sequence::apng(&arts, &delays, size, r.scale, r.look())?,
                    _ => render::still(&arts[0], size, r.scale, Format::Png, r.look())?,
                }
            }
            "txt" | "ans" | "html" | "svg" => {
                let art = |print: &studio::Print| textart::make(print, r, adjust, &colors, self.paper, self.ink);
                let first = art(&clip.frames[0].print);
                let title = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                match ext.as_str() {
                    "txt" => first.text(),
                    "ans" => first.ansi(),
                    "svg" => first.svg(),
                    // An animation becomes a film that plays itself.
                    _ if clip.is_animated() => {
                        let frames: Vec<_> = clip.frames.iter().map(|f| art(&f.print)).collect();
                        textart::film(&frames, &sequence::delays(&clip.frames, r.speed), &title)
                    }
                    _ => first.html(&title),
                }
                .into_bytes()
            }
            "" => return Err(format!("{} has no extension; use .png, .jpg, .webp, .bmp, .tif, .gif, .mp4, .txt, .ans, .html or .svg", output.display())),
            other => return Err(format!(".{other} isn't an output Darkroom makes; use .png, .jpg, .webp, .bmp, .tif, .gif, .mp4, .txt, .ans, .html or .svg")),
        };
        if let Some(dir) = output.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(output, bytes).map_err(|e| format!("couldn't write {}: {e}", output.display()))
    }

    /// Develop every picture in `dir` (not its subfolders) into `out_dir`:
    /// animations as GIFs, stills as PNGs. Returns what was written and what
    /// failed, with why.
    pub fn develop_folder(&self, dir: &Path, out_dir: &Path) -> Result<Outcome, String> {
        let mut inputs: Vec<PathBuf> = std::fs::read_dir(dir)
            .map_err(|e| format!("couldn't read {}: {e}", dir.display()))?
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| INPUTS.contains(&e.to_string_lossy().to_lowercase().as_str())))
            .collect();
        inputs.sort();
        let (mut done, mut failed) = (Vec::new(), Vec::new());
        for input in inputs {
            let stem = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "print".into());
            let animated = sequence::load(&input).map(|c| c.is_animated()).unwrap_or(false);
            let output = out_dir.join(format!("{stem}.{}", if animated { "gif" } else { "png" }));
            match self.develop_file(&input, &output) {
                Ok(()) => done.push(output),
                Err(why) => failed.push((input, why)),
            }
        }
        Ok((done, failed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Algo;
    use crate::recipe::Mode;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("darkroom-batch-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn job() -> Job {
        Job::with_scheme(Recipe { algo: Algo::Bayer4, palette: "bw".into(), cols: 40, scale: 2, ..Recipe::default() }, "ferrite", false)
    }

    fn write_still(path: &Path) {
        image::RgbImage::from_fn(80, 40, |x, _| image::Rgb([(x * 3) as u8; 3])).save(path).unwrap();
    }

    #[test]
    fn a_still_to_png_gif_and_text() {
        let dir = scratch("still");
        let input = dir.join("ramp.png");
        write_still(&input);
        let job = job();
        job.develop_file(&input, &dir.join("out.png")).unwrap();
        let png = image::open(dir.join("out.png")).unwrap();
        assert_eq!((png.width(), png.height()), (80, 40));
        job.develop_file(&input, &dir.join("out.gif")).unwrap();
        assert!(image::open(dir.join("out.gif")).is_ok());
        let ascii = Job { recipe: Recipe { mode: Mode::Ascii, ascii_cols: 30, ..job.recipe.clone() }, ..job };
        ascii.develop_file(&input, &dir.join("out.txt")).unwrap();
        let text = std::fs::read_to_string(dir.join("out.txt")).unwrap();
        assert!(text.lines().count() > 3);
        assert!(job_err(&input, &dir.join("out.psd")).contains(".psd"));
        // Stills in other formats, at a size.
        let big = Job { size: Size::Long(1920), ..super::tests::job() };
        for ext in ["jpg", "webp", "bmp", "tif"] {
            big.develop_file(&input, &dir.join(format!("out.{ext}"))).unwrap();
            let img = image::open(dir.join(format!("out.{ext}"))).unwrap();
            assert_eq!(img.width(), 1920, "{ext}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn job_err(input: &Path, output: &Path) -> String {
        job().develop_file(input, output).unwrap_err()
    }

    #[test]
    fn a_folder() {
        let dir = scratch("folder");
        write_still(&dir.join("a.png"));
        write_still(&dir.join("b.bmp"));
        std::fs::write(dir.join("notes.txt"), "not a picture").unwrap();
        std::fs::write(dir.join("broken.png"), "not a png either").unwrap();
        let out = dir.join("darkroom");
        let (done, failed) = job().develop_folder(&dir, &out).unwrap();
        assert_eq!(done, vec![out.join("a.png"), out.join("b.png")]);
        assert_eq!(failed.len(), 1);
        assert!(failed[0].0.ends_with("broken.png"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scheme_colours_without_a_window() {
        let dark = Job::with_scheme(Recipe::default(), "ferrite", false);
        let light = Job::with_scheme(Recipe::default(), "ferrite", true);
        assert!(oklab(dark.paper)[0] < 0.3 && oklab(light.paper)[0] > 0.7);
        assert!(dark.adjust().light_ink && !light.adjust().light_ink);
        // An unknown scheme falls back to the first.
        assert_eq!(Job::with_scheme(Recipe::default(), "nope", false).paper, dark.paper);
    }
}
