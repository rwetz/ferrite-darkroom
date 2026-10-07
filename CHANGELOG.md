# Changelog

All notable changes to Darkroom are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

The first part of the 0.2 milestone in [the roadmap](docs/ROADMAP.md).

### Added

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

### Changed

- `pattern` in the settings file is now `algorithm`; 0.1 files still load.
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

[0.1.1]: https://github.com/rwetz/ferrite-darkroom/releases/tag/v0.1.1
[0.1.0]: https://github.com/rwetz/ferrite-darkroom/releases/tag/v0.1.0
