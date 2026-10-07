<p align="center">
  <img src="assets/logo.svg" width="128" height="128" alt="Darkroom logo: a framed photo developing from dither under an amber safelight">
</p>

<h1 align="center">Darkroom</h1>

<p align="center">
  A <a href="https://github.com/rwetz/ferrite-design">Ferrite</a> dither studio:
  turn a photo into pixel art or ASCII with Ferrite's own patterns, and export it.
</p>

<p align="center">
  <a href="https://github.com/rwetz/ferrite-darkroom/actions/workflows/ci.yml"><img src="https://github.com/rwetz/ferrite-darkroom/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/rwetz/ferrite-darkroom/releases/latest"><img src="https://img.shields.io/github/v/release/rwetz/ferrite-darkroom?color=F2A93B&labelColor=18181B" alt="Latest release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-3D3D42?labelColor=18181B" alt="License: Apache-2.0"></a>
</p>

![Darkroom developing the sample print with Atkinson dither in amber](docs/main.png)

Darkroom turns photos into Ferrite-style art. Drop an image on the window
and develop it with the same dither masks ferrite-design draws its textures
with (ordered Bayer, blue noise and Atkinson error diffusion) or as ASCII
with its glyph-fitted character sets. The art takes the colours of whichever
Ferrite scheme you pick, so amber on iron, phosphor green or cyanotype blue
are a click apart. Export a PNG or a text file. It's how you make splash art
and icons for every other Ferrite app.

It's a native desktop app built with [GPUI](https://gpui.rs) and
[ferrite-design](https://github.com/rwetz/ferrite-design): amber on iron,
0px corners, pixel type and stepped motion. It runs no webview.

## Features

- **Bring a photo.** Drop it on the print, open one with
  <kbd>Ctrl</kbd>+<kbd>O</kbd>, or pass a path: `ferrite-darkroom photo.jpg`.
  PNG, JPEG, GIF, BMP and WebP. A built-in sample print is there to start.
- **Dither.** Bayer, blue noise or Atkinson, at 32 to 480 pixels wide. The
  preview draws whole device pixels per art pixel, so it's exactly the export.
- **ASCII.** All thirteen of ferrite-design's character sets, from classic
  `.:-=+*#%@` to box drawing and the "best character" set, fitted by shape
  (edges) or by tone (brightness), 20 to 200 characters wide.
- **Tone.** Brightness, contrast and invert.
- **Palette.** The art's paper is the scheme's background and its ink is the
  scheme's text or accent colour. Light appearance gives dark ink on light
  paper.
- **Export.** A PNG at 1×, 2×, 4× or 8× pixel size
  (<kbd>Ctrl</kbd>+<kbd>E</kbd>), the ASCII as a text file
  (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>E</kbd>), or the ASCII copied to the
  clipboard (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd>).
- **Ferrite throughout.** Ten color schemes, a command palette
  (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd>) with every pattern and
  character set in it, and a CRT switch-off when you close the window.

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

Darkroom remembers the look and the last process you used, as plain
`key = value` lines:

| OS | Settings |
|---|---|
| Windows | `%APPDATA%\ferrite\darkroom.conf` |
| macOS | `~/Library/Application Support/ferrite/darkroom.conf` |
| Linux | `$XDG_CONFIG_HOME/ferrite/darkroom.conf` (or `~/.config/…`) |

```ini
scheme = ferrite      # ferrite mono graphite slate concrete harbor cyanotype phosphor verdigris bruise
appearance = dark     # dark | light | system: the paper
mode = dither         # dither | ascii
pattern = atkinson    # bayer | blue-noise | atkinson
cols = 160            # dither width in pixels
scale = 4             # PNG pixel size
brightness = 0.00     # -1 to 1
contrast = 1.00       # 0.25 to 3
invert = false
ink = accent          # accent | text
charset = best-character
fit = shape           # shape | tone
ascii_cols = 80
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
cargo test                    # resampling, tone, every pattern, PNG export, ASCII, settings
cargo clippy --all-targets
```

- `src/studio.rs`: the whole process: loading, the working print,
  adjustments, dither masks, PNG encoding and ASCII. Pure and unit-tested;
  the preview and the exports both run it.
- `src/settings.rs`: the settings file.
- `src/main.rs`: the view. It follows ferrite-design's
  [AGENTS.md](https://github.com/rwetz/ferrite-design/blob/main/AGENTS.md).

On Windows, `build.rs` embeds `assets/logo.ico` as the executable and window
icon. The `.ico` is generated from `assets/logo.svg`, so regenerate it when
the logo changes.

## License

Apache-2.0, see [LICENSE](LICENSE). The binary embeds fonts from
ferrite-design under their own licenses; see [NOTICE](NOTICE).
