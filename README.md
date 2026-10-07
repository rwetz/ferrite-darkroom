<p align="center">
  <img src="assets/logo.svg" width="128" height="128" alt="Darkroom logo: a framed photo developing from dither under an amber safelight">
</p>

<h1 align="center">Darkroom</h1>

<p align="center">
  A <a href="https://github.com/rwetz/ferrite-design">Ferrite</a> dither studio:
  turn a photo, GIF or video into pixel art or text art, in twenty-nine algorithms and any palette.
</p>

<p align="center">
  <a href="https://github.com/rwetz/ferrite-darkroom/actions/workflows/ci.yml"><img src="https://github.com/rwetz/ferrite-darkroom/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/rwetz/ferrite-darkroom/releases/latest"><img src="https://img.shields.io/github/v/release/rwetz/ferrite-darkroom?color=F2A93B&labelColor=18181B" alt="Latest release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-3D3D42?labelColor=18181B" alt="License: Apache-2.0"></a>
</p>

![Darkroom developing the sample print with Atkinson dither in amber](docs/main.png)

Darkroom turns photos into dithered pixel art. Drop an image on the window
and develop it with any of twenty-nine algorithms, from Floyd–Steinberg and
Atkinson to Bayer matrices, CMYK halftones, engraving lines and stippling,
or as ASCII, braille and block art. The art takes the colours of
whichever Ferrite scheme you pick (amber on iron, phosphor green, cyanotype
blue) or a preset palette: Game Boy, Commodore 64, ZX Spectrum, CMYK and
more. Export a PNG or a text file. It's how you make splash art
and icons for every other Ferrite app.

It's a native desktop app built with [GPUI](https://gpui.rs) and
[ferrite-design](https://github.com/rwetz/ferrite-design): amber on iron,
0px corners, pixel type and stepped motion. It runs no webview.

## Features

- **Bring a photo, an animation or a video.** Drop it on the print, open one
  with <kbd>Ctrl</kbd>+<kbd>O</kbd>, or pass a path: `ferrite-darkroom photo.jpg`.
  PNG, JPEG, GIF, BMP and WebP, animated GIF, APNG and WebP, and with
  [ffmpeg](https://ffmpeg.org/download.html) installed, video (MP4, MOV,
  WebM, MKV, AVI), sampled at 15 frames a second. A built-in sample print
  is there to start.
- **Animations.** An animated GIF plays in the print at its own timing, with
  a timeline under it: <kbd>Space</kbd> plays and pauses, <kbd>,</kbd> and
  <kbd>.</kbd> step a frame. *Stability* stops error diffusion from
  shimmering: where the picture stood still, a pixel keeps last frame's
  colour as long as it still fits, and dots left behind by something moving
  get cleared. *Speed* changes the timing. Export a GIF in the palette's
  exact colours (<kbd>Ctrl</kbd>+<kbd>E</kbd>), an APNG, a sprite sheet, an
  MP4 (with ffmpeg), or just the frame on show.
- **Dither.** Twenty-nine algorithms, at 32 to 480 pixels wide:
  - *Error diffusion:* Floyd–Steinberg, Atkinson, Jarvis–Judice–Ninke,
    Stucki, Burkes, Sierra, Sierra two-row, Sierra Lite, False
    Floyd–Steinberg, Shiau–Fan and Shiau–Fan 2 (with a serpentine switch),
    Riemersma along a Hilbert curve, and Knuth's dot diffusion.
  - *Ordered:* Bayer 2×2, 4×4, 8×8 and 16×16, blue noise, interleaved
    gradient noise, an 8×8 halftone dot, random, and plain threshold.
  - *Print and drawing:* a CMYK halftone with each ink screened at its own
    angle (try the 3-bit palette), crosshatch, engraving lines, and
    weighted-Voronoi stippling.
  - *Many-colour patterns:* Knoll and Yliluoma, which mix palette colours
    by plan rather than by error.
  - *Custom tile:* import a small square greyscale picture as your own
    threshold matrix.
  - *Strength* from 0 (flat posterise) to 200% (crunchy), and a
    *threshold* that moves where ink starts.
  The preview draws whole device pixels per art pixel, so it's exactly the
  export.
- **ASCII and text art.** Plain printable ASCII (the default: its text
  pastes anywhere) or any of ferrite-design's thirteen character sets, from
  classic `.:-=+*#%@` to box drawing and "every glyph", fitted by shape
  (edges) or by tone (brightness) to the chosen font's real glyphs, 16 to
  240 characters wide. *Smooth gradients* spreads each character's tone
  error to its neighbours, so a short set doesn't band. Six fonts, all
  bundled: IBM VGA (the DOS text-mode face), VT323 (a DEC terminal),
  JetBrains Mono, IBM Plex Mono, Cascadia Mono (full block, box and braille
  coverage) and Fira Mono. Masks work here too: the background is left as
  blank paper. Or pack more picture into each character: *braille* (2×4 dots),
  *half blocks* and *quadrants*, dithered with any of the twenty algorithms,
  and *colour half blocks*, two palette-coloured pixels per character.
  Colour each cell with the ink, the photo's own colours or the palette.
  Export plain text, ANSI (24-bit colour, `cat` it in a terminal), HTML,
  or a picture in any export format (characters drawn in the chosen font,
  with even, anti-aliased strokes at any size); an animation exports as an
  *ASCII film*, an HTML page that plays itself. Braille dots have a size.
- **Tone.** *Auto levels* stretches the photo's own darkest to lightest;
  a *black point* clears a dark background to paper (and a white point
  the light end); then brightness, contrast, gamma and invert.
- **Palette.** *Scheme* inks the art in the scheme's text or accent colour
  on its background (light appearance gives dark ink on light paper). Or
  pick a preset: Black & white, RGB, CMYK, 3-bit, Game Boy, Teletext,
  Apple II, Commodore 64, ZX Spectrum, 6-bit RGB, 2-bit grey, Vaporwave
  or Hacker, or the photo's own palette (*From photo*, 2 to 32 colours by
  k-means). Colours are matched in Oklab (as the eye sees), RGB, or linear
  light (so a dither averages to the photo's real brightness), and a
  palette without a true black or white still gets the photo's whole tonal
  range. Two-colour palettes dither by lightness, so any ink on any paper
  keeps every tone.
- **Pixels as shapes.** Draw each pixel as a square, circle, diamond or plus,
  with a gutter between them, dots that swell with the light (*size by
  tone*), a choice of which colour is the paper, transparent paper for PNGs,
  and a lattice of dots over the bare paper. It's how you get halftone dot
  grids and LED-board looks. Shapes show once a pixel is 3 px or more: in
  the preview, and in 4× and 8× PNGs.
- **Masks and layers.** A mask splits the print into a subject and a
  background: by brightness, by a colour you click on, *Border* (whatever
  floods in from the image's edge, which cuts a subject off a plain
  backdrop by itself), or painted with a brush in the *Mask* view. Feather
  it for a dithered edge, invert it. The background then becomes the same
  dither, bare paper (a cut-out, transparent if you like), or its own
  algorithm, strength and cells. A painted mask exports and imports as a
  greyscale PNG.
- **Compare and contact sheet.** *Compare* puts the photo left of a split
  and the art right of it. *Contact sheet* shows the print through all
  twenty algorithms, or every palette, side by side; click one to use it.
- **Undo.** <kbd>Ctrl</kbd>+<kbd>Z</kbd> and <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Z</kbd>
  (or <kbd>Ctrl</kbd>+<kbd>Y</kbd>) step through every change to the
  recipe; a slider drag undoes in one step.
- **Your own palettes.** Import one from [Lospec](https://lospec.com/palette-list)
  or anywhere else: `.hex`, GIMP `.gpl`, JASC `.pal`, paint.net `.txt`, or a
  palette image (its distinct colours). Or copy hex colours from anywhere and
  *Paste hex*. Dropping a palette file on the print imports it too.
- **Recipes.** Everything about how a print develops (algorithm, palette,
  tone, sizes) saves as a small TOML file (<kbd>Ctrl</kbd>+<kbd>S</kbd>).
  Open one (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>O</kbd>, or drop it on the
  print) to develop any photo the same way. Nine come built in, one click
  each from the Recipes row: Cut-out (a subject on a plain backdrop, in
  one ink on clean paper), Lattice (a solid figure on a grid of dots),
  Dots, Newsprint, CMYK print, Engraving, Game Boy, Sunset and Braille.
  Their files are in `recipes/`.
- **Export.** A picture in the palette's exact colours as PNG, JPEG, WebP,
  BMP, TIFF or SVG (vector, one path per colour)
  (<kbd>Ctrl</kbd>+<kbd>E</kbd>), sized by pixel size (1×, 2×, 4×, 8×) or by
  long edge: HD, Full HD, QHD, 4K, 5K, 8K, or any width up to 8192 px.
  Animations take the same size. A *frame* gives the picture a fixed shape
  (16:9, 16:10, 21:9, 32:9, 4:3, 1:1, 4:5, 9:16): *fill* crops the photo to
  it before developing, so a 4K wallpaper is exactly 3840 × 2160; *fit*
  keeps the whole picture and pads it with paper. The ASCII as a text file
  (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>E</kbd>), or the ASCII copied to the
  clipboard (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd>).
- **A print bigger than the window** scrolls, with scrollbars both ways.
  The zoom over the print picks *Fit* (all of it in view, shrunk if it has
  to be), or *1x*, *2x*, *4x* whole pixels.
- **What photo to use.** Anything about 1000 px wide or more is plenty: a
  still is worked at up to 960 px wide (an animation at 480), so a bigger
  file adds nothing, and the export size doesn't depend on it. A clear
  subject and good contrast matter far more than pixels. The Width hint
  says when a photo is too small for the width chosen.
- **Batch and command line.** *Batch develop a folder* (command palette)
  runs the current recipe over every picture in a folder, into a `darkroom`
  folder beside them. Or skip the window entirely:

  ```bash
  ferrite-darkroom photo.jpg -o print.png --recipe recipes/gameboy.toml
  ferrite-darkroom loop.gif -o loop.gif --recipe recipes/dots.toml --scheme phosphor
  ferrite-darkroom --batch ./photos -o ./prints --recipe recipes/newsprint.toml
  ```

  A painted mask goes along with `--mask mask.png`. The output's
  extension picks the format: `.png` (an APNG for an animation), `.gif`,
  a still `.jpg`, `.webp`, `.bmp` or `.tif`, or text art as `.txt`, `.ans`, `.svg` or `.html` (an ASCII film for an
  animation). `--size 8k` (or `4k`, `1080p`, `3000`, …) sets how big
  pictures come out, `--frame 16:9` (or `16:9-fit`, `21:9`, `9:16`, …) their
  shape. A recipe value Darkroom doesn't recognise is reported, not
  silently ignored. Without `--recipe` it uses the
  app's last recipe; `--help` lists the rest.
- **Ferrite throughout.** Ten color schemes, a command palette
  (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd>) with every algorithm,
  palette and character set in it, and a CRT switch-off when you close the window.

## Install

### Download

Prebuilt binaries for Windows, macOS and Linux are attached to each
[release](https://github.com/rwetz/ferrite-darkroom/releases/latest).
Unpack the archive and run `ferrite-darkroom`. Nothing else is needed.

### From source

```bash
git clone https://github.com/rwetz/ferrite-darkroom
cd ferrite-darkroom
cargo run --release
```

On Linux you need the usual GPUI development packages first:

```bash
sudo apt install libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libxcb1-dev libfontconfig-dev
```

On macOS with Xcode 27 or later, install the Metal toolchain once:

```bash
xcodebuild -downloadComponent MetalToolchain
```

## Configuration

Darkroom remembers the look and the last recipe you used, as plain
`key = value` lines (a recipe file uses the same keys, in TOML):

| OS | Settings |
|---|---|
| Windows | `%APPDATA%\ferrite\darkroom.conf` |
| macOS | `~/Library/Application Support/ferrite/darkroom.conf` |
| Linux | `$XDG_CONFIG_HOME/ferrite/darkroom.conf` (or `~/.config/…`) |

```ini
scheme = ferrite      # ferrite mono graphite slate concrete harbor cyanotype phosphor verdigris bruise
appearance = dark     # dark | light | system: the paper
mode = dither         # dither | ascii
algorithm = atkinson  # floyd-steinberg atkinson jarvis-judice-ninke stucki burkes sierra sierra-two-row
                      # sierra-lite false-floyd-steinberg shiau-fan shiau-fan-2 bayer-2 bayer bayer-8
                      # bayer-16 blue-noise gradient-noise halftone random threshold
palette = scheme      # scheme bw rgb cmyk 3-bit gameboy teletext apple-ii c64 zx-spectrum 6-bit
                      # 2-bit-grey vaporwave hacker custom
colors = 0f380f 306230 8bac0f 9bbc0f   # palette = custom: its colours
strength = 1.00       # 0 to 2
bias = 0.00           # -1 to 1: the threshold
serpentine = true     # error diffusion alternates direction per row
match = oklab         # oklab | rgb | linear
tile = 0 8 12 4 / 9 13 5 1 / 14 6 2 10 / 7 3 11 15   # algorithm = tile: ranks, rows split by /
seed = 1              # for algorithm = random
stability = 0.50      # 0 to 1: animations hold still pixels between frames
speed = 1.00          # 0.25 to 4: animation playback and export speed
cols = 160            # dither width in pixels
scale = 4             # PNG pixel size
shape = square        # square | circle | diamond | plus
gutter = 0.00         # 0 to 0.9: gap between pixels
modulate = false      # size by tone
paper = first         # first | darkest | lightest: the palette colour shapes sit on
transparent = false   # leave the paper transparent in PNGs
lattice = 0.00        # 0 to 1: dots over the bare paper
mask = none           # none | brightness | colour | border | painted
mask_low = 0.50       # brightness: the subject's lightness range
mask_high = 1.00
mask_colour = ffffff  # colour: the key colour
mask_tolerance = 0.15 # colour, border: how close counts as a match
mask_feather = 0.20   # 0 to 1: a soft, dithered edge
mask_invert = false
background = paper    # same | paper | own: where the mask isn't
bg_algorithm = bayer  # background = own: its algorithm, strength, threshold
bg_strength = 1.00
bg_bias = 0.00
bg_shape = square     # its cells; bg_lattice also dots a paper background
bg_gutter = 0.00
bg_modulate = false
bg_lattice = 0.00
levels = auto         # auto | manual: stretch to the photo's own range
black = 0.00          # 0 to 0.6: tones under this go to black
white = 1.00          # 0.4 to 1: tones over this go to white
brightness = 0.00     # -1 to 1
contrast = 1.00       # 0.25 to 3
gamma = 1.00          # 0.2 to 5
invert = false
ink = accent          # accent | text: the Scheme palette's ink
charset = ascii       # ascii, or a ferrite-design set: classic, box-drawing, best-character, …
fit = shape           # shape | tone
diffuse = false       # smooth gradients: spread tone error between characters
font = vga            # vga | vt323 | jetbrains-mono | plex-mono | cascadia-mono | fira-mono
dot = 0.72            # braille dot size, 0.3 to 1
ascii_cols = 80
glyphs = characters   # characters | braille | half-blocks | quadrants | colour-blocks
ascii_colour = ink    # ink | photo | palette
```

`FERRITE_*` variables (the shared look from
[Lodestone](https://github.com/rwetz/ferrite-lodestone)) win over the saved
scheme and appearance.

## With Lodestone

[Lodestone](https://github.com/rwetz/ferrite-lodestone) launches Ferrite apps
from their **source checkouts**: it scans a folder for crates that depend on
ferrite-design, runs `cargo build`, and starts the result with its shared
look. To see Darkroom there, clone this repo next to Lodestone (or into the
folder you point Lodestone at) and have a Rust toolchain on your `PATH`:

```
Dev/
├── ferrite-lodestone/
└── ferrite-darkroom/  # appears as a tile in Lodestone
```

A downloaded Darkroom binary runs fine on its own, but Lodestone doesn't
discover installed binaries.

## Development

```bash
cargo test                    # every algorithm and palette, colour, tone, PNG export, ASCII, settings
cargo clippy --all-targets
```

- `src/engine.rs`: the dither engine: every algorithm, Oklab colour
  matching, palette mapping. Pure and unit-tested.
- `src/palettes.rs`: the preset palettes and palette-file import.
- `src/recipe.rs`: the recipe, read and written as settings lines or TOML.
- `src/algos.rs`: Riemersma, dot diffusion, the Knoll and Yliluoma
  pattern dithers, the CMYK halftone and stippling.
- `src/sequence.rs`: animations and video: decoding frames, developing a
  whole clip with temporal stability, and GIF / APNG / sprite-sheet / MP4
  encoding.
- `src/batch.rs` and `src/cli.rs`: developing files without a window.
- `src/textart.rs`: text art: glyph sets, cell colours, and the text,
  ANSI, HTML, SVG, PNG and film exports.
- `src/fit.rs`: character sets and fitting cells to glyphs (tone, shape,
  diffusion). `src/fonts.rs`: the bundled faces, drawn at any cell size.
- `src/export.rs`: export formats, sizes and frames. `src/pan.rs`: the
  print's two-way scroll pane.
- `src/mask.rs`: masks (brightness, colour, border flood, painted) and
  their dithered edges.
- `src/render.rs`: how art pixels are drawn (shapes, gutter, paper,
  lattice) for the preview and PNGs, and the compare view's "before".
- `recipes/`: example recipes; tests check each one loads.
- `src/studio.rs`: the process around it: loading, the working print,
  adjustments, PNG and preview pixels, ASCII. Pure and unit-tested; the
  preview and the exports both run it.
- `docs/ROADMAP.md`: where Darkroom is going (GIFs, masks, layers, an
  ASCII engine).
- `src/settings.rs`: the settings file.
- `src/main.rs`: the view. It follows ferrite-design's
  [AGENTS.md](https://github.com/rwetz/ferrite-design/blob/main/AGENTS.md).

On Windows, `build.rs` embeds `assets/logo.ico` as the executable and window
icon. The `.ico` is generated from `assets/logo.svg`, so regenerate it when
the logo changes.

## License

Apache-2.0, see [LICENSE](LICENSE). The binary embeds fonts from
ferrite-design under their own licenses; see [NOTICE](NOTICE).
