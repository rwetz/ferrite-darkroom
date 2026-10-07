# Changelog

All notable changes to Darkroom are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [1.2.2] - 2026-10-07

### Fixed

- A print bigger than the window (zoomed in, or a tall or wide one) spilled
  over the toolbar and sidebar instead of scrolling: the scroll pane kept
  the overflowing side at the print's full size. It now clips to the room
  and scrolls, with its scrollbars.

## [1.2.1] - 2026-10-07

### Fixed

- An animation in ASCII mode exports as an animated GIF (<kbd>Ctrl</kbd>+<kbd>E</kbd>),
  APNG or MP4 of the text art, not only as an HTML film; at any export size
  and frame. On the command line, an ASCII recipe's `.gif`, `.png` or `.mp4`
  of an animation is the text art too.

## [1.2.0] - 2026-10-07

An overhaul of ASCII mode, from going through every glyph mode on a test
picture.

### Added

- Fonts for text art, all bundled: IBM VGA, VT323, JetBrains Mono, IBM Plex
  Mono, Cascadia Mono and Fira Mono. Characters are fitted to the chosen
  font's real glyphs, and HTML and SVG exports name it.
- Plain ASCII, a new character set and the default: printable ASCII only,
  so the text pastes anywhere. "Best character" is now "Every glyph".
- Smooth gradients (`diffuse`): each character's tone error spreads to its
  neighbours, so tone sets like Classic no longer band. On by default for
  the short tone sets.
- Levels: auto levels (on by default) stretch the photo's own range, and a
  black point and white point clear a dark or light background to paper.
  They work in dither mode too.
- Masks in ASCII mode; the background is left as blank paper.
- Frames: 16:9, 16:10, 21:9, 32:9, 4:3, 1:1, 4:5 and 9:16, filled (the photo
  cropped to the shape before developing, so a 4K export is exactly
  3840 × 2160) or fitted (padded with paper). `--frame` on the command
  line. A painted mask moves with the crop.
- Braille dot size.
- Compare and Mask views in ASCII mode.
- Recipes report values Darkroom doesn't recognise (a toast in the app, a
  line on stderr from the command line) instead of ignoring them.

### Changed

- Text-art characters are drawn anti-aliased at any size, so exports that
  aren't a whole multiple of the font have even strokes.
- The ASCII width goes to 240 characters (the recipe limit).
- Character set names in recipes are read in any case (`Classic`).

### Fixed

- From the command line, an ASCII recipe's `.png` was a dithered picture,
  not the text art.

## [1.1.0] - 2026-10-07

### Added

- Export pictures as PNG, JPEG, WebP, BMP, TIFF or SVG. The SVG of a dither
  is vector (one path per colour, cell shapes included), sharp at any size.
- Export size by long edge: HD, Full HD, QHD, 4K, 5K, 8K, or any width up to
  8192 px, besides the pixel size. GIF, APNG, MP4 and sprite sheets use it
  too. On the command line: `--size 8k`, and `.jpg` `.webp` `.bmp` `.tif`
  outputs.
- Character ASCII exports as a picture too, drawn from the display face's
  own 8×16 glyphs.
- A zoom over the print (Fit, 1x, 2x, 4x), and scrollbars both ways when the
  print is bigger than the window.
- The Width hint advises on the photo's size: whether it's big enough for
  the width chosen.

### Changed

- The ASCII preview is drawn as pixels, the same as the exported picture.

## [1.0.0] - 2026-10-07

Darkroom grows from a three-pattern dither toy into a dither and text-art
studio: twenty-nine algorithms, any palette, masks and layers, shapes,
animation and video, braille and block art, recipes, batches and a command
line. The reference looks (a subject cut out in one ink, a solid figure on a
lattice of dots, the retro palette row) each come from a bundled recipe in
one click. The milestones that got here were merged but not released on
their own; their notes follow.

### Added

- The bundled recipes are built into the app: a Recipes row at the top of
  the sidebar, and a `Recipe:` command for each. Two new ones: CMYK print
  and Braille (nine in all).
- Tests that develop a figure on a backdrop through the Cut-out and Lattice
  recipes and check the result: clean paper with one ink, and a regular dot
  lattice around a solid figure.

### 0.7: the long tail

#### Added

- Nine algorithms (twenty-nine in all): Riemersma (error diffusion along a
  Hilbert curve), Knuth's dot diffusion, Knoll and Yliluoma pattern
  dithers, a CMYK halftone with screen angles, crosshatch, engraving lines,
  weighted-Voronoi stippling, and a custom threshold tile imported from a
  small greyscale picture.
- Linear-light colour matching.
- Palette extraction: the photo's own 2 to 32 colours by k-means in Oklab.
- Video in (MP4, MOV, WebM, MKV, AVI) and MP4 out, through ffmpeg when it's
  installed; `.mp4` on the command line and in batches.

### 0.6: the text-art engine

#### Added

- Glyph sets beyond characters: braille (2×4 dots a cell), half blocks,
  quadrants, each dithered at sub-cell resolution with any algorithm; and
  colour half blocks (`▀` with a palette colour above and below).
- Cell colour: ink, the photo's own colours, or the nearest palette colour.
- Exports: ANSI with 24-bit colour, HTML (colours as shared classes), SVG
  (runs stretched to the cell grid, so any monospace font lines up), and a
  PNG of braille and block art.
- The ASCII film: an animation's text art as one HTML page that plays
  itself; ASCII mode plays animations in the app too.
- Braille and block art preview as pixels, so missing glyphs in the display
  face can't misalign the grid.
- The command line writes `.ans`, `.html` and `.svg`.

### 0.5: masks and layers

#### Added

- Masks: brightness, colour (click the photo to pick), border (automatic
  cut-out from a plain backdrop) and painted, with feather and invert.
  Soft edges are dithered with blue noise.
- Layers: the background becomes the same dither, bare paper, or its own
  algorithm, strength, threshold, shape, gutter, size by tone and lattice.
- The Mask view: the photo with the background dimmed; paint on it, or
  click to pick the key colour.
- Painted masks export and import as greyscale PNGs; `--mask` on the
  command line.
- Two recipes for the reference looks: `cutout.toml` and `lattice.toml`.

### 0.4: animation, batch and the command line

#### Added

- Animated GIF, APNG and WebP in: decoded a frame at a time, shrunk as they
  arrive, played in the print at their own timing.
- A timeline: play/pause (Space), a frame scrubber, step with `,` and `.`.
- Temporal stability, inside error diffusion: still areas stop shimmering,
  and nothing a moving object leaves behind gets frozen.
- Speed, for playback and export.
- Export an animation as a GIF in the palette's exact colours (transparent
  paper included), an APNG, a sprite sheet, or the frame on show.
- Batch develop a folder, from the command palette.
- The command line: `ferrite-darkroom IN -o OUT [--recipe R] [--scheme K]
  [--light|--dark]` and `--batch DIR`, with no window.

### 0.3: render and compare

#### Added

- Pixels as shapes: square, circle, diamond or plus, with a gutter, size by
  tone, a choice of paper colour, transparent paper (PNG alpha), and a dot
  lattice over the bare paper.
- Compare: the photo and the art either side of an adjustable split.
- Contact sheet: the print through all twenty algorithms, or every palette;
  click a tile to use it.
- Undo and redo for every recipe change (Ctrl+Z, Ctrl+Shift+Z, Ctrl+Y).
- A fifth example recipe, `recipes/dots.toml`.

#### Fixed

- A recipe or settings file saved with a UTF-8 byte-order mark (Notepad,
  PowerShell) lost its first line.

### 0.2: algorithms, palettes and recipes

#### Added

- Recipes: save how a print develops as a TOML file (Ctrl+S) and open it
  again (Ctrl+Shift+O, or drop it on the print). Four example recipes ship in
  `recipes/`.
- Custom palettes: import Lospec `.hex`, GIMP `.gpl`, JASC `.pal`,
  paint.net `.txt` or a palette image, or paste hex colours.
- A threshold (bias) control, and a new seed for the random algorithm.

- Twenty dither algorithms in Darkroom's own engine: eleven error-diffusion
  kernels (Floyd–Steinberg, Atkinson, Jarvis–Judice–Ninke, Stucki, Burkes,
  Sierra ×3, False Floyd–Steinberg, Shiau–Fan ×2) with a serpentine switch,
  and nine ordered ones (Bayer 2/4/8/16, blue noise, interleaved gradient
  noise, halftone dot, random, threshold).
- Strength: how much error is carried, or how far the threshold map reaches.
- Colour. Photos keep their colour, and develop into the scheme's ink or one
  of thirteen preset palettes (Black & white, RGB, CMYK, 3-bit, Game Boy,
  Teletext, Apple II, Commodore 64, ZX Spectrum, 6-bit RGB, 2-bit grey,
  Vaporwave, Hacker), chosen from a preset row with a swatch strip.
- Colour matching in Oklab or RGB; the photo's tones are fitted to the
  palette's lightness range.
- Gamma in the tone controls.
- `docs/ROADMAP.md`: the plan to grow Darkroom into a full dither and ASCII
  studio.

#### Changed

- `pattern` in the settings file is now `algorithm`; 0.1 files still load.
- The settings file holds the window's look plus a recipe; inline `# comments`
  (as in the README's example) no longer end up in the values.
- PNG exports are named after the algorithm and palette.

## [0.1.1] - 2026-10-07

### Fixed

- Windows: the Open button in the title bar does something when clicked. The
  whole title bar was a window-drag area, so Windows took the click as the
  start of a drag (ferrite-design's `title_bar`).
- The date rolls over at local midnight, not UTC midnight.

## [0.1.0] - 2026-10-07

The first release.

### Added

- Drop a photo, open one, or pass a path; a sample print to start with.
- Dither with ferrite-design's Bayer, blue-noise and Atkinson masks, at a
  preview that is pixel-for-pixel the export.
- ASCII with all thirteen character sets, fitted by shape or tone.
- Brightness, contrast and invert; the scheme's colours as paper and ink.
- Export PNG (1×–8×) or text, or copy the ASCII.
- The shared look from Lodestone (`FERRITE_*`) when launched from it.
- The pixel-art logo as the Windows executable, window and taskbar icon.

[1.2.2]: https://github.com/rwetz/ferrite-darkroom/releases/tag/v1.2.2
[1.2.1]: https://github.com/rwetz/ferrite-darkroom/releases/tag/v1.2.1
[1.2.0]: https://github.com/rwetz/ferrite-darkroom/releases/tag/v1.2.0
[1.1.0]: https://github.com/rwetz/ferrite-darkroom/releases/tag/v1.1.0
[1.0.0]: https://github.com/rwetz/ferrite-darkroom/releases/tag/v1.0.0
[0.1.1]: https://github.com/rwetz/ferrite-darkroom/releases/tag/v0.1.1
[0.1.0]: https://github.com/rwetz/ferrite-darkroom/releases/tag/v0.1.0
