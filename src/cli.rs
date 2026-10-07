//! Darkroom without a window:
//!
//!     ferrite-darkroom photo.jpg -o print.png
//!     ferrite-darkroom loop.gif -o loop.gif --recipe recipes/gameboy.toml
//!     ferrite-darkroom --batch ./photos -o ./prints --recipe newsprint.toml
//!
//! Without `--recipe`, the recipe last used in the app is used. The scheme
//! and appearance (for the Scheme palette and ASCII) come from the app's
//! settings too, unless `--scheme` / `--light` / `--dark` say otherwise.

use std::path::PathBuf;

use crate::batch::Job;
use crate::recipe::Recipe;
use crate::settings::Settings;

const USAGE: &str = "\
Darkroom: dither photos and animations into pixel art.

  ferrite-darkroom [photo]                     open the app (with a photo, GIF or recipe)
  ferrite-darkroom IN -o OUT [options]         develop one file, no window
  ferrite-darkroom --batch DIR [-o OUTDIR]     develop every picture in DIR
                                               (into DIR/darkroom unless -o)

OUT's extension picks the format: .png (an APNG for an animation), .gif, .mp4 (needs
ffmpeg), or text art:
.txt, .ans (ANSI colour), .html (a film for an animation), .svg.

Options:
  --recipe FILE     a recipe saved from the app (default: the app's last recipe)
  --mask FILE       a painted mask (greyscale PNG, light = subject), for
                    recipes with mask = painted
  --scheme KEY      the scheme for the Scheme palette: ferrite mono graphite slate
                    concrete harbor cyanotype phosphor verdigris bruise
  --light, --dark   the scheme's appearance
  -h, --help        this
";

/// What the command line asked for, or `None` to open the app.
#[derive(Debug, PartialEq)]
enum Command {
    Help,
    File { input: PathBuf, output: PathBuf, opts: Opts },
    Folder { dir: PathBuf, output: Option<PathBuf>, opts: Opts },
}

#[derive(Debug, Default, PartialEq)]
struct Opts {
    recipe: Option<PathBuf>,
    mask: Option<PathBuf>,
    scheme: Option<String>,
    light: Option<bool>,
}

fn parse(args: &[String]) -> Result<Option<Command>, String> {
    let (mut input, mut output, mut batch, mut opts) = (None, None, None, Opts::default());
    let mut it = args.iter();
    let value = |it: &mut std::slice::Iter<String>, flag: &str| it.next().cloned().ok_or(format!("{flag} needs a value"));
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Some(Command::Help)),
            "-o" | "--output" => output = Some(PathBuf::from(value(&mut it, arg)?)),
            "--batch" => batch = Some(PathBuf::from(value(&mut it, arg)?)),
            "--recipe" => opts.recipe = Some(PathBuf::from(value(&mut it, arg)?)),
            "--mask" => opts.mask = Some(PathBuf::from(value(&mut it, arg)?)),
            "--scheme" => opts.scheme = Some(value(&mut it, arg)?),
            "--light" => opts.light = Some(true),
            "--dark" => opts.light = Some(false),
            flag if flag.starts_with('-') => return Err(format!("unknown option {flag} (try --help)")),
            path if input.is_none() => input = Some(PathBuf::from(path)),
            extra => return Err(format!("one input at a time ({extra}); use --batch for a folder")),
        }
    }
    Ok(match (batch, input, output) {
        (Some(dir), None, output) => Some(Command::Folder { dir, output, opts }),
        (Some(_), Some(_), _) => return Err("--batch takes a folder instead of an input file".into()),
        (None, Some(input), Some(output)) => Some(Command::File { input, output, opts }),
        (None, None, Some(_)) => return Err("-o needs an input file".into()),
        // No output: open the app (with the file, if one was given).
        (None, _, None) => None,
    })
}

fn job(opts: &Opts) -> Result<Job, String> {
    let settings = Settings::load();
    let recipe = match &opts.recipe {
        Some(path) => Recipe::from_toml(&std::fs::read_to_string(path).map_err(|e| format!("couldn't read {}: {e}", path.display()))?)?,
        None => settings.recipe.clone(),
    };
    let scheme = opts.scheme.clone().unwrap_or(settings.scheme);
    let light = opts.light.unwrap_or(settings.appearance == "light");
    let mut job = Job::with_scheme(recipe, &scheme, light);
    if let Some(path) = &opts.mask {
        let img = image::open(path).map_err(|e| format!("couldn't read the mask {}: {e}", path.display()))?.to_luma8();
        job.paint = Some(crate::mask::Paint::from_image(&img, img.width(), img.height()));
    }
    Ok(job)
}

/// Run a command-line job if one was asked for. `None` means open the app.
pub fn run(args: &[String]) -> Option<i32> {
    let command = match parse(args) {
        Ok(None) => return None,
        Ok(Some(c)) => c,
        Err(why) => {
            attach_console();
            eprintln!("darkroom: {why}");
            return Some(2);
        }
    };
    attach_console();
    let result = match command {
        Command::Help => {
            print!("{USAGE}");
            Ok(())
        }
        Command::File { input, output, opts } => job(&opts).and_then(|job| job.develop_file(&input, &output)).map(|()| println!("{}", output.display())),
        Command::Folder { dir, output, opts } => job(&opts).and_then(|job| {
            let out = output.unwrap_or_else(|| dir.join("darkroom"));
            let (done, failed) = job.develop_folder(&dir, &out)?;
            for path in &done {
                println!("{}", path.display());
            }
            for (path, why) in &failed {
                eprintln!("darkroom: {}: {why}", path.display());
            }
            if done.is_empty() && failed.is_empty() {
                return Err(format!("no pictures in {}", dir.display()));
            }
            if failed.is_empty() { Ok(()) } else { Err(format!("{} of {} failed", failed.len(), done.len() + failed.len())) }
        }),
    };
    match result {
        Ok(()) => Some(0),
        Err(why) => {
            eprintln!("darkroom: {why}");
            Some(1)
        }
    }
}

/// Release builds on Windows have no console of their own; borrow the one
/// we were started from so output and errors show up there.
fn attach_console() {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn AttachConsole(process_id: u32) -> i32;
        }
        const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
        // Fails harmlessly when there's already a console (debug builds).
        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn no_output_opens_the_app() {
        assert_eq!(parse(&args("")), Ok(None));
        assert_eq!(parse(&args("photo.jpg")), Ok(None));
    }

    #[test]
    fn one_file() {
        let c = parse(&args("in.gif -o out.gif --recipe gb.toml --scheme phosphor --light")).unwrap().unwrap();
        assert_eq!(
            c,
            Command::File {
                input: "in.gif".into(),
                output: "out.gif".into(),
                opts: Opts { recipe: Some("gb.toml".into()), mask: None, scheme: Some("phosphor".into()), light: Some(true) }
            }
        );
    }

    #[test]
    fn a_folder() {
        let c = parse(&args("--batch pics")).unwrap().unwrap();
        assert_eq!(c, Command::Folder { dir: "pics".into(), output: None, opts: Opts::default() });
    }

    #[test]
    fn mistakes_are_explained() {
        assert!(parse(&args("-o out.png")).unwrap_err().contains("input"));
        assert!(parse(&args("a.png b.png -o c.png")).unwrap_err().contains("one input"));
        assert!(parse(&args("a.png --wat")).unwrap_err().contains("--wat"));
        assert!(parse(&args("a.png -o")).unwrap_err().contains("needs a value"));
        assert_eq!(parse(&args("--help")), Ok(Some(Command::Help)));
    }
}
