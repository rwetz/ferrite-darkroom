//! Recipe discovery and bounded previews, independent of the window.
use crate::{engine::Rgb, mask, recipe::{BUNDLED, Mode, Recipe}, render, studio::{self, Adjust, Print}, textart};

#[derive(Clone, Copy, Default, PartialEq)]
pub enum Category { #[default] All, Print, Pixel, Text, Favorites }

impl Category {
    pub const ALL: [Self; 5] = [Self::All, Self::Print, Self::Pixel, Self::Text, Self::Favorites];
    pub fn name(self) -> &'static str {
        match self { Self::All => "All", Self::Print => "Print", Self::Pixel => "Pixel", Self::Text => "Text", Self::Favorites => "Favorites" }
    }
}

pub fn key(name: &str) -> String { name.to_ascii_lowercase().replace(' ', "-") }

pub fn category(name: &str, recipe: &Recipe) -> Category {
    if recipe.mode == Mode::Ascii { Category::Text }
    else if matches!(name, "Game Boy" | "Sunset" | "Dots" | "Lattice" | "Ice Mosaic" | "Candy Dots") { Category::Pixel }
    else { Category::Print }
}

pub fn description(source: &str) -> String {
    let description = source.lines().take_while(|line| line.starts_with('#')).map(|line| line.trim_start_matches('#').trim())
        .collect::<Vec<_>>().join(" ");
    let description = description.trim_start_matches("A Darkroom recipe: ");
    description.split_once(". ").map_or(description.to_string(), |(first, _)| format!("{first}."))
}

pub struct Preview {
    pub recipe: Recipe,
    pub colors: Vec<Rgb>,
    pub w: u32,
    pub h: u32,
    pub bgra: Vec<u8>,
}

/// All recipes on the current frame/crop. Keep computation and GPU uploads separate.
pub fn previews(print: &Print, reference: &Print, inks: [[Rgb; 3]; 2], paint: Option<&mask::Paint>) -> Vec<Preview> {
    BUNDLED.iter().map(|(_, source)| {
        let recipe = Recipe::from_toml(source).expect("bundled recipe validated by tests");
        let [paper, text, accent] = inks[usize::from(recipe.mode == Mode::Ascii)];
        let ink = if recipe.accent_ink { accent } else { text };
        let colors = recipe.palette_colors(paper, ink);
        let mut small = recipe.clone();
        small.cols = (48. / print.aspect().max(0.01)).clamp(1., 64.) as u32;
        small.ascii_cols = (24. / print.aspect().max(0.01)).clamp(1., 32.) as u32;
        let lightness = |rgb: Rgb| (rgb.into_iter().fold(f32::MIN, f32::max) + rgb.into_iter().fold(f32::MAX, f32::min)) * 0.5;
        let adjust = Adjust::new(&small, lightness(ink) > lightness(paper), reference);
        let (w, h, bgra) = match recipe.mode {
            Mode::Dither => {
                let art = studio::develop_recipe(print, &small, adjust, colors.clone(), paint, None);
                render::bgra(&art, 4, small.look())
            }
            Mode::Ascii => {
                let text = textart::make(print, &small, adjust, &colors, paper, ink, paint);
                let (nw, nh) = text.native();
                let factor = (256. / nw.max(1) as f32).min(192. / nh.max(1) as f32);
                let (w, h, mut rgba) = text.raster_sized((nw as f32 * factor).max(1.) as u32, (nh as f32 * factor).max(1.) as u32);
                for pixel in rgba.chunks_exact_mut(4) { pixel.swap(0, 2); }
                (w, h, rgba)
            }
        };
        let (w, h, bgra) = if w > 256 || h > 192 {
            let factor = (256. / w as f32).min(192. / h as f32);
            let image = image::RgbaImage::from_raw(w, h, bgra).expect("preview dimensions match pixels");
            let image = image::imageops::resize(&image, (w as f32 * factor).max(1.) as u32, (h as f32 * factor).max(1.) as u32, image::imageops::FilterType::Nearest);
            (image.width(), image.height(), image.into_raw())
        } else { (w, h, bgra) };
        Preview { recipe, colors, w, h, bgra }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn previews_are_bounded_and_keys_are_unique() {
        let mut keys = Vec::new();
        for (name, source) in BUNDLED {
            keys.push(key(name));
            assert!(!description(source).is_empty());
        }
        keys.sort(); keys.dedup(); assert_eq!(keys.len(), BUNDLED.len());
        for print in [studio::sample(), Print::from_rgb(4, 160, vec![[0.4, 0.6, 0.8]; 640]), Print::from_rgb(1, 600, vec![[0.4, 0.6, 0.8]; 600])] {
            let previews = previews(&print, &print, [[[0.05; 3], [0.95; 3], [0.8, 0.5, 0.2]]; 2], None);
            assert_eq!(previews.len(), BUNDLED.len());
            for preview in previews {
                assert!(preview.w > 0 && preview.w <= 256 && preview.h > 0 && preview.h <= 192);
                assert_eq!(preview.bgra.len(), (preview.w * preview.h * 4) as usize);
            }
        }
    }
}
