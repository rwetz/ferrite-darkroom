//! Darkroom: a Ferrite dither studio.
//!
//! Drop in a photo (or open one) and develop it as pixel art — twenty
//! dither algorithms (error diffusion and ordered) into the scheme's own
//! ink or a preset palette (Game Boy, C64, CMYK…) — or as ASCII with
//! Ferrite's glyph-fitted character sets. Export a PNG or a text file.
//!
//!     cargo run
//!     cargo run -- photo.jpg
//!     cargo run -- photo.jpg -o print.png --recipe recipes/gameboy.toml   (no window; see cli.rs)

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod batch;
mod cli;
mod engine;
mod palettes;
mod recipe;
mod render;
mod sequence;
mod settings;
mod studio;

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ferrite_design::ascii::{Charset, Fit};
use gpui::{
    App, AppContext as _, Bounds, ClickEvent, ClipboardItem, ContentMask, Context, Corners, Entity, ExternalPaths, Hsla, IntoElement,
    KeyBinding, PathPromptOptions, Pixels, Render, RenderImage, Rgba, Subscription, Window, canvas, div, fill, point, px, size,
};
use ferrite_design::prelude::*;

use engine::{Algo, Rgb, Space};
use recipe::{Mode, Recipe};
use render::{Paper, Shape};
use settings::Settings;
use sequence::Clip;
use studio::{Adjust, Art, Print};

const APPEARANCES: [(&str, &str); 3] = [("dark", "Dark"), ("light", "Light"), ("system", "System")];
const SCALES: [u32; 4] = [1, 2, 4, 8];
const SIDEBAR: f32 = 360.;
/// Contact-sheet thumbnails: art width, and the narrowest a tile gets.
const THUMB_COLS: u32 = 96;
const TILE_MIN: f32 = 180.;
/// A tile's padding and border, both sides, and the gap between tiles.
const TILE_FRAME: f32 = 10.;
const TILE_GAP: f32 = 8.;
/// Edits closer together than this (a slider drag) undo as one.
const UNDO_MERGE: Duration = Duration::from_millis(600);
const UNDO_DEPTH: usize = 200;

/// What the print panel shows.
#[derive(Clone, Copy, PartialEq)]
enum View {
    Print,
    /// The photo left of a split, the art right of it.
    Compare,
    /// One thumbnail per algorithm or palette; click to adopt.
    Sheet,
}

#[derive(Clone, Copy, PartialEq, Hash)]
enum SheetKind {
    Algorithms,
    Palettes,
}

/// GPU textures the view draws, keyed by what they show. Every frame marks
/// what it uses; the rest are freed from the atlas at the frame's end.
#[derive(Default)]
struct Textures {
    map: HashMap<u64, (Arc<RenderImage>, bool)>,
}

impl Textures {
    fn begin(&mut self) {
        for entry in self.map.values_mut() {
            entry.1 = false;
        }
    }

    fn get(&mut self, key: u64, make: impl FnOnce() -> (u32, u32, Vec<u8>)) -> Arc<RenderImage> {
        let entry = self.map.entry(key).or_insert_with(|| {
            let (w, h, bgra) = make();
            let buffer = image::RgbaImage::from_raw(w, h, bgra).expect("texture size matches its pixels");
            (Arc::new(RenderImage::new([image::Frame::new(buffer)])), false)
        });
        entry.1 = true;
        entry.0.clone()
    }

    fn end(&mut self, window: &mut Window) {
        let stale: Vec<u64> = self.map.iter().filter(|(_, (_, used))| !used).map(|(k, _)| *k).collect();
        for key in stale {
            if let Some((image, _)) = self.map.remove(&key) {
                let _ = window.drop_image(image);
            }
        }
    }
}

fn key_of(value: impl Hash) -> u64 {
    let mut h = DefaultHasher::new();
    value.hash(&mut h);
    h.finish()
}

/// `dw`×`dh` device pixels placed at `bounds`' origin, snapped to the
/// device grid so texels land 1:1 on screen pixels.
fn snapped(bounds: Bounds<Pixels>, dw: u32, dh: u32, sf: f32) -> Bounds<Pixels> {
    let origin = point(px((f32::from(bounds.origin.x) * sf).round() / sf), px((f32::from(bounds.origin.y) * sf).round() / sf));
    Bounds::new(origin, size(px(dw as f32 / sf), px(dh as f32 / sf)))
}

/// An element showing a texture pixel for pixel.
fn texture(image: Arc<RenderImage>, dw: u32, dh: u32, sf: f32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds: Bounds<Pixels>, _, window: &mut Window, _| {
            let target = snapped(bounds, dw, dh, window.scale_factor());
            let _ = window.paint_image(target, target, Corners::default(), image, 0, false);
        },
    )
    .w(px(dw as f32 / sf))
    .h(px(dh as f32 / sf))
}

/// Before left of `split` (0..1), after right of it, a rule between.
fn compare(before: Arc<RenderImage>, after: Arc<RenderImage>, dw: u32, dh: u32, sf: f32, split: f32, rule: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds: Bounds<Pixels>, _, window: &mut Window, _| {
            let sf = window.scale_factor();
            let target = snapped(bounds, dw, dh, sf);
            // Split on a device pixel, so neither side is resampled.
            let cut = (dw as f32 * split).round() / sf;
            let left = Bounds::new(target.origin, size(px(cut), target.size.height));
            let right = Bounds::new(point(target.origin.x + px(cut), target.origin.y), size(target.size.width - px(cut), target.size.height));
            window.with_content_mask(Some(ContentMask { bounds: left }), |window| {
                let _ = window.paint_image(target, target, Corners::default(), before, 0, false);
            });
            window.with_content_mask(Some(ContentMask { bounds: right }), |window| {
                let _ = window.paint_image(target, target, Corners::default(), after, 0, false);
            });
            let line = Bounds::new(point(target.origin.x + px(cut), target.origin.y), size(px(2. / sf), target.size.height));
            window.paint_quad(fill(line, rule));
        },
    )
    .w(px(dw as f32 / sf))
    .h(px(dh as f32 / sf))
}

/// What's on the easel: a still, or an animation's frames.
struct Photo {
    name: String,
    /// Where it came from (for the export dialog's folder).
    path: Option<PathBuf>,
    clip: Arc<Clip>,
    /// The frame on show.
    frame: usize,
}

impl Photo {
    fn sample() -> Photo {
        Photo { name: "sample".into(), path: None, clip: Arc::new(Clip::still(studio::sample())), frame: 0 }
    }

    fn print(&self) -> &Print {
        &self.clip.frames[self.frame.min(self.clip.frames.len() - 1)].print
    }

    fn frames(&self) -> usize {
        self.clip.frames.len()
    }
}

/// What an animation export makes.
#[derive(Clone, Copy)]
enum AnimOut {
    Gif,
    Apng,
    Sheet,
}

/// Everything a dither depends on, compared bit for bit to skip redevelops.
#[derive(Clone, PartialEq, Hash)]
struct DevKey {
    frame: usize,
    cols: u32,
    adjust: [u32; 3],
    invert: bool,
    algo: Algo,
    strength: u32,
    bias: u32,
    seed: u32,
    serpentine: bool,
    space: Space,
    palette: Vec<[u32; 3]>,
}

/// The developed art and what it was developed with.
struct Developed {
    key: DevKey,
    art: Rc<Art>,
}

/// A contact-sheet tile: its label, its art, and whether it's the current one.
struct Tile {
    label: &'static str,
    art: Rc<Art>,
    current: bool,
}

type Typeset = ((usize, [u32; 3], bool, bool, Charset, Fit), Rc<Vec<String>>);

struct Darkroom {
    settings: Settings,
    photo: Photo,
    loading: bool,
    /// Bumped per photo, to replay the develop effect.
    roll: u64,
    developed: Option<Developed>,
    tex: Textures,
    view: View,
    /// Compare's split, 0..1 across the print.
    split: f32,
    sheet: SheetKind,
    /// The contact sheet's tiles and the inputs they were developed from.
    tiles: Option<(u64, Rc<Vec<Tile>>)>,
    undo: Vec<Recipe>,
    redo: Vec<Recipe>,
    last_edit: Option<Instant>,
    /// An animation's frames, all developed (with stability), and the
    /// inputs they came from; developed in the background.
    reel: Option<(u64, Arc<Vec<Art>>)>,
    /// The inputs of the reel being developed now.
    reel_job: Option<u64>,
    playing: bool,
    /// Bumped to stop the running playback loop.
    play_gen: u64,
    typeset: Option<Typeset>,
    palette: Entity<CommandPalette>,
    toaster: Entity<Toaster>,
    _appearance: Subscription,
}

impl Darkroom {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut room = Self {
            settings: Settings::load(),
            photo: Photo::sample(),
            loading: false,
            roll: 0,
            developed: None,
            tex: Textures::default(),
            view: View::Print,
            split: 0.5,
            sheet: SheetKind::Algorithms,
            tiles: None,
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: None,
            reel: None,
            reel_job: None,
            playing: false,
            play_gen: 0,
            typeset: None,
            palette: cx.new(|cx| CommandPalette::new(window, cx)),
            toaster: cx.new(|_| Toaster::new()),
            _appearance: theme::follow_system(window),
        };
        room.apply_look(window, cx);
        // Lodestone hands over its shared look as FERRITE_* variables; when
        // launched that way, those win over the saved settings.
        theme::apply_env(cx);
        room.set_commands(cx);
        if let Some(path) = std::env::args().skip(1).find(|a| !a.starts_with("--")) {
            room.open_path(PathBuf::from(path), cx);
        }
        room
    }

    // ── Settings ─────────────────────────────────────────────────────────

    fn apply_look(&self, window: &mut Window, cx: &mut App) {
        let s = &self.settings;
        if let Some(scheme) = schemes::by_key(&s.scheme) {
            theme::set_scheme(scheme, cx);
        }
        let appearance = match s.appearance.as_str() {
            "light" => Appearance::Light,
            "system" => Appearance::System,
            _ => Appearance::Dark,
        };
        theme::set_appearance(appearance, window, cx);
        motion::set_fps(s.fps);
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if let Err(err) = self.settings.save() {
            self.toast(toast("Couldn't save settings").danger().message(err), cx);
        }
        cx.notify();
    }

    fn change(&mut self, f: impl FnOnce(&mut Settings), window: &mut Window, cx: &mut Context<Self>) {
        let before = self.settings.recipe.clone();
        f(&mut self.settings);
        self.record(before);
        self.apply_look(window, cx);
        self.save(cx);
    }

    /// Change only the recipe: no look to reapply, so no window needed
    /// (async results land here).
    fn set_recipe(&mut self, f: impl FnOnce(&mut Recipe), cx: &mut Context<Self>) {
        let before = self.settings.recipe.clone();
        f(&mut self.settings.recipe);
        self.record(before);
        self.save(cx);
    }

    // ── Undo ─────────────────────────────────────────────────────────────

    /// Remember the recipe as it was before an edit. A burst of edits (a
    /// slider drag) keeps only its first "before", so it undoes in one go.
    fn record(&mut self, before: Recipe) {
        if before == self.settings.recipe {
            return;
        }
        let now = Instant::now();
        let merging = self.last_edit.is_some_and(|t| now.duration_since(t) < UNDO_MERGE);
        if !merging {
            self.undo.push(before);
            if self.undo.len() > UNDO_DEPTH {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.last_edit = Some(now);
    }

    fn undo(&mut self, cx: &mut Context<Self>) {
        if let Some(r) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.settings.recipe, r));
            self.last_edit = None;
            self.save(cx);
        }
    }

    fn redo(&mut self, cx: &mut Context<Self>) {
        if let Some(r) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.settings.recipe, r));
            self.last_edit = None;
            self.save(cx);
        }
    }

    fn set_view(&mut self, view: View, cx: &mut Context<Self>) {
        self.view = view;
        cx.notify();
    }

    fn toast(&self, t: Toast, cx: &mut Context<Self>) {
        self.toaster.update(cx, |toaster, cx| toaster.push(t, cx));
    }

    fn set_commands(&self, cx: &mut Context<Self>) {
        let weak = cx.weak_entity();
        let run = |f: fn(&mut Self, &mut Window, &mut Context<Self>)| {
            let weak = weak.clone();
            move |window: &mut Window, cx: &mut App| {
                let _ = weak.update(cx, |this, cx| f(this, window, cx));
            }
        };
        let mut commands = vec![
            command("Open photo…").group("File").icon(Icon::Folder).shortcut("Ctrl+O").on_run(run(|this, _, cx| this.open(cx))),
            command("Open recipe…").group("Recipe").icon(Icon::Folder).shortcut("Ctrl+Shift+O").on_run(run(|this, _, cx| this.open_recipe(cx))),
            command("Save recipe…").group("Recipe").icon(Icon::File).shortcut("Ctrl+S").on_run(run(|this, _, cx| this.save_recipe(cx))),
            command("Import palette…").group("Recipe").icon(Icon::Folder).on_run(run(|this, _, cx| this.import_palette(cx))),
            command("Batch develop a folder…").group("File").icon(Icon::Folder).on_run(run(|this, _, cx| this.batch(cx))),
            command("Paste palette").group("Recipe").icon(Icon::Copy).on_run(run(|this, _, cx| this.paste_palette(cx))),
            command("New random seed").group("Develop").icon(Icon::Refresh).on_run(run(|this, _, cx| this.reseed(cx))),
            command("Undo").group("Edit").shortcut("Ctrl+Z").on_run(run(|this, _, cx| this.undo(cx))),
            command("Redo").group("Edit").shortcut("Ctrl+Shift+Z").on_run(run(|this, _, cx| this.redo(cx))),
            command("View: print").group("View").on_run(run(|this, _, cx| this.set_view(View::Print, cx))),
            command("View: compare with the photo").group("View").on_run(run(|this, _, cx| this.set_view(View::Compare, cx))),
            command("View: contact sheet of algorithms").group("View").on_run(run(|this, _, cx| {
                this.sheet = SheetKind::Algorithms;
                this.set_view(View::Sheet, cx)
            })),
            command("View: contact sheet of palettes").group("View").on_run(run(|this, _, cx| {
                this.sheet = SheetKind::Palettes;
                this.set_view(View::Sheet, cx)
            })),
            command("Export PNG…").group("File").icon(Icon::File).shortcut("Ctrl+E").on_run(run(|this, window, cx| this.export_png(window, cx))),
            command("Export text…").group("File").icon(Icon::File).shortcut("Ctrl+Shift+E").on_run(run(|this, _, cx| this.export_text(cx))),
            command("Copy ASCII").group("File").icon(Icon::Copy).shortcut("Ctrl+Shift+C").on_run(run(|this, _, cx| this.copy_text(cx))),
            command("Back to the sample").group("File").icon(Icon::Refresh).on_run(run(|this, _, cx| {
                this.photo = Photo::sample();
                this.new_roll();
                cx.notify();
            })),
            command("Dither").group("Develop").on_run(run(|this, window, cx| this.change(|s| s.recipe.mode = Mode::Dither, window, cx))),
            command("ASCII").group("Develop").on_run(run(|this, window, cx| this.change(|s| s.recipe.mode = Mode::Ascii, window, cx))),
            command("Invert").group("Develop").shortcut("Ctrl+I").on_run(run(|this, window, cx| this.change(|s| s.recipe.invert = !s.recipe.invert, window, cx))),
            command("Reset tone").group("Develop").on_run(run(|this, window, cx| {
                this.change(
                    |s| {
                        s.recipe.brightness = 0.;
                        s.recipe.contrast = 1.;
                        s.recipe.gamma = 1.;
                        s.recipe.invert = false;
                    },
                    window,
                    cx,
                )
            })),
            command("Stepped motion (25 fps)").group("Theme").on_run(run(|this, window, cx| this.change(|s| s.fps = 25, window, cx))),
            command("Smooth motion (240 fps)").group("Theme").on_run(run(|this, window, cx| this.change(|s| s.fps = 240, window, cx))),
            command("Dark theme").group("Theme").on_run(run(|this, window, cx| this.change(|s| s.appearance = "dark".into(), window, cx))),
            command("Light theme").group("Theme").on_run(run(|this, window, cx| this.change(|s| s.appearance = "light".into(), window, cx))),
        ];
        for algo in Algo::ALL {
            let weak = weak.clone();
            commands.push(command(format!("Algorithm: {}", algo.name())).group("Develop").on_run(move |window, cx| {
                let _ = weak.update(cx, |this, cx| {
                    this.change(
                        |s| {
                            s.recipe.mode = Mode::Dither;
                            s.recipe.algo = algo;
                        },
                        window,
                        cx,
                    )
                });
            }));
        }
        for preset in palettes::PRESETS {
            let weak = weak.clone();
            commands.push(command(format!("Palette: {}", preset.name)).group("Develop").on_run(move |window, cx| {
                let _ = weak.update(cx, |this, cx| {
                    this.change(
                        |s| {
                            s.recipe.mode = Mode::Dither;
                            s.recipe.palette = preset.key.into();
                        },
                        window,
                        cx,
                    )
                });
            }));
        }
        for charset in Charset::ALL {
            let weak = weak.clone();
            commands.push(command(format!("Characters: {}", charset.name())).group("Develop").on_run(move |window, cx| {
                let _ = weak.update(cx, |this, cx| {
                    this.change(
                        |s| {
                            s.recipe.mode = Mode::Ascii;
                            s.recipe.charset = charset;
                            s.recipe.fit = charset.default_fit();
                        },
                        window,
                        cx,
                    )
                });
            }));
        }
        for scheme in SCHEMES {
            let weak = weak.clone();
            commands.push(command(format!("Scheme: {}", scheme.name)).group("Theme").on_run(move |window, cx| {
                let _ = weak.update(cx, |this, cx| this.change(|s| s.scheme = scheme.key.into(), window, cx));
            }));
        }
        self.palette.update(cx, |p, cx| p.set_commands(commands, cx));
    }

    // ── Photos in ────────────────────────────────────────────────────────

    /// Ask for one file, then hand it to `then`.
    fn ask_path(&self, prompt: &str, then: fn(&mut Self, PathBuf, &mut Context<Self>), cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some(prompt.to_string().into()) });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |this, cx| then(this, path, cx));
            }
        })
        .detach();
    }

    fn open(&mut self, cx: &mut Context<Self>) {
        self.ask_path("Develop", Self::open_path, cx);
    }

    fn open_recipe(&mut self, cx: &mut Context<Self>) {
        self.ask_path("Open recipe", Self::open_path, cx);
    }

    fn import_palette(&mut self, cx: &mut Context<Self>) {
        self.ask_path("Import palette", Self::import_palette_path, cx);
    }

    /// Whatever arrived (dropped, opened, or on the command line): a recipe,
    /// a palette file, or a photo, by its extension.
    fn open_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        match ext.as_str() {
            "toml" => self.open_recipe_path(path, cx),
            "gpl" | "pal" | "hex" | "txt" => self.import_palette_path(path, cx),
            _ => self.open_photo(path, cx),
        }
    }

    fn open_recipe_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let result = std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|src| Recipe::from_toml(&src));
        match result {
            Ok(recipe) => {
                self.set_recipe(|r| *r = recipe, cx);
                let name = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.toast(toast("Recipe applied").success().message(name), cx);
            }
            Err(why) => self.toast(toast("Couldn't open that recipe").danger().message(why), cx),
        }
    }

    fn save_recipe(&mut self, cx: &mut Context<Self>) {
        let r = &self.settings.recipe;
        let name = format!("{}-{}-{}.toml", self.stem(), r.algo.key(), r.palette);
        self.save_as(name, r.to_toml().into_bytes(), "Recipe", cx);
    }

    fn use_palette(&mut self, colors: Result<Vec<Rgb>, String>, cx: &mut Context<Self>) {
        match colors {
            Ok(colors) => {
                let n = colors.len();
                self.set_recipe(
                    |r| {
                        r.mode = Mode::Dither;
                        r.palette = palettes::CUSTOM.into();
                        r.colors = colors;
                    },
                    cx,
                );
                self.toast(toast("Palette imported").success().message(format!("{n} colours")), cx);
            }
            Err(why) => self.toast(toast("Couldn't use that palette").danger().message(why), cx),
        }
    }

    fn import_palette_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let colors = palettes::load(&path);
        self.use_palette(colors, cx);
    }

    fn paste_palette(&mut self, cx: &mut Context<Self>) {
        let text = cx.read_from_clipboard().and_then(|item| item.text()).unwrap_or_default();
        self.use_palette(palettes::parse(&text), cx);
    }

    fn reseed(&mut self, cx: &mut Context<Self>) {
        self.set_recipe(
            |r| {
                r.seed = r.seed.wrapping_mul(1_103_515_245).wrapping_add(12_345) % 1_000_000;
                r.algo = Algo::Random;
            },
            cx,
        );
    }

    fn open_photo(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.loading = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let p = path.clone();
            let result = cx.background_executor().spawn(async move { sequence::load(&p) }).await;
            let _ = this.update(cx, |this, cx| {
                this.loading = false;
                match result {
                    Ok(clip) => {
                        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "photo".into());
                        if clip.truncated {
                            this.toast(toast("Long animation").warning().message(format!("Only the first {} frames were loaded", clip.frames.len())), cx);
                        }
                        let animated = clip.is_animated();
                        this.photo = Photo { name, path: Some(path), clip: Arc::new(clip), frame: 0 };
                        this.new_roll();
                        if animated {
                            this.play(cx);
                        } else {
                            this.playing = false;
                        }
                    }
                    Err(why) => this.toast(toast("Couldn't open that").danger().message(why), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    // ── Developing ───────────────────────────────────────────────────────

    /// A new photo on the easel: develop it from scratch, with the effect.
    fn new_roll(&mut self) {
        self.roll += 1;
        self.developed = None;
        self.typeset = None;
        self.reel = None;
        self.tiles = None;
    }

    // ── Animation ────────────────────────────────────────────────────────

    /// Everything the whole reel depends on.
    fn reel_key(&self, cx: &App) -> u64 {
        let r = &self.settings.recipe;
        let colors: Vec<[u32; 3]> = self.palette_colors(cx).iter().map(|c| c.map(f32::to_bits)).collect();
        key_of((self.roll, Self::adjust_key(self.adjust(cx)), r.cols, r.algo, r.strength.to_bits(), r.bias.to_bits(), r.seed, r.serpentine, r.space, r.stability.to_bits(), colors))
    }

    /// The developed reel, if it's current; otherwise start developing it
    /// (one job at a time: a slider drag doesn't queue a job per step).
    fn reel(&mut self, cx: &mut Context<Self>) -> Option<Arc<Vec<Art>>> {
        let key = self.reel_key(cx);
        if let Some((k, arts)) = &self.reel
            && *k == key
        {
            return Some(arts.clone());
        }
        if self.reel_job.is_none() {
            self.reel_job = Some(key);
            let clip = self.photo.clip.clone();
            let (a, colors, r) = (self.adjust(cx), self.palette_colors(cx), self.settings.recipe.clone());
            cx.spawn(async move |this, cx| {
                let arts = cx
                    .background_executor()
                    .spawn(async move { sequence::develop_all(&clip.frames, r.cols, a, &colors, r.params(), r.stability) })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    this.reel_job = None;
                    // A stale reel is dropped; the next frame starts a fresh one.
                    if this.reel_key(cx) == key {
                        this.reel = Some((key, Arc::new(arts)));
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        None
    }

    /// The current frame's delay, after the speed setting.
    fn frame_delay(&self) -> u64 {
        let ms = self.photo.clip.frames[self.photo.frame].delay_ms as f32 / self.settings.recipe.speed.max(0.05);
        ms.round().max(10.) as u64
    }

    fn play(&mut self, cx: &mut Context<Self>) {
        self.playing = true;
        self.play_gen += 1;
        let generation = self.play_gen;
        cx.spawn(async move |this, cx| {
            loop {
                let Ok(Some(delay)) = this.update(cx, |this, _| (this.playing && this.play_gen == generation).then(|| this.frame_delay())) else { break };
                cx.background_executor().timer(Duration::from_millis(delay)).await;
                let still_playing = this.update(cx, |this, cx| {
                    if !this.playing || this.play_gen != generation {
                        return false;
                    }
                    this.photo.frame = (this.photo.frame + 1) % this.photo.frames();
                    cx.notify();
                    true
                });
                if !matches!(still_playing, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn pause(&mut self, cx: &mut Context<Self>) {
        self.playing = false;
        cx.notify();
    }

    fn toggle_play(&mut self, cx: &mut Context<Self>) {
        if self.photo.frames() < 2 {
            return;
        }
        if self.playing { self.pause(cx) } else { self.play(cx) }
    }

    fn step(&mut self, by: isize, cx: &mut Context<Self>) {
        let n = self.photo.frames() as isize;
        if n < 2 {
            return;
        }
        self.playing = false;
        self.photo.frame = (self.photo.frame as isize + by).rem_euclid(n) as usize;
        cx.notify();
    }

    /// The art for the frame on show: from the reel when it's ready (with
    /// stability), else this frame alone.
    fn current_art(&mut self, cx: &mut Context<Self>) -> (Rc<Art>, u64) {
        if self.photo.frames() > 1
            && let Some(reel) = self.reel(cx)
        {
            let frame = self.photo.frame.min(reel.len() - 1);
            let key = key_of(("reel", self.reel.as_ref().map(|r| r.0), frame));
            return (Rc::new(reel[frame].clone()), key);
        }
        let art = self.developed(cx);
        (art, self.developed.as_ref().map(|d| key_of(&d.key)).unwrap_or_default())
    }

    fn export_anim(&mut self, what: AnimOut, cx: &mut Context<Self>) {
        let key = self.reel_key(cx);
        let ready = self.reel.as_ref().filter(|(k, _)| *k == key).map(|(_, arts)| arts.clone());
        let clip = self.photo.clip.clone();
        let (a, colors, r) = (self.adjust(cx), self.palette_colors(cx), self.settings.recipe.clone());
        let (ext, label): (&str, &'static str) = match what {
            AnimOut::Gif => ("gif", "GIF"),
            AnimOut::Apng => ("png", "APNG"),
            AnimOut::Sheet => ("png", "Sprite sheet"),
        };
        let suffix = if matches!(what, AnimOut::Sheet) { "-sheet" } else { "" };
        let name = format!("{}-{}-{}{suffix}.{ext}", self.stem(), r.algo.key(), r.palette);
        self.toast(toast(format!("Making the {label}…")).message(format!("{} frames", clip.frames.len())), cx);
        cx.spawn(async move |this, cx| {
            let bytes = cx
                .background_executor()
                .spawn(async move {
                    let arts = match ready {
                        Some(arts) => arts,
                        None => Arc::new(sequence::develop_all(&clip.frames, r.cols, a, &colors, r.params(), r.stability)),
                    };
                    let delays = sequence::delays(&clip.frames, r.speed);
                    match what {
                        AnimOut::Gif => sequence::gif(&arts, &delays, r.scale, r.look()),
                        AnimOut::Apng => sequence::apng(&arts, &delays, r.scale, r.look()),
                        AnimOut::Sheet => sequence::sprite_sheet(&arts, r.scale, r.look()),
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| match bytes {
                Ok(bytes) => this.save_as(name, bytes, label, cx),
                Err(why) => this.toast(toast(format!("Couldn't make the {label}")).danger().message(why), cx),
            });
        })
        .detach();
    }

    /// Play/pause, the frame scrubber, and where we are.
    fn timeline(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let n = self.photo.frames();
        let frame = self.photo.frame;
        let p = palette(cx);
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .child(
                Button::new("play")
                    .label(if self.playing { "Pause" } else { "Play" })
                    .secondary()
                    .small()
                    .shortcut("Space")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_play(cx))),
            )
            .child(
                slider("frame-slider")
                    .range(0., (n - 1) as f32)
                    .step(1.)
                    .value(frame as f32)
                    .width(px(260.))
                    .format(move |v| format!("{}/{n}", v as usize + 1).into())
                    .on_change(cx.listener(|this, v: &f32, _, cx| {
                        this.playing = false;
                        this.photo.frame = (*v as usize).min(this.photo.frames() - 1);
                        cx.notify();
                    })),
            )
            .child(div().body(text::SM).text_color(hsla(p.fg_dim)).child(format!("{} ms", self.frame_delay())))
            .when(self.reel_job.is_some(), |el| el.child(spinner("reel-spinner")).child(div().body(text::SM).text_color(hsla(p.fg_dim)).child("developing every frame")))
    }

    fn adjust(&self, cx: &App) -> Adjust {
        let p = palette(cx);
        let s = &self.settings;
        let ink = hsla(if s.recipe.accent_ink { p.accent } else { p.fg });
        Adjust { brightness: s.recipe.brightness, contrast: s.recipe.contrast, gamma: s.recipe.gamma, invert: s.recipe.invert, light_ink: ink.l > hsla(p.bg).l }
    }

    fn adjust_key(a: Adjust) -> ([u32; 3], bool, bool) {
        ([a.brightness.to_bits(), a.contrast.to_bits(), a.gamma.to_bits()], a.invert, a.light_ink)
    }

    /// The scheme's paper and ink, as the `Scheme` palette uses them.
    fn scheme_inks(&self, cx: &App) -> (Rgb, Rgb) {
        let p = palette(cx);
        let rgb = |c: gpui::Hsla| {
            let c = Rgba::from(c);
            [c.r, c.g, c.b]
        };
        (rgb(hsla(p.bg)), rgb(hsla(if self.settings.recipe.accent_ink { p.accent } else { p.fg })))
    }

    fn preset(&self) -> &'static palettes::Preset {
        palettes::by_key(&self.settings.recipe.palette).unwrap_or(&palettes::PRESETS[0])
    }

    /// The custom palette, when it's chosen and has its colours.
    fn custom(&self) -> Option<&[Rgb]> {
        let r = &self.settings.recipe;
        r.uses_custom().then_some(r.colors.as_slice())
    }

    fn palette_name(&self) -> &'static str {
        if self.custom().is_some() { "Custom" } else { self.preset().name }
    }

    fn palette_colors(&self, cx: &App) -> Vec<Rgb> {
        let (paper, ink) = self.scheme_inks(cx);
        self.settings.recipe.palette_colors(paper, ink)
    }

    /// Develop every picture in a folder with the current recipe, into a
    /// `darkroom` folder beside them.
    fn batch(&mut self, cx: &mut Context<Self>) {
        let dirs = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some("Develop folder".into()) });
        let (paper, ink) = self.scheme_inks(cx);
        let job = batch::Job { recipe: self.settings.recipe.clone(), paper, ink };
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(dirs))) = dirs.await else { return };
            let Some(dir) = dirs.into_iter().next() else { return };
            let out = dir.join("darkroom");
            let _ = this.update(cx, |this, cx| this.toast(toast("Developing the folder…").message(dir.display().to_string()), cx));
            let target = out.clone();
            let result = cx.background_executor().spawn(async move { job.develop_folder(&dir, &target) }).await;
            let _ = this.update(cx, |this, cx| {
                let t = match result {
                    Ok((done, failed)) if failed.is_empty() => toast(format!("{} developed", done.len())).success().message(out.display().to_string()),
                    Ok((done, failed)) => toast(format!("{} developed, {} failed", done.len(), failed.len()))
                        .warning()
                        .message(failed.iter().map(|(p, why)| format!("{}: {why}", p.file_name().unwrap_or_default().to_string_lossy())).collect::<Vec<_>>().join("\n")),
                    Err(why) => toast("Couldn't develop the folder").danger().message(why),
                };
                this.toast(t, cx);
            });
        })
        .detach();
    }

    /// The dither, developed again only when something changed.
    fn developed(&mut self, cx: &App) -> Rc<Art> {
        let a = self.adjust(cx);
        let s = &self.settings;
        let colors = self.palette_colors(cx);
        let key = DevKey {
            frame: self.photo.frame,
            cols: s.recipe.cols,
            adjust: Self::adjust_key(a).0,
            invert: a.invert,
            algo: s.recipe.algo,
            strength: s.recipe.strength.to_bits(),
            bias: s.recipe.bias.to_bits(),
            seed: s.recipe.seed,
            serpentine: s.recipe.serpentine,
            space: s.recipe.space,
            palette: colors.iter().map(|c| c.map(f32::to_bits)).collect(),
        };
        if let Some(d) = &self.developed
            && d.key == key
        {
            return d.art.clone();
        }
        let params = s.recipe.params();
        let art = Rc::new(studio::develop(self.photo.print(), s.recipe.cols, a, colors, params));
        self.developed = Some(Developed { key, art: art.clone() });
        art
    }

    /// The contact sheet's tiles: the print through every algorithm (in the
    /// current palette) or every palette (with the current algorithm).
    fn tiles(&mut self, cx: &App) -> Rc<Vec<Tile>> {
        let a = self.adjust(cx);
        let r = self.settings.recipe.clone();
        let (paper, ink) = self.scheme_inks(cx);
        let current = self.palette_colors(cx);
        let custom = self.custom().is_some();
        let mut choices: Vec<(&'static str, Vec<Rgb>, bool)> =
            palettes::PRESETS.iter().map(|p| (p.name, p.colors(paper, ink), !custom && p.key == r.palette)).collect();
        if r.colors.len() >= 2 {
            choices.push(("Custom", r.colors.clone(), custom));
        }
        let bits = |c: &[Rgb]| c.iter().map(|c| c.map(f32::to_bits)).collect::<Vec<_>>();
        let inputs = key_of((self.sheet, self.roll, Self::adjust_key(a), r.flat_lines(), bits(&current), choices.iter().map(|c| bits(&c.1)).collect::<Vec<_>>()));
        if let Some((k, tiles)) = &self.tiles
            && *k == inputs
        {
            return tiles.clone();
        }
        let print = self.photo.print();
        let tiles: Vec<Tile> = match self.sheet {
            SheetKind::Algorithms => Algo::ALL
                .iter()
                .map(|&algo| Tile {
                    label: algo.name(),
                    art: Rc::new(studio::develop(print, THUMB_COLS, a, current.clone(), engine::Params { algo, ..r.params() })),
                    current: algo == r.algo,
                })
                .collect(),
            SheetKind::Palettes => choices
                .into_iter()
                .map(|(label, colors, current)| Tile { label, art: Rc::new(studio::develop(print, THUMB_COLS, a, colors, r.params())), current })
                .collect(),
        };
        let tiles = Rc::new(tiles);
        self.tiles = Some((inputs, tiles.clone()));
        tiles
    }

    fn typeset(&mut self, cx: &App) -> Rc<Vec<String>> {
        let a = self.adjust(cx);
        let (b, inv, light) = Self::adjust_key(a);
        let s = &self.settings;
        let key = (s.recipe.ascii_cols as usize, b, inv, light, s.recipe.charset, s.recipe.fit);
        match &self.typeset {
            Some((k, lines)) if *k == key => lines.clone(),
            _ => {
                let lines = studio::ascii_lines(self.photo.print(), key.0, a, s.recipe.charset, s.recipe.fit);
                self.typeset = Some((key, lines.clone()));
                lines
            }
        }
    }

    // ── Exports ──────────────────────────────────────────────────────────

    fn stem(&self) -> String {
        Path::new(&self.photo.name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "darkroom".into())
    }

    fn export_dir(&self) -> PathBuf {
        self.photo
            .path
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// Ask where, then write `bytes` there.
    fn save_as(&self, name: String, bytes: Vec<u8>, what: &'static str, cx: &mut Context<Self>) {
        let target = cx.prompt_for_new_path(&self.export_dir(), Some(&name));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = target.await else { return };
            let result = std::fs::write(&path, bytes);
            let _ = this.update(cx, |this, cx| {
                let t = match result {
                    Ok(()) => toast(format!("{what} saved")).success().message(path.display().to_string()),
                    Err(e) => toast(format!("Couldn't save the {}", what.to_lowercase())).danger().message(e.to_string()),
                };
                this.toast(t, cx);
            });
        })
        .detach();
    }

    fn export_png(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let (art, _) = self.current_art(cx);
        match render::png(&art, self.settings.recipe.scale, self.settings.recipe.look()) {
            Ok(bytes) => {
                let name = format!("{}-{}-{}.png", self.stem(), self.settings.recipe.algo.key(), self.settings.recipe.palette);
                self.save_as(name, bytes, "PNG", cx);
            }
            Err(why) => self.toast(toast("Couldn't make the PNG").danger().message(why), cx),
        }
    }

    fn export_text(&mut self, cx: &mut Context<Self>) {
        let text = studio::text(&self.typeset(cx));
        self.save_as(format!("{}.txt", self.stem()), text.into_bytes(), "Text", cx);
    }

    fn copy_text(&mut self, cx: &mut Context<Self>) {
        let text = studio::text(&self.typeset(cx));
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.toast(toast("ASCII copied").success(), cx);
    }

    // ── Views ────────────────────────────────────────────────────────────

    fn preview(&mut self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let p = palette(cx);
        let ink = hsla(if self.settings.recipe.accent_ink { p.accent } else { p.fg });
        // The room the print has: the window less the sidebar, chrome and padding.
        let view = window.viewport_size();
        let (room_w, room_h) = (f32::from(view.width) - SIDEBAR - 72., f32::from(view.height) - 200.);
        match self.settings.recipe.mode {
            Mode::Dither if self.view == View::Sheet => self.sheet_view(room_w, room_h, window, cx).into_any_element(),
            Mode::Dither => {
                let (art, dev) = self.current_art(cx);
                let (w, h) = (art.w, art.h);
                // Whole device pixels per art pixel, so the preview is the export.
                let sf = window.scale_factor();
                let room_h = if self.photo.frames() > 1 { room_h - 44. } else { room_h };
                let cell = ((room_w / w as f32).min(room_h / h as f32) * sf).floor().max(1.) as u32;
                let look = self.settings.recipe.look();
                let after = self.tex.get(key_of(("after", dev, cell, look.bits())), || render::bgra(&art, cell, look));
                let (dw, dh) = (w * cell, h * cell);
                if self.view == View::Compare {
                    let print = self.photo.print();
                    let before = self.tex.get(key_of(("before", self.roll, self.photo.frame, w, h, cell)), || render::before_bgra(print, w, h, cell));
                    return compare(before, after, dw, dh, sf, self.split, hsla(p.accent)).into_any_element();
                }
                develop(("print", self.roll), self.roll, texture(after, dw, dh, sf)).into_any_element()
            }
            Mode::Ascii => {
                let lines = self.typeset(cx);
                div()
                    .id("ascii-scroll")
                    .max_w(px(room_w))
                    .max_h(px(room_h))
                    .overflow_scroll()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_none()
                            .text_color(ink)
                            .children(lines.iter().map(|l| div().display(Scale::X1, window).whitespace_nowrap().child(l.clone()))),
                    )
                    .into_any_element()
            }
        }
    }

    /// A grid of thumbnails, one per algorithm or palette. Click one to use it.
    fn sheet_view(&mut self, room_w: f32, room_h: f32, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let p = palette(cx);
        let sf = window.scale_factor();
        let tiles = self.tiles(cx);
        let kind = self.sheet;
        // As many columns as fit, each tile stretched to share the width.
        let per_row = ((room_w + TILE_GAP) / (TILE_MIN + TILE_FRAME + TILE_GAP)).floor().max(1.);
        let tile_w = (room_w - TILE_GAP * (per_row - 1.)) / per_row - TILE_FRAME - 1.;
        let items: Vec<_> = tiles
            .iter()
            .enumerate()
            .map(|(i, tile)| {
                let art = tile.art.clone();
                let cell = ((tile_w * sf) / art.w as f32).floor().max(1.) as u32;
                let image = self.tex.get(key_of(("tile", self.tiles.as_ref().map(|t| t.0), i, cell)), || render::bgra(&art, cell, render::Look::default()));
                let border = if tile.current { p.accent } else { p.line };
                div()
                    .id(("tile", i))
                    .w(px(tile_w + TILE_FRAME))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .p_1()
                    .border_1()
                    .border_color(hsla(border))
                    .hover(|style| style.bg(hsla(p.raised)))
                    .cursor_pointer()
                    .child(texture(image, art.w * cell, art.h * cell, sf))
                    .child(div().body(text::SM).text_color(hsla(if tile.current { p.accent } else { p.fg_dim })).child(tile.label))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.set_recipe(
                            |r| match kind {
                                SheetKind::Algorithms => r.algo = Algo::ALL[i],
                                SheetKind::Palettes => match palettes::PRESETS.get(i) {
                                    Some(preset) => r.palette = preset.key.into(),
                                    None => r.palette = palettes::CUSTOM.into(),
                                },
                            },
                            cx,
                        );
                        this.set_view(View::Print, cx);
                    }))
            })
            .collect();
        div()
            .id("sheet-scroll")
            .w(px(room_w))
            .h(px(room_h))
            .overflow_y_scroll()
            .child(div().flex().flex_row().flex_wrap().gap(px(TILE_GAP)).children(items))
    }

    /// The row over the print: which view, and that view's controls.
    fn view_bar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let views = [View::Print, View::Compare, View::Sheet];
        let mut bar = div().flex().flex_row().items_center().gap_4().child(
            segmented("view-seg")
                .option("Print")
                .option("Compare")
                .option("Contact sheet")
                .selected(views.iter().position(|v| *v == self.view).unwrap_or(0))
                .on_select(cx.listener(move |this, i: &usize, _, cx| this.set_view(views[*i], cx))),
        );
        match self.view {
            View::Compare => {
                bar = bar.child(
                    slider("split-slider")
                        .range(0., 1.)
                        .step(0.01)
                        .value(self.split)
                        .width(px(200.))
                        .format(|v| format!("photo {:.0}%", v * 100.).into())
                        .on_change(cx.listener(|this, v: &f32, _, cx| {
                            this.split = *v;
                            cx.notify();
                        })),
                )
            }
            View::Sheet => {
                bar = bar.child(
                    segmented("sheet-seg")
                        .option("Algorithms")
                        .option("Palettes")
                        .selected(if self.sheet == SheetKind::Palettes { 1 } else { 0 })
                        .on_select(cx.listener(|this, i: &usize, _, cx| {
                            this.sheet = if *i == 1 { SheetKind::Palettes } else { SheetKind::Algorithms };
                            cx.notify();
                        })),
                )
            }
            View::Print => {}
        }
        bar
    }

    /// How each pixel is drawn.
    fn render_controls(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let r = &self.settings.recipe;
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                field("shape", "Shape")
                    .hint("Shapes show when a pixel is 3 px or more: in the preview, and in 4× and 8× PNGs")
                    .stacked()
                    .child(
                        select("shape-select")
                            .options(Shape::ALL.iter().map(|sh| sh.name()))
                            .selected(Shape::ALL.iter().position(|sh| *sh == r.shape))
                            .width(px(220.))
                            .on_change(cx.listener(|this, i: &usize, _, cx| this.set_recipe(|r| r.shape = Shape::ALL[*i], cx))),
                    ),
            )
            .child(field("gutter", "Gutter").stacked().child(
                slider("gutter-slider")
                    .range(0., 0.9)
                    .step(0.05)
                    .value(r.gutter)
                    .width(px(220.))
                    .format(|v| format!("{:.0}%", v * 100.).into())
                    .on_change(cx.listener(|this, v: &f32, _, cx| this.set_recipe(|r| r.gutter = *v, cx))),
            ))
            .child(
                switch("modulate")
                    .label("Size by tone")
                    .checked(r.modulate)
                    .on_change(cx.listener(|this, on: &bool, _, cx| this.set_recipe(|r| r.modulate = *on, cx))),
            )
            .child(
                field("paper-kind", "Paper").hint("Which palette colour the shapes sit on").stacked().child(
                    Paper::ALL
                        .iter()
                        .fold(segmented("paper-seg"), |seg, pa| seg.option(pa.name()))
                        .selected(Paper::ALL.iter().position(|pa| *pa == r.paper).unwrap_or(0))
                        .on_select(cx.listener(|this, i: &usize, _, cx| this.set_recipe(|r| r.paper = Paper::ALL[*i], cx))),
                ),
            )
            .child(
                switch("transparent")
                    .label("Transparent paper")
                    .checked(r.transparent)
                    .on_change(cx.listener(|this, on: &bool, _, cx| this.set_recipe(|r| r.transparent = *on, cx))),
            )
            .child(field("lattice", "Lattice").hint("Dots over the bare paper, in the colour furthest from it").stacked().child(
                slider("lattice-slider")
                    .range(0., 1.)
                    .step(0.05)
                    .value(r.lattice)
                    .width(px(220.))
                    .format(|v| if v <= 0. { "off".into() } else { format!("{:.0}%", v * 100.).into() })
                    .on_change(cx.listener(|this, v: &f32, _, cx| this.set_recipe(|r| r.lattice = *v, cx))),
            ))
    }

    fn anim_controls(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let r = &self.settings.recipe;
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                field("stability", "Stability")
                    .hint("Holds pixels that didn't change since the last frame, so diffusion doesn't shimmer")
                    .stacked()
                    .child(
                        slider("stability-slider")
                            .range(0., 1.)
                            .step(0.05)
                            .value(r.stability)
                            .width(px(220.))
                            .format(|v| if v <= 0. { "off".into() } else { format!("{:.0}%", v * 100.).into() })
                            .on_change(cx.listener(|this, v: &f32, _, cx| this.set_recipe(|r| r.stability = *v, cx))),
                    ),
            )
            .child(field("speed", "Speed").stacked().child(
                slider("speed-slider")
                    .range(0.25, 4.)
                    .step(0.25)
                    .value(r.speed)
                    .width(px(220.))
                    .format(|v| format!("{v:.2}x").into())
                    .on_change(cx.listener(|this, v: &f32, _, cx| this.set_recipe(|r| r.speed = *v, cx))),
            ))
    }

    /// The preset row, the colours it gives, and how they're matched.
    fn palette_picker(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let s = &self.settings;
        let chips = palettes::PRESETS.iter().enumerate().map(|(i, preset)| {
            let key = preset.key;
            Button::new(("palette", i))
                .label(preset.name)
                .small()
                .secondary()
                .selected(s.recipe.palette == key)
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.change(|s| s.recipe.palette = key.into(), window, cx)))
        });
        // The custom palette gets a chip once there is one to go back to.
        let custom = (!s.recipe.colors.is_empty()).then(|| {
            Button::new("palette-custom")
                .label(format!("Custom ({})", s.recipe.colors.len()))
                .small()
                .secondary()
                .selected(self.custom().is_some())
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.set_recipe(|r| r.palette = palettes::CUSTOM.into(), cx)))
        });
        // The swatches are the art's own colours, data rather than chrome.
        let swatches = self.palette_colors(cx).into_iter().map(|[r, g, b]| div().w(px(14.)).h(px(14.)).bg(Rgba { r, g, b, a: 1. }));
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().flex().flex_row().flex_wrap().gap_1().children(chips).children(custom))
            .child(div().flex().flex_row().flex_wrap().children(swatches))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(Button::new("import-palette").label("Import…").icon(Icon::Folder).ghost().small().on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.import_palette(cx))))
                    .child(Button::new("paste-palette").label("Paste hex").icon(Icon::Copy).ghost().small().on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.paste_palette(cx)))),
            )
            .child(div().body(text::SM).text_color(hsla(palette(cx).fg_dim)).child("Lospec .hex, .gpl, .pal, .txt or a palette image; or copy hex colours and paste."))
            .child(field("match", "Match colours").hint("Oklab compares as the eye does; RGB is cruder and punchier").stacked().child(
                segmented("match-seg")
                    .option("Oklab")
                    .option("RGB")
                    .selected(if s.recipe.space == Space::Rgb { 1 } else { 0 })
                    .on_select(cx.listener(|this, i: &usize, window, cx| this.change(|s| s.recipe.space = if *i == 1 { Space::Rgb } else { Space::Oklab }, window, cx))),
            ))
    }

    fn sidebar(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let p = palette(cx);
        let s = &self.settings;
        let mode = s.recipe.mode;

        let process = match mode {
            Mode::Dither => div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    field("algo", "Algorithm")
                        .hint(if s.recipe.algo.diffuses() { "Error diffusion: each pixel's error carries to its neighbours" } else { "Ordered: a threshold map, stable and tileable" })
                        .stacked()
                        .child(
                            select("algo-select")
                                .options(Algo::ALL.iter().map(|a| a.name()))
                                .selected(Algo::ALL.iter().position(|a| *a == s.recipe.algo))
                                .width(px(220.))
                                .on_change(cx.listener(|this, i: &usize, window, cx| this.change(|s| s.recipe.algo = Algo::ALL[*i], window, cx))),
                        ),
                )
                .child(field("strength", "Strength").stacked().child(
                    slider("strength-slider")
                        .range(0., 2.)
                        .step(0.05)
                        .value(s.recipe.strength)
                        .width(px(220.))
                        .format(|v| format!("{:.0}%", v * 100.).into())
                        .on_change(cx.listener(|this, v: &f32, window, cx| this.change(|s| s.recipe.strength = *v, window, cx))),
                ))
                .child(
                    field("bias", "Threshold")
                        .hint(if s.recipe.algo.diffuses() { "Where ink starts; with diffusion it shifts texture more than tone" } else { "Where ink starts: left for less, right for more" })
                        .stacked()
                        .child(
                            slider("bias-slider")
                                .range(-1., 1.)
                                .step(0.05)
                                .value(s.recipe.bias)
                                .width(px(220.))
                                .format(|v| format!("{:+.0}", v * 100.).into())
                                .on_change(cx.listener(|this, v: &f32, window, cx| this.change(|s| s.recipe.bias = *v, window, cx))),
                        ),
                )
                .when(s.recipe.algo == Algo::Random, |el| {
                    el.child(
                        Button::new("reseed")
                            .label(format!("New seed ({})", s.recipe.seed))
                            .icon(Icon::Refresh)
                            .secondary()
                            .small()
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.reseed(cx))),
                    )
                })
                .when(s.recipe.algo.diffuses(), |el| {
                    el.child(
                        switch("serpentine")
                            .label("Serpentine")
                            .checked(s.recipe.serpentine)
                            .on_change(cx.listener(|this, on: &bool, window, cx| this.change(|s| s.recipe.serpentine = *on, window, cx))),
                    )
                })
                .child(field("cols", "Width").stacked().child(
                    slider("cols-slider")
                        .range(32., 480.)
                        .step(8.)
                        .value(s.recipe.cols as f32)
                        .width(px(220.))
                        .format(|v| format!("{v:.0} px").into())
                        .on_change(cx.listener(|this, v: &f32, window, cx| this.change(|s| s.recipe.cols = *v as u32, window, cx))),
                ))
                .child(field("scale", "PNG pixel size").stacked().child(
                    SCALES.iter().fold(segmented("scale-seg"), |seg, x| seg.option(format!("{x}x")))
                        .selected(SCALES.iter().position(|x| *x == s.recipe.scale).unwrap_or(2))
                        .on_select(cx.listener(|this, i: &usize, window, cx| this.change(|s| s.recipe.scale = SCALES[*i], window, cx))),
                )),
            Mode::Ascii => div()
                .flex()
                .flex_col()
                .gap_3()
                .child(field("charset", "Characters").stacked().child(
                    select("charset-select")
                        .options(Charset::ALL.iter().map(|c| c.name()))
                        .selected(Charset::ALL.iter().position(|c| *c == s.recipe.charset))
                        .width(px(220.))
                        .on_change(cx.listener(|this, i: &usize, window, cx| {
                            let c = Charset::ALL[*i];
                            this.change(
                                |s| {
                                    s.recipe.charset = c;
                                    s.recipe.fit = c.default_fit();
                                },
                                window,
                                cx,
                            )
                        })),
                ))
                .child(field("fit", "Fit").hint("Shape follows edges; tone follows brightness").stacked().child(
                    segmented("fit-seg")
                        .option("Shape")
                        .option("Tone")
                        .selected(if s.recipe.fit == Fit::Tone { 1 } else { 0 })
                        .on_select(cx.listener(|this, i: &usize, window, cx| this.change(|s| s.recipe.fit = if *i == 1 { Fit::Tone } else { Fit::Shape }, window, cx))),
                ))
                .child(field("ascii-cols", "Width").stacked().child(
                    slider("ascii-cols-slider")
                        .range(20., 200.)
                        .step(4.)
                        .value(s.recipe.ascii_cols as f32)
                        .width(px(220.))
                        .format(|v| format!("{v:.0} ch").into())
                        .on_change(cx.listener(|this, v: &f32, window, cx| this.change(|s| s.recipe.ascii_cols = *v as u32, window, cx))),
                )),
        };

        let animated = self.photo.frames() > 1;
        let exports = match mode {
            Mode::Dither if animated => div()
                .flex()
                .flex_col()
                .gap_2()
                .child(Button::new("export-gif").label("Export GIF").icon(Icon::File).primary().full_width().shortcut("Ctrl+E").on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| this.export_anim(AnimOut::Gif, cx),
                )))
                .child(Button::new("export-apng").label("Export APNG").icon(Icon::File).secondary().full_width().on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| this.export_anim(AnimOut::Apng, cx),
                )))
                .child(Button::new("export-sheet").label("Export sprite sheet").icon(Icon::File).secondary().full_width().on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| this.export_anim(AnimOut::Sheet, cx),
                )))
                .child(Button::new("export-frame").label("Export this frame").icon(Icon::File).ghost().full_width().on_click(cx.listener(
                    |this, _: &ClickEvent, window, cx| this.export_png(window, cx),
                ))),
            Mode::Dither => div()
                .flex()
                .flex_col()
                .gap_2()
                .child(Button::new("export-png").label("Export PNG").icon(Icon::File).primary().full_width().shortcut("Ctrl+E").on_click(cx.listener(
                    |this, _: &ClickEvent, window, cx| this.export_png(window, cx),
                ))),
            Mode::Ascii => div()
                .flex()
                .flex_col()
                .gap_2()
                .child(Button::new("export-text").label("Export text").icon(Icon::File).primary().full_width().shortcut("Ctrl+Shift+E").on_click(
                    cx.listener(|this, _: &ClickEvent, _, cx| this.export_text(cx)),
                ))
                .child(Button::new("copy-text").label("Copy").icon(Icon::Copy).secondary().full_width().shortcut("Ctrl+Shift+C").on_click(
                    cx.listener(|this, _: &ClickEvent, _, cx| this.copy_text(cx)),
                )),
        };

        panel("Develop").w(px(SIDEBAR)).flex_none().min_h_0().child(
            scroll_area("develop-scroll").flex_1().min_h_0().child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .p_2()
                    .child(
                        segmented("mode-seg")
                            .option("Dither")
                            .option("ASCII")
                            .selected(if mode == Mode::Ascii { 1 } else { 0 })
                            .on_select(cx.listener(|this, i: &usize, window, cx| this.change(|s| s.recipe.mode = if *i == 1 { Mode::Ascii } else { Mode::Dither }, window, cx))),
                    )
                    .child(process)
                    .child(rule(Some("tone"), window, cx))
                    .child(field("brightness", "Brightness").stacked().child(
                        slider("brightness-slider")
                            .range(-1., 1.)
                            .step(0.05)
                            .value(s.recipe.brightness)
                            .width(px(220.))
                            .format(|v| format!("{:+.0}", v * 100.).into())
                            .on_change(cx.listener(|this, v: &f32, window, cx| this.change(|s| s.recipe.brightness = *v, window, cx))),
                    ))
                    .child(field("contrast", "Contrast").stacked().child(
                        slider("contrast-slider")
                            .range(0.25, 3.)
                            .step(0.05)
                            .value(s.recipe.contrast)
                            .width(px(220.))
                            .format(|v| format!("{v:.2}x").into())
                            .on_change(cx.listener(|this, v: &f32, window, cx| this.change(|s| s.recipe.contrast = *v, window, cx))),
                    ))
                    .child(field("gamma", "Gamma").stacked().child(
                        slider("gamma-slider")
                            .range(0.2, 3.)
                            .step(0.05)
                            .value(s.recipe.gamma)
                            .width(px(220.))
                            .format(|v| format!("{v:.2}").into())
                            .on_change(cx.listener(|this, v: &f32, window, cx| this.change(|s| s.recipe.gamma = *v, window, cx))),
                    ))
                    .child(switch("invert").label("Invert").checked(s.recipe.invert).on_change(cx.listener(|this, on: &bool, window, cx| this.change(|s| s.recipe.invert = *on, window, cx))))
                    .when(mode == Mode::Dither, |el| el.child(rule(Some("render"), window, cx)).child(self.render_controls(cx)))
                    .when(mode == Mode::Dither && animated, |el| el.child(rule(Some("animation"), window, cx)).child(self.anim_controls(cx)))
                    .child(rule(Some("palette"), window, cx))
                    .when(mode == Mode::Dither, |el| el.child(self.palette_picker(cx)))
                    .child(div().body(text::SM).text_color(hsla(p.fg_dim)).child(
                        if mode == Mode::Dither && self.settings.recipe.palette != palettes::SCHEME {
                            "The scheme dresses the window; the preset colours the art."
                        } else {
                            "The art takes the scheme's colours: its paper, and its text or accent as ink."
                        },
                    ))
                    .child(field("scheme", "Scheme").stacked().child(
                        select("scheme-select")
                            .options(SCHEMES.iter().map(|sc| sc.name))
                            .selected(SCHEMES.iter().position(|sc| sc.key == s.scheme))
                            .width(px(220.))
                            .on_change(cx.listener(|this, i: &usize, window, cx| this.change(|s| s.scheme = SCHEMES[*i].key.into(), window, cx))),
                    ))
                    .child(field("appearance", "Paper").stacked().child(
                        APPEARANCES.iter().fold(segmented("appearance-seg"), |seg, (_, l)| seg.option(*l))
                            .selected(APPEARANCES.iter().position(|(k, _)| *k == s.appearance).unwrap_or(0))
                            .on_select(cx.listener(|this, i: &usize, window, cx| this.change(|s| s.appearance = APPEARANCES[*i].0.into(), window, cx))),
                    ))
                    .child(field("ink", "Ink").stacked().child(
                        segmented("ink-seg")
                            .option("Text")
                            .option("Accent")
                            .selected(if s.recipe.accent_ink { 1 } else { 0 })
                            .on_select(cx.listener(|this, i: &usize, window, cx| this.change(|s| s.recipe.accent_ink = *i == 1, window, cx))),
                    ))
                    .child(rule(Some("export"), window, cx))
                    .child(exports)
                    .child(Button::new("save-recipe").label("Save recipe").icon(Icon::File).secondary().full_width().shortcut("Ctrl+S").on_click(
                        cx.listener(|this, _: &ClickEvent, _, cx| this.save_recipe(cx)),
                    )),
            ),
        )
    }
}

impl Render for Darkroom {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let (w, h) = (self.photo.print().w, self.photo.print().h);
        let size_meta = match self.settings.recipe.mode {
            Mode::Dither => format!("{} · {}×{} px", self.photo.name, self.settings.recipe.cols, studio::rows_for(self.photo.print(), self.settings.recipe.cols)),
            Mode::Ascii => format!("{} · {} ch", self.photo.name, self.settings.recipe.ascii_cols),
        };
        self.tex.begin();
        let preview = self.preview(window, cx);
        self.tex.end(window);
        let view_bar = (self.settings.recipe.mode == Mode::Dither).then(|| self.view_bar(cx));
        let timeline = (self.photo.frames() > 1 && self.view != View::Sheet).then(|| self.timeline(cx));
        let drop_hint = hsla(p.raised);
        let print = panel("Print").meta(size_meta).flex_1().min_w_0().min_h_0().children(view_bar.map(|bar| div().px_2().pb_2().child(bar))).child(
            div()
                .id("easel")
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .items_center()
                .justify_center()
                .gap_3()
                .bg(hsla(p.sunken))
                .drag_over::<ExternalPaths>(move |style, _, _, _| style.bg(drop_hint))
                .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                    if let Some(path) = paths.paths().first() {
                        this.open_path(path.clone(), cx);
                    }
                }))
                .child(if self.loading { spinner("loading").into_any_element() } else { preview })
                .when(self.photo.path.is_none() && self.view != View::Sheet, |el| {
                    el.child(div().body(text::SM).text_color(hsla(p.fg_dim)).child("A sample print. Drop a photo or a GIF here, or open one with Ctrl+O."))
                }),
        )
        .children(timeline.map(|t| div().px_2().pt_2().child(t)));

        let open = Button::new("open").label("Open").icon(Icon::Folder).ghost().small().shortcut("Ctrl+O").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.open(cx)));
        let sidebar = self.sidebar(window, cx);

        window_frame().child(power_on_in(
            "power",
            div()
                .flex()
                .flex_col()
                .size_full()
                .bg(hsla(p.bg))
                .text_color(hsla(p.fg))
                .body(text::BASE)
                .on_action(cx.listener(|this, _: &TogglePalette, window, cx| this.palette.update(cx, |p, cx| p.toggle(window, cx))))
                .on_action(cx.listener(|this, _: &Open, _, cx| this.open(cx)))
                .on_action(cx.listener(|this, _: &OpenRecipe, _, cx| this.open_recipe(cx)))
                .on_action(cx.listener(|this, _: &SaveRecipe, _, cx| this.save_recipe(cx)))
                .on_action(cx.listener(|this, _: &Undo, _, cx| this.undo(cx)))
                .on_action(cx.listener(|this, _: &Redo, _, cx| this.redo(cx)))
                .on_action(cx.listener(|this, _: &ExportPng, window, cx| {
                    if this.photo.frames() > 1 && this.settings.recipe.mode == Mode::Dither {
                        this.export_anim(AnimOut::Gif, cx)
                    } else {
                        this.export_png(window, cx)
                    }
                }))
                .on_action(cx.listener(|this, _: &PlayPause, _, cx| this.toggle_play(cx)))
                .on_action(cx.listener(|this, _: &NextFrame, _, cx| this.step(1, cx)))
                .on_action(cx.listener(|this, _: &PrevFrame, _, cx| this.step(-1, cx)))
                .on_action(cx.listener(|this, _: &ExportText, _, cx| this.export_text(cx)))
                .on_action(cx.listener(|this, _: &CopyText, _, cx| this.copy_text(cx)))
                .on_action(cx.listener(|this, _: &Invert, window, cx| this.change(|s| s.recipe.invert = !s.recipe.invert, window, cx)))
                .child(self.palette.clone())
                .child(self.toaster.clone())
                .child(title_bar("Darkroom").child(open))
                .child(div().flex().flex_row().flex_1().min_h_0().gap(space::ROW).p(space::ROW).child(print).child(sidebar))
                .child(
                    status_bar()
                        .left(match self.settings.recipe.mode {
                            Mode::Dither => format!("DITHER · {} · {}", self.settings.recipe.algo.name().to_uppercase(), self.palette_name().to_uppercase()),
                            Mode::Ascii => format!("ASCII · {}", self.settings.recipe.charset.name().to_uppercase()),
                        })
                        .left(format!("SOURCE {w}×{h}"))
                        .when(self.photo.frames() > 1, |bar| bar.right_live(format!("FRAME {}/{}", self.photo.frame + 1, self.photo.frames())))
                        .right(theme::scheme(cx).name.to_uppercase())
                        .right_live(format!("{}FPS", motion::fps())),
                ),
        ))
    }
}

gpui::actions!(darkroom, [Open, OpenRecipe, SaveRecipe, ExportPng, ExportText, CopyText, Invert, Undo, Redo, PlayPause, NextFrame, PrevFrame]);

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = cli::run(&args) {
        std::process::exit(code);
    }
    gpui_platform::application().run(|cx: &mut App| {
        ferrite_design::init(Appearance::Dark, cx);
        cx.bind_keys([
            KeyBinding::new("ctrl-shift-p", TogglePalette, None),
            KeyBinding::new("ctrl-o", Open, None),
            KeyBinding::new("ctrl-shift-o", OpenRecipe, None),
            KeyBinding::new("ctrl-s", SaveRecipe, None),
            KeyBinding::new("ctrl-z", Undo, None),
            KeyBinding::new("ctrl-shift-z", Redo, None),
            KeyBinding::new("ctrl-y", Redo, None),
            KeyBinding::new("space", PlayPause, None),
            KeyBinding::new(".", NextFrame, None),
            KeyBinding::new(",", PrevFrame, None),
            KeyBinding::new("ctrl-e", ExportPng, None),
            KeyBinding::new("ctrl-shift-e", ExportText, None),
            KeyBinding::new("ctrl-shift-c", CopyText, None),
            KeyBinding::new("ctrl-i", Invert, None),
        ]);
        let options = chrome::window_options("Darkroom", size(px(1180.), px(780.)), cx);
        cx.open_window(options, |window, cx| {
            chrome::square_corners(window);
            chrome::power_off_on_close(window, cx);
            cx.new(|cx| Darkroom::new(window, cx))
        })
        .expect("failed to open the window");
        cx.activate(true);
    });
}
