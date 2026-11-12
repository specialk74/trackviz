//! The command line: options, what they ask for, and the commands.
//!
//! - `files`: scene files, loading a scene;
//! - `video`, `raster`: rendering frames and videos;
//! - `images`: layout image and contact sheet;
//! - `error_video`: the video of a scene that breaks, up to where it breaks;
//! - `new_scene`, `sort_scene`, `index_page`: the other commands.

mod error_video;
mod files;
mod images;
mod index_page;
mod new_scene;
mod raster;
mod sort_scene;
mod video;

pub(crate) use files::{load, scene_doc};
use error_video::*;
use files::*;
use images::*;
use index_page::*;
use new_scene::*;
use raster::*;
use sort_scene::*;
use video::*;

use crate::render;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// Version and repository from the package manifest.
const VERSION: &str = env!("CARGO_PKG_VERSION");

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

fn usage() -> String {
    format!(
        "trackviz {VERSION}\n\
         {REPOSITORY}\n\
         usage: trackviz <scene.mmd>... [-o out.mp4] [-j jobs] [--still SECONDS [--svg]] [--check]\n\
         \x20                [--from SECONDS] [--to SECONDS] [--preview] [--watch] [--layout] [--sheet]\n\
         \x20 a scene is a Mermaid diagram with %% directives; its common/ folder is next to it or in scenes/\n\
         \x20 scene files may use the wildcards * and ? (e.g. \"scenes/*.mmd\"): they are rendered in parallel\n\
         \x20 without -o the video is written to out/<scene name>.mp4 (-o and --still take a single scene)\n\
         \x20 --from/--to render only that part, --preview at half size and 15 fps: both write\n\
         \x20 out/<scene name>_preview.mp4, so the full video is kept\n\
         \x20 --watch repeats the command when the scene or a file it uses changes (one scene)\n\
         \x20 a full video also writes out/<scene name>_layout.png: the names of the points and devices\n\
         \x20 --layout writes only that image, without the video\n\
         \x20 --sheet writes out/<scene name>_sheet.png: one frame per caption, at its end, to check a scene at a glance\n\
         \x20 trackviz new <scene> [--geometry name] [--carriers n] writes the skeleton of a new scene\n\
         \x20 trackviz index [<scene>...] [-o out/index.html] writes the page that lists the videos (default all of scenes/)\n\
         \x20 trackviz sort <scene>... [--dry-run] rewrites a scene with its steps in time order (the video stays the same)"
    )
}

pub fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "new") {
        return new_cmd(&args[1..]);
    }
    if args.first().is_some_and(|a| a == "index") {
        return index_cmd(&args[1..]);
    }
    if args.first().is_some_and(|a| a == "sort") {
        return sort_cmd(&args[1..]);
    }
    let mut scene_args = Vec::new();
    let mut out = None;
    let mut still: Option<f64> = None;
    let mut svg_out = false;
    let mut layout = false;
    let mut sheet = false;
    let mut check = false;
    let mut watch = false;
    let mut opts = VideoOpts::default();
    let mut jobs = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" => {
                i += 1;
                out = args.get(i).cloned();
            }
            "-j" => {
                i += 1;
                jobs = args.get(i).and_then(|s| s.parse().ok()).ok_or("-j needs a number")?;
            }
            "--still" => {
                i += 1;
                still = Some(args.get(i).and_then(|s| s.parse().ok()).ok_or("--still needs seconds")?);
            }
            "--from" => {
                i += 1;
                opts.from = Some(args.get(i).and_then(|s| s.parse().ok()).ok_or("--from needs seconds")?);
            }
            "--to" => {
                i += 1;
                opts.to = Some(args.get(i).and_then(|s| s.parse().ok()).ok_or("--to needs seconds")?);
            }
            "--preview" => opts.preview = true,
            "--watch" => watch = true,
            "--svg" => svg_out = true,
            "--check" => check = true,
            "--layout" => layout = true,
            "--sheet" => sheet = true,
            "-h" | "--help" => {
                println!("{}", usage());
                return Ok(());
            }
            a if !a.starts_with('-') => scene_args.push(a.to_string()),
            a => return Err(format!("unexpected argument '{a}'")),
        }
        i += 1;
    }
    if scene_args.is_empty() {
        return Err(format!("missing scene file\n{}", usage()));
    }
    let mut files = Vec::new();
    for a in &scene_args {
        for f in expand(a)? {
            if !files.contains(&f) {
                files.push(f);
            }
        }
    }
    if layout && (check || still.is_some() || opts.partial() || sheet) {
        return Err("--layout writes only the layout image: not with --check, --still, --from, --to, --preview or --sheet".into());
    }
    if sheet && (check || still.is_some() || opts.partial()) {
        return Err("--sheet writes only the contact sheet: not with --check, --still, --from, --to or --preview".into());
    }
    if files.len() > 1 && (out.is_some() || still.is_some() || watch) {
        return Err(format!("-o, --still and --watch take a single scene, but {} were given", files.len()));
    }
    if opts.partial() && (check || still.is_some()) {
        return Err("--from, --to and --preview are for videos: not with --check or --still".into());
    }
    let job = Job { out, still, layout, sheet, svg_out, check, jobs: jobs.max(1), opts };
    if watch {
        watch_scene(&files[0], &job);
    }
    job.run(&files)
}

/// What the command line asks for.
struct Job {
    out: Option<String>,
    still: Option<f64>,
    /// Only the layout image (`--layout`).
    layout: bool,
    /// Only the contact sheet (`--sheet`).
    sheet: bool,
    svg_out: bool,
    check: bool,
    jobs: usize,
    opts: VideoOpts,
}

impl Job {
    fn run(&self, files: &[String]) -> Result<(), String> {
        if self.check {
            let mut failures = Vec::new();
            for f in files {
                if files.len() > 1 {
                    eprintln!("== {f}");
                }
                match load(f) {
                    Ok((sc, tl)) => eprint!("{}", timeline_text(f, &sc, &tl)),
                    Err(e) if files.len() == 1 => return Err(e),
                    Err(e) => failures.push(e),
                }
            }
            return report_failures(failures, files.len());
        }
        if self.layout || self.sheet {
            // One image per scene, next to where its video would go.
            let mut failures = Vec::new();
            for f in files {
                let res = load(f).and_then(|(sc, tl)| {
                    let path = match &self.out {
                        Some(o) => o.clone(),
                        None => {
                            std::fs::create_dir_all("out").map_err(|e| format!("cannot create out/: {e}"))?;
                            let video = default_out(f, VideoOpts::default());
                            if self.sheet { sheet_path(&video) } else { layout_path(&video) }
                        }
                    };
                    if self.sheet {
                        write_sheet(&sc, &tl, &path, self.jobs)
                    } else {
                        write_layout(&sc, &tl, &path)
                    }
                });
                match res {
                    Ok(()) => {}
                    Err(e) if files.len() == 1 => return Err(e),
                    Err(e) => failures.push(e),
                }
            }
            return report_failures(failures, files.len());
        }
        if files.len() > 1 {
            return render_many(files, self.opts, self.jobs);
        }

        let scene_path = &files[0];
        let (sc, tl) = match load(scene_path) {
            Ok(r) => r,
            Err(e) => {
                if self.still.is_none() {
                    render_until_error(scene_path, &e, self.out.as_deref(), self.opts, self.jobs);
                }
                return Err(e);
            }
        };
        eprint!("{}", timeline_text(scene_path, &sc, &tl));
        if let Some(t) = self.still {
            let (w, h) = (sc.video.width, sc.video.height);
            let svg = render::frame(&sc, &tl, t);
            let path = self.out.clone().unwrap_or_else(|| if self.svg_out { "frame.svg".into() } else { "frame.png".into() });
            if self.svg_out {
                std::fs::write(&path, svg).map_err(|e| e.to_string())?;
            } else {
                let png = rasterize(&raster_options(), &svg, w, h)?.encode_png().map_err(|e| e.to_string())?;
                std::fs::write(&path, png).map_err(|e| e.to_string())?;
            }
            eprintln!("wrote {path}");
            return Ok(());
        }
        let out = match &self.out {
            Some(o) => o.clone(),
            None => {
                std::fs::create_dir_all("out").map_err(|e| format!("cannot create out/: {e}"))?;
                default_out(scene_path, self.opts)
            }
        };
        render_video(&sc, &tl, &out, self.opts, self.jobs, &raster_options(), true)?;
        eprintln!("wrote {out}");
        Ok(())
    }
}

/// Modification times of the scene and of everything it includes.
fn stamps(files: &[PathBuf]) -> Vec<Option<SystemTime>> {
    files.iter().map(|f| std::fs::metadata(f).and_then(|m| m.modified()).ok()).collect()
}

/// Runs the job, then again every time a file changes, until the user stops it.
/// A broken scene only prints its error: it can be fixed while watching.
fn watch_scene(path: &str, job: &Job) -> ! {
    let mut files = vec![PathBuf::from(path)];
    loop {
        if let Ok(doc) = scene_doc(path) {
            files = doc.files;
        }
        let seen = stamps(&files);
        if let Err(e) = job.run(&[path.to_string()]) {
            eprintln!("error: {e}");
        }
        eprintln!("watching {} file(s), Ctrl-C to stop", files.len());
        while stamps(&files) == seen {
            std::thread::sleep(Duration::from_millis(300));
        }
    }
}

#[cfg(test)]
mod tests;
