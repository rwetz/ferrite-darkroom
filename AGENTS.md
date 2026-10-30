# Darkroom

A native desktop app on [ferrite-design](https://github.com/rwetz/ferrite-design)
(GPUI, Rust), started from the `minimal` template.

Before changing UI code, read ferrite-design's AGENTS.md — it is the
contract for how Ferrite apps are built (components, colors, type, motion,
and the mistakes that cost time). Short version:

- `use ferrite_design::prelude::*;` — every component comes from there.
- Colors only through `palette(cx)`; no hex values in views.
- Display type via `.display(Scale::X1, window)`, UPPERCASE; body via `.body(text::BASE)`.
- Controlled components: you own the value, the handler gets the new one.
- Check: let CI run build and clippy, then run the release `.exe` (no local cargo builds; see Workspace conventions).

## Workspace conventions

Added by the 2026-10-30 workspace audit. Workspace-level rules live in `Dev/AGENTS.md`; this section only records how they apply here.

- **Registry.** This project has a row in `Dev/PROJECTS.md`. Update it in the same change as any move, rename or archive.
- **File header.** New source files a human authors start with the Ryan Wetzstein box (project line: `Darkroom`, year 2026) in the language's comment syntax, above imports and below any shebang. Skip generated files, lockfiles, JSON, Markdown and vendored code.
- **Builds.** Do not run `cargo build`, `cargo run` or `cargo clippy` locally: GPUI compiles overheat the user's CPU. Let CI build, then test the release `.exe`. This overrides any local build or check command above.
- **CI.** Linux-only, `pull_request` and `main` pushes, `concurrency` with `cancel-in-progress`, `paths-ignore` for docs (start from `_templates/workflows/ci-rust.yml`).
- **README.** Ferrite style, modeled on `ferrite/ferrite-wireless/README.md`: centered 128px `assets/logo.svg`, badge row, real `docs/main.png` screenshot, `<kbd>` keycaps, centered heptagram footer.
- **Jev.** Bounded judgments (pick one, score, yes/no) go through the TypeSafe skill or the contract in `apps/job-app-finder/src/job_app_finder/jev_fit.py`. Jev only ranks, routes or flags; irreversible actions stay behind human confirmation.
