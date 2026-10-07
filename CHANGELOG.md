# Changelog

All notable changes to Darkroom are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

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
