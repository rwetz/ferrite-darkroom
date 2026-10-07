# Roadmap

The plan for growing Darkroom into a full dither and ASCII studio. It started
as the Darkroom 2 section of ferrite-design's
[IDEAS.md](https://github.com/rwetz/ferrite-design/blob/main/docs/IDEAS.md)
(2026-10-07); this copy is the one to keep up to date.

Darkroom today: one still image, three patterns (Bayer, blue noise,
Atkinson), thirteen ASCII sets, brightness/contrast/invert, two-tone ink
from the scheme, PNG/TXT export. ~1,100 lines. The goal is a tool for
making art like the reference images: a knight dithered in one ink on
white with the background gone, a hand cut out in black against a lattice
of dots, and Game Boy / C64 / Teletext palette looks from a preset row.

## What the reference images actually need

- **Duotone ink on paper** with any two colours, not just scheme colours
  (blue knight on white).
- **Subject isolation**: background removed so it becomes clean paper.
- **Cell rendering**: each art pixel drawn as a shape (square, round dot,
  plus) with a gutter, so the result reads as a lattice (the hand).
- **Different treatment per region**: subject in error diffusion, the
  background as an ordered dot field.
- **Palette presets**: one click to B&W, RGB, CMYK, 3-bit, Game Boy,
  Teletext, Apple II, C64, ZX Spectrum, 6-bit RGB, 2-bit grey,
  Vaporwave, Hacker.

## Architecture: a short pipeline, not a filter pile

```
Source ─▶ Adjust ─▶ Mask ─▶ Dither / ASCII ─▶ Render ─▶ Composite ─▶ Export
(image,   (levels,  (subject, (algorithm,       (cell shape,  (layers,   (PNG, GIF,
 GIF,      curves,   luma,     palette,          gutter,       blend)     WebP, MP4,
 video,    blur,     painted)  strength)         scale)                   SVG, TXT,
 webcam)   sharpen)                                                       ANSI, HTML)
```

Each stage is a pure function in `studio/` with tests, as `studio.rs` is
today. The view only edits a `Recipe` (a serialisable struct of every
stage's settings). A recipe *is* a preset: save it as TOML, share it,
apply it from the CLI. One or more **layers**, each a recipe with a
mask, composite into the final print; the default is one layer.

## Algorithms (the catalogue)

Every algorithm takes a palette (2..256 colours) and a **strength**.

| Family | Algorithms |
|---|---|
| Threshold | Fixed, adaptive (local mean), random (white noise) |
| Ordered | Bayer 2×2 / 4×4 / 8×8 / 16×16, blue noise (void-and-cluster), interleaved gradient noise, clustered-dot halftone, line screen, crosshatch, custom tile (paint or import a matrix) |
| Halftone | Dot, line, ellipse, square, cross; per-channel screen angles for CMYK |
| Error diffusion | Floyd–Steinberg, False Floyd–Steinberg, Jarvis–Judice–Ninke, Stucki, Burkes, Sierra / Two-row / Lite, Atkinson, Shiau–Fan, Ostromoukhov (variable coefficients), Zhou–Fang; serpentine scan toggle |
| Path / space-filling | Riemersma (Hilbert curve), dot diffusion (Knuth) |
| Multi-colour ordered | Yliluoma 1 / 2, Knoll (Photoshop-style pattern dither) |
| Stippling | Weighted Voronoi stipple (dots that cluster with tone), line hatching / engraving |
| ASCII | Tone ramps, glyph-shape matching (chafa-style, rasterised from PxPlus CP437), braille 2×4, half/quarter blocks, sextants, edge-direction glyphs (`/ \ | _ -`), colour per cell (fg + bg) |

Error-diffusion kernels are tables, so most of the list is data, not code.
Ship them in waves (see milestones), each with a gallery tile.

## Controls ("intensity" and friends)

- **Strength**: error-diffusion coefficient scale (0 = posterise, 1 =
  classic, >1 = crunchy), or ordered-matrix amplitude.
- **Pixel size / width**: the art grid (today's `cols`), with a lock to
  the source aspect.
- **Threshold bias**, **gamma**, **levels**, **curves**, **posterise**,
  **pre-blur**, **sharpen / unsharp**, **edge boost** (adds linework
  before dithering, gives engravings bite).
- **Colour space**: dither in sRGB or linear light; match colours by RGB,
  Oklab, or CIEDE2000. Oklab as the default; linear light is the
  "correct" setting that people will toggle off for a punchier look.
- **Seed** for anything random, so an export is repeatable.

## Palettes

- The 13 presets from the reference row, plus every Ferrite scheme.
- Import Lospec palettes (`.hex`, `.gpl`, `.pal`), paste hex lists.
- Extract a palette from the image (median cut, k-means in Oklab), with
  a count slider.
- A palette editor: reorder, lock, pick ink/paper; "duotone" mode is just
  a 2-colour palette with a one-click ink swap.

## Render (how each art pixel is drawn)

- Cell shape: square, circle, diamond, plus, glyph.
- Gutter between cells, and cell size modulated by tone (halftone without
  a halftone algorithm).
- Transparent paper for PNG/WebP/SVG export, so the art drops onto
  anything.
- Background field: fill the paper with a pattern (dots, Bayer at a fixed
  level, scanlines) behind the subject. This is the hand image.

## Masks and subject isolation

- Luma key and colour key (cheap, always available).
- Paint mask with a brush, feathered by dither rather than blur.
- Automatic subject segmentation via a small local ONNX model (U²-Net /
  BiRefNet class) behind a feature flag. Offline, nothing uploaded.
- A mask chooses which layer applies where: subject → Floyd–Steinberg in
  blue, background → dot field, as in the references.

## Animation: GIFs and video

- **Import** GIF, APNG, animated WebP; video and webcam later via ffmpeg.
- **Timeline** with a frame scrubber, per-frame preview, playback at the
  source rate; trim, loop, ping-pong, fps change.
- **Temporal stability** is the hard part. Error diffusion shimmers from
  frame to frame. Offer: ordered/blue-noise (stable by nature), error
  diffusion seeded from the previous frame's result where the source
  didn't change, and a stability slider. Read the *Obra Dinn* devlog
  first (ferrite-design's [REFERENCES.md](https://github.com/rwetz/ferrite-design/blob/main/docs/REFERENCES.md)).
- **Animated effects** using Ferrite's motion vocabulary: develop-in,
  scanline reveal, dither crawl, glitch, decrypt for ASCII.
- **Export** GIF (palette-exact, no requantisation since we already
  chose the palette), APNG, WebP, MP4, sprite sheet, and ASCII film
  (`ascii_film` format and an ANSI-escape player script).

## Workflow

- **Before/after** split slider on the print.
- **Contact sheet**: one image through N algorithms or palettes in a grid,
  click one to adopt it. The fastest way to explore 40 algorithms.
- Preset row (like the reference), recipe save/load, recent recipes.
- Undo/redo over the recipe (cheap: it's a small struct).
- Batch: a folder in, the same recipe applied, a folder out.
- CLI: `ferrite-darkroom --recipe gameboy.toml in.gif -o out.gif`,
  headless, for scripts and Teletype.
- Zoom and pan the print at integer zoom, with a pixel grid at high zoom.

## Performance

- Ordered, threshold and halftone are per-pixel independent: run them in
  parallel (rayon) and they're instant at any size.
- Error diffusion is serial by nature; parallelise across rows with the
  standard wavefront (row *n* starts once row *n−1* is two pixels ahead),
  and across frames for GIFs.
- Preview at screen resolution while dragging, full resolution on
  release and on export. A stepped "developing" progress bar, never a
  spinner over a frozen print.
- Keep the GPU path (compute shaders) as a later option; gpui doesn't
  expose compute today.

## Milestones

Progress is marked inline: ✅ done, everything else still to do.

1. **0.2: algorithms and palettes.** ✅ Error-diffusion kernel table
   (FS, False FS, JJN, Stucki, Burkes, Sierra ×3, Atkinson, Shiau–Fan ×2),
   serpentine; ✅ Bayer 2–16, blue noise, gradient noise, halftone dot,
   random, threshold; ✅ strength, gamma; ✅ the 13 preset palettes and the
   preset row; ✅ Oklab / RGB matching; ✅ the `Recipe` (src/recipe.rs) with
   TOML save/load and four bundled recipes; ✅ threshold bias; ✅ Lospec
   import (.hex, .gpl, .pal, .txt, palette images, pasted hex). *Moved on:*
   linear-light dithering and palette extraction from the photo join 0.7.
   Original scope: Pipeline refactor into `Recipe`;
   error-diffusion kernel table (FS, JJN, Stucki, Burkes, Sierra ×3,
   Atkinson, Shiau–Fan); Bayer 2–16; strength, gamma, threshold;
   multi-colour palettes with the 13 presets; Oklab matching; preset row;
   recipe TOML.
2. **0.3: render and compare.** ✅ Cell shapes (square, circle, diamond,
   plus), gutter, size by tone, paper choice, transparent paper, a dot
   lattice; ✅ compare (split, set by a slider; dragging on the print itself
   is still to do); ✅ contact sheet of algorithms and palettes; ✅ undo/redo.
   Original scope: Cell shapes, gutter, transparent paper,
   background field; before/after; contact sheet; undo.
3. **0.4: GIF.** ✅ GIF / APNG / animated WebP in, a timeline, temporal
   stability (inside error diffusion, so nothing gets frozen), speed;
   ✅ GIF (exact palette) / APNG / sprite-sheet export; ✅ batch; ✅ CLI.
   *Not done:* animated WebP export (the `image` crate only writes still
   WebP; it needs libwebp, which isn't worth a C dependency yet).
   Original scope: Import, timeline, temporal stability, GIF/APNG/WebP
   export, batch, CLI.
4. **0.5: masks and layers.** ✅ Brightness / colour key (eyedropper),
   ✅ border flood (automatic cut-out from a plain backdrop, which covers
   the knight reference without a model), ✅ painted mask with PNG
   import/export and `--mask`; ✅ feather and invert, dithered edges;
   ✅ a background layer with its own treatment (same, paper, or own
   algorithm and cells). *Narrowed:* two layers (subject and background)
   rather than any number. *Deferred past 1.0:* ONNX subject segmentation:
   it needs onnxruntime (a large native library) and a model download,
   and it can't be verified without a heavy local build; the border flood
   covers plain backdrops, painting covers the rest.
   Original scope: Luma/colour key, paint mask, layers with
   per-layer recipes; then ONNX subject segmentation.
5. **0.6: ASCII engine.** Glyph-shape matching on PxPlus CP437, braille,
   blocks, colour ASCII, ANSI/HTML/SVG export, ASCII film.
6. **0.7: the long tail.** Halftone screens with CMYK angles, Riemersma,
   dot diffusion, Yliluoma, Knoll, Ostromoukhov, stippling, hatching,
   custom tiles, palette extraction; video and webcam via ffmpeg.
7. **1.0** when the reference images can each be reproduced from a
   bundled recipe in under a minute.

## What moves into ferrite-design

Darkroom proves things the crate should own. When an algorithm is used by
a second app (Fluoroscope's weight images, Teletype's export), lift it
from Darkroom into `ferrite_design::dither`, keeping the 4×4 Bayer as the
UI signature (ferrite-design's REFERENCES.md, "Ideas parked").
