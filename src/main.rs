//! Darkroom: a Ferrite dither studio.
//!
//! Drop in a photo (or open one) and develop it as pixel art with Ferrite's
//! own dither masks — ordered Bayer, blue noise, Atkinson diffusion — or as
//! ASCII with Ferrite's glyph-fitted character sets. The art takes the
//! colours of the scheme you pick. Export a PNG or a text file.
//!
//!     cargo run
//!     cargo run -- photo.jpg

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod settings;
mod studio;

use std::path::{Path, PathBuf};
use std::rc::Rc;

use ferrite_design::ascii::{Charset, Fit};
use ferrite_design::dither::Picture;
use gpui::{
    App, AppContext as _, ClickEvent, ClipboardItem, Context, Entity, ExternalPaths, IntoElement, KeyBinding, PathPromptOptions, Render,
    Rgba, Subscription, Window, div, px, size,
};
use ferrite_design::prelude::*;

use settings::{Mode, PATTERNS, Settings};
use studio::{Adjust, Print};

const APPEARANCES: [(&str, &str); 3] = [("dark", "Dark"), ("light", "Light"), ("system", "System")];
const SCALES: [u32; 4] = [1, 2, 4, 8];
const SIDEBAR: f32 = 360.;

/// What's on the easel.
struct Photo {
    name: String,
    /// Where it came from (for the export dialog's folder).
    path: Option<PathBuf>,
    print: Print,
}

/// The developed picture and what it was developed with.
type Developed = ((u32, [u32; 2], bool, bool), Picture);
type Typeset = ((usize, [u32; 2], bool, bool, Charset, Fit), Rc<Vec<String>>);

struct Darkroom {
    settings: Settings,
    photo: Photo,
    loading: bool,
    /// Bumped per photo, to replay the develop effect.
    roll: u64,
    developed: Option<Developed>,
    typeset: Option<Typeset>,
    palette: Entity<CommandPalette>,
    toaster: Entity<Toaster>,
    _appearance: Subscription,
}

impl Darkroom {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut room = Self {
            settings: Settings::load(),
            photo: Photo { name: "sample".into(), path: None, print: studio::sample() },
            loading: false,
            roll: 0,
            developed: None,
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
        f(&mut self.settings);
        self.apply_look(window, cx);
        self.save(cx);
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
            command("Export PNG…").group("File").icon(Icon::File).shortcut("Ctrl+E").on_run(run(|this, window, cx| this.export_png(window, cx))),
            command("Export text…").group("File").icon(Icon::File).shortcut("Ctrl+Shift+E").on_run(run(|this, _, cx| this.export_text(cx))),
            command("Copy ASCII").group("File").icon(Icon::Copy).shortcut("Ctrl+Shift+C").on_run(run(|this, _, cx| this.copy_text(cx))),
            command("Back to the sample").group("File").icon(Icon::Refresh).on_run(run(|this, _, cx| {
                this.photo = Photo { name: "sample".into(), path: None, print: studio::sample() };
                this.new_roll();
                cx.notify();
            })),
            command("Dither").group("Develop").on_run(run(|this, window, cx| this.change(|s| s.mode = Mode::Dither, window, cx))),
            command("ASCII").group("Develop").on_run(run(|this, window, cx| this.change(|s| s.mode = Mode::Ascii, window, cx))),
            command("Invert").group("Develop").shortcut("Ctrl+I").on_run(run(|this, window, cx| this.change(|s| s.invert = !s.invert, window, cx))),
            command("Reset tone").group("Develop").on_run(run(|this, window, cx| {
                this.change(
                    |s| {
                        s.brightness = 0.;
                        s.contrast = 1.;
                        s.invert = false;
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
        for (pattern, _, label) in PATTERNS {
            let weak = weak.clone();
            commands.push(command(format!("Pattern: {label}")).group("Develop").on_run(move |window, cx| {
                let _ = weak.update(cx, |this, cx| {
                    this.change(
                        |s| {
                            s.mode = Mode::Dither;
                            s.pattern = pattern;
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
                            s.mode = Mode::Ascii;
                            s.charset = charset;
                            s.fit = charset.default_fit();
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

    fn open(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some("Develop".into()) });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |this, cx| this.open_path(path, cx));
            }
        })
        .detach();
    }

    fn open_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.loading = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let p = path.clone();
            let result = cx.background_executor().spawn(async move { studio::load(&p) }).await;
            let _ = this.update(cx, |this, cx| {
                this.loading = false;
                match result {
                    Ok(print) => {
                        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "photo".into());
                        this.photo = Photo { name, path: Some(path), print };
                        this.new_roll();
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
    }

    fn adjust(&self, cx: &App) -> Adjust {
        let p = palette(cx);
        let s = &self.settings;
        let ink = hsla(if s.accent_ink { p.accent } else { p.fg });
        Adjust { brightness: s.brightness, contrast: s.contrast, invert: s.invert, light_ink: ink.l > hsla(p.bg).l }
    }

    fn adjust_key(a: Adjust) -> ([u32; 2], bool, bool) {
        ([a.brightness.to_bits(), a.contrast.to_bits()], a.invert, a.light_ink)
    }

    /// The dither picture, developed again only when something changed.
    fn developed(&mut self, cx: &App) -> Picture {
        let a = self.adjust(cx);
        let (b, inv, light) = Self::adjust_key(a);
        let key = (self.settings.cols, b, inv, light);
        match &self.developed {
            Some((k, pic)) if *k == key => pic.clone(),
            _ => {
                let pic = studio::picture(&self.photo.print, self.settings.cols, a);
                self.developed = Some((key, pic.clone()));
                pic
            }
        }
    }

    fn typeset(&mut self, cx: &App) -> Rc<Vec<String>> {
        let a = self.adjust(cx);
        let (b, inv, light) = Self::adjust_key(a);
        let s = &self.settings;
        let key = (s.ascii_cols as usize, b, inv, light, s.charset, s.fit);
        match &self.typeset {
            Some((k, lines)) if *k == key => lines.clone(),
            _ => {
                let lines = studio::ascii_lines(&self.photo.print, key.0, a, s.charset, s.fit);
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
        let p = palette(cx);
        let rgb = |c: gpui::Hsla| {
            let c = Rgba::from(c);
            [c.r, c.g, c.b].map(|v| (v * 255.).round() as u8)
        };
        let (ink, paper) = (rgb(hsla(if self.settings.accent_ink { p.accent } else { p.fg })), rgb(hsla(p.bg)));
        let pic = self.developed(cx);
        match studio::png(&pic, self.settings.pattern, self.settings.scale, ink, paper) {
            Ok(bytes) => {
                let pattern = PATTERNS.iter().find(|x| x.0 == self.settings.pattern).map(|x| x.1).unwrap_or("dither");
                self.save_as(format!("{}-{pattern}.png", self.stem()), bytes, "PNG", cx);
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
        let ink = hsla(if self.settings.accent_ink { p.accent } else { p.fg });
        // The room the print has: the window less the sidebar, chrome and padding.
        let view = window.viewport_size();
        let (room_w, room_h) = (f32::from(view.width) - SIDEBAR - 72., f32::from(view.height) - 150.);
        match self.settings.mode {
            Mode::Dither => {
                let pic = self.developed(cx);
                let (w, h) = pic.size();
                // Whole device pixels per art pixel, so the preview is the export.
                let sf = window.scale_factor();
                let cell = ((room_w / w as f32).min(room_h / h as f32) * sf).floor().max(1.);
                let (ew, eh) = (w as f32 * cell / sf, h as f32 * cell / sf);
                develop(
                    ("print", self.roll),
                    self.roll,
                    dither(pic).pattern(self.settings.pattern).ink(ink).paper(hsla(p.bg)).cell(cell as u32).w(px(ew)).h(px(eh)),
                )
                .into_any_element()
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

    fn sidebar(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let p = palette(cx);
        let s = &self.settings;
        let mode = s.mode;

        let process = match mode {
            Mode::Dither => div()
                .flex()
                .flex_col()
                .gap_3()
                .child(field("pattern", "Pattern").stacked().child(
                    PATTERNS.iter().fold(segmented("pattern-seg"), |seg, (_, _, l)| seg.option(*l))
                        .selected(PATTERNS.iter().position(|x| x.0 == s.pattern).unwrap_or(0))
                        .on_select(cx.listener(|this, i: &usize, window, cx| this.change(|s| s.pattern = PATTERNS[*i].0, window, cx))),
                ))
                .child(field("cols", "Width").stacked().child(
                    slider("cols-slider")
                        .range(32., 480.)
                        .step(8.)
                        .value(s.cols as f32)
                        .width(px(220.))
                        .format(|v| format!("{v:.0} px").into())
                        .on_change(cx.listener(|this, v: &f32, window, cx| this.change(|s| s.cols = *v as u32, window, cx))),
                ))
                .child(field("scale", "PNG pixel size").stacked().child(
                    SCALES.iter().fold(segmented("scale-seg"), |seg, x| seg.option(format!("{x}x")))
                        .selected(SCALES.iter().position(|x| *x == s.scale).unwrap_or(2))
                        .on_select(cx.listener(|this, i: &usize, window, cx| this.change(|s| s.scale = SCALES[*i], window, cx))),
                )),
            Mode::Ascii => div()
                .flex()
                .flex_col()
                .gap_3()
                .child(field("charset", "Characters").stacked().child(
                    select("charset-select")
                        .options(Charset::ALL.iter().map(|c| c.name()))
                        .selected(Charset::ALL.iter().position(|c| *c == s.charset))
                        .width(px(220.))
                        .on_change(cx.listener(|this, i: &usize, window, cx| {
                            let c = Charset::ALL[*i];
                            this.change(
                                |s| {
                                    s.charset = c;
                                    s.fit = c.default_fit();
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
                        .selected(if s.fit == Fit::Tone { 1 } else { 0 })
                        .on_select(cx.listener(|this, i: &usize, window, cx| this.change(|s| s.fit = if *i == 1 { Fit::Tone } else { Fit::Shape }, window, cx))),
                ))
                .child(field("ascii-cols", "Width").stacked().child(
                    slider("ascii-cols-slider")
                        .range(20., 200.)
                        .step(4.)
                        .value(s.ascii_cols as f32)
                        .width(px(220.))
                        .format(|v| format!("{v:.0} ch").into())
                        .on_change(cx.listener(|this, v: &f32, window, cx| this.change(|s| s.ascii_cols = *v as u32, window, cx))),
                )),
        };

        let exports = match mode {
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
                            .on_select(cx.listener(|this, i: &usize, window, cx| this.change(|s| s.mode = if *i == 1 { Mode::Ascii } else { Mode::Dither }, window, cx))),
                    )
                    .child(process)
                    .child(rule(Some("tone"), window, cx))
                    .child(field("brightness", "Brightness").stacked().child(
                        slider("brightness-slider")
                            .range(-1., 1.)
                            .step(0.05)
                            .value(s.brightness)
                            .width(px(220.))
                            .format(|v| format!("{:+.0}", v * 100.).into())
                            .on_change(cx.listener(|this, v: &f32, window, cx| this.change(|s| s.brightness = *v, window, cx))),
                    ))
                    .child(field("contrast", "Contrast").stacked().child(
                        slider("contrast-slider")
                            .range(0.25, 3.)
                            .step(0.05)
                            .value(s.contrast)
                            .width(px(220.))
                            .format(|v| format!("{v:.2}x").into())
                            .on_change(cx.listener(|this, v: &f32, window, cx| this.change(|s| s.contrast = *v, window, cx))),
                    ))
                    .child(switch("invert").label("Invert").checked(s.invert).on_change(cx.listener(|this, on: &bool, window, cx| this.change(|s| s.invert = *on, window, cx))))
                    .child(rule(Some("palette"), window, cx))
                    .child(div().body(text::SM).text_color(hsla(p.fg_dim)).child("The art takes the scheme's colours: its paper, and its text or accent as ink."))
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
                            .selected(if s.accent_ink { 1 } else { 0 })
                            .on_select(cx.listener(|this, i: &usize, window, cx| this.change(|s| s.accent_ink = *i == 1, window, cx))),
                    ))
                    .child(rule(Some("export"), window, cx))
                    .child(exports),
            ),
        )
    }
}

impl Render for Darkroom {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let (w, h) = (self.photo.print.w, self.photo.print.h);
        let size_meta = match self.settings.mode {
            Mode::Dither => format!("{} · {}×{} px", self.photo.name, self.settings.cols, studio::rows_for(&self.photo.print, self.settings.cols)),
            Mode::Ascii => format!("{} · {} ch", self.photo.name, self.settings.ascii_cols),
        };
        let preview = self.preview(window, cx);
        let drop_hint = hsla(p.raised);
        let print = panel("Print").meta(size_meta).flex_1().min_w_0().min_h_0().child(
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
                .when(self.photo.path.is_none(), |el| {
                    el.child(div().body(text::SM).text_color(hsla(p.fg_dim)).child("A sample print. Drop a photo here, or open one with Ctrl+O."))
                }),
        );

        let open = Button::new("open").label("Open").icon(Icon::Folder).ghost().small().shortcut("Ctrl+O").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.open(cx)));
        let sidebar = self.sidebar(window, cx);
        let pattern = PATTERNS.iter().find(|x| x.0 == self.settings.pattern).map(|x| x.2).unwrap_or("");

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
                .on_action(cx.listener(|this, _: &ExportPng, window, cx| this.export_png(window, cx)))
                .on_action(cx.listener(|this, _: &ExportText, _, cx| this.export_text(cx)))
                .on_action(cx.listener(|this, _: &CopyText, _, cx| this.copy_text(cx)))
                .on_action(cx.listener(|this, _: &Invert, window, cx| this.change(|s| s.invert = !s.invert, window, cx)))
                .child(self.palette.clone())
                .child(self.toaster.clone())
                .child(title_bar("Darkroom").child(open))
                .child(div().flex().flex_row().flex_1().min_h_0().gap(space::ROW).p(space::ROW).child(print).child(sidebar))
                .child(
                    status_bar()
                        .left(match self.settings.mode {
                            Mode::Dither => format!("DITHER · {}", pattern.to_uppercase()),
                            Mode::Ascii => format!("ASCII · {}", self.settings.charset.name().to_uppercase()),
                        })
                        .left(format!("SOURCE {w}×{h}"))
                        .right(theme::scheme(cx).name.to_uppercase())
                        .right_live(format!("{}FPS", motion::fps())),
                ),
        ))
    }
}

gpui::actions!(darkroom, [Open, ExportPng, ExportText, CopyText, Invert]);

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        ferrite_design::init(Appearance::Dark, cx);
        cx.bind_keys([
            KeyBinding::new("ctrl-shift-p", TogglePalette, None),
            KeyBinding::new("ctrl-o", Open, None),
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
