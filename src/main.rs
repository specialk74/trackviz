//! trackviz — renders scenario files (track + message sequence) to MP4.
//!
//! Usage:
//!   trackviz <scene.mmd>... [-o out.mp4] [-j jobs]   (default: out/<scene name>.mp4)
//!   trackviz <scene.mmd> --still <seconds> [-o frame.png]
//!   trackviz <scene.mmd>... --layout      (only out/<scene name>_layout.png: points and devices)
//!   trackviz <scene.mmd>... --sheet       (out/<scene name>_sheet.png: one frame per caption)
//!   trackviz new <scene> [--geometry spur2] [--carriers 3]   (the skeleton of a new scene)
//!   trackviz index [<scene.mmd>...] [-o out/index.html]   (the page that lists the videos)
//!   trackviz <scene.mmd>... --check         (validate + print the timeline)
//!   trackviz <scene.mmd> --preview --from 15 --to 25   (quick look, out/<scene name>_preview.mp4)
//!   trackviz <scene.mmd> --preview --watch  (regenerate when the scene or a file it uses changes)
//!
//! A scene is a Mermaid sequence diagram with `%%` directives for the physical side.
//! Scene arguments may contain the wildcards `*` and `?` in the file name: several
//! scenes are rendered in parallel, sharing the `-j` threads.
//!
//! Frames are rasterized in-process with resvg; only `ffmpeg` is needed on PATH
//! (and only for video output).

mod doc;
mod index;
mod physics;
mod lint;
mod mermaid;
mod render;
mod scene;
mod template;

use resvg::{tiny_skia, usvg};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Mutex};
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
         \x20 trackviz index [<scene>...] [-o out/index.html] writes the page that lists the videos (default all of scenes/)"
    )
}

/// Inter (variable font: font-weight maps to the wght axis), fetched by build.rs.
const INTER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/Inter.ttf"));

/// Rendering options shared by all frames. Fonts are loaded once (this is the
/// slow part), then the options are reused for every frame.
fn raster_options() -> usvg::Options<'static> {
    let mut opt = usvg::Options::default();
    let db = opt.fontdb_mut();
    // Both fonts are embedded in the executable, so every machine renders the
    // same frames regardless of what is installed.
    db.load_font_data(INTER.to_vec());
    db.load_font_data(dejavu::sans_mono::regular().to_vec());
    db.set_sans_serif_family("Inter");
    db.set_monospace_family("DejaVu Sans Mono");
    // System fonts only as a fallback for glyphs the bundled fonts lack.
    db.load_system_fonts();
    opt
}

/// Rasterizes one SVG frame into a w x h pixmap (premultiplied RGBA).
fn rasterize(opt: &usvg::Options, svg: &str, w: u32, h: u32) -> Result<tiny_skia::Pixmap, String> {
    let tree = usvg::Tree::from_str(svg, opt).map_err(|e| format!("invalid frame SVG: {e}"))?;
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or_else(|| format!("invalid frame size {w}x{h}"))?;
    let size = tree.size();
    let tr = tiny_skia::Transform::from_scale(w as f32 / size.width(), h as f32 / size.height());
    resvg::render(&tree, tr, &mut pixmap.as_mut());
    Ok(pixmap)
}

/// Does `name` match `pattern`? `*` stands for any text, `?` for one character.
fn wildcard_match(pattern: &[char], name: &[char]) -> bool {
    match pattern.split_first() {
        None => name.is_empty(),
        Some(('*', rest)) => (0..=name.len()).any(|k| wildcard_match(rest, &name[k..])),
        Some(('?', rest)) => !name.is_empty() && wildcard_match(rest, &name[1..]),
        Some((c, rest)) => name.first() == Some(c) && wildcard_match(rest, &name[1..]),
    }
}

/// Expands the wildcards of a scene argument (only in the file name) into the
/// matching files, sorted. An argument without wildcards is returned as it is.
fn expand(arg: &str) -> Result<Vec<String>, String> {
    if !arg.contains(['*', '?']) {
        return Ok(vec![arg.to_string()]);
    }
    let path = Path::new(arg);
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty());
    if dir.is_some_and(|d| d.to_string_lossy().contains(['*', '?'])) {
        return Err(format!("{arg}: wildcards are supported only in the file name"));
    }
    let pattern: Vec<char> = path.file_name().map(|n| n.to_string_lossy().chars().collect()).unwrap_or_default();
    let entries = std::fs::read_dir(dir.unwrap_or(Path::new("."))).map_err(|e| format!("{arg}: {e}"))?;
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("{arg}: {e}"))?;
        let name: Vec<char> = entry.file_name().to_string_lossy().chars().collect();
        if entry.path().is_file() && wildcard_match(&pattern, &name) {
            files.push(dir.map_or_else(|| entry.file_name().into(), |d| d.join(entry.file_name())).to_string_lossy().into_owned());
        }
    }
    if files.is_empty() {
        return Err(format!("{arg}: no matching files"));
    }
    files.sort();
    Ok(files)
}

/// Where a scene finds `common/`: next to it, otherwise in scenes/.
fn common_base(path: &Path) -> PathBuf {
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    if dir.join("common").is_dir() || !Path::new("scenes/common").is_dir() {
        dir.to_path_buf()
    } else {
        PathBuf::from("scenes")
    }
}

/// The data of a scene, with the files read to build it.
fn scene_doc(path: &str) -> Result<doc::Doc, String> {
    if !Path::new(path).extension().is_some_and(|e| e == "mmd") {
        return Err("a scene is a Mermaid diagram, a .mmd file".into());
    }
    mermaid::load(Path::new(path), &common_base(Path::new(path)))
}

fn load(path: &str) -> Result<(scene::Scene, scene::Timeline), String> {
    let prefix = |e: String| if e.starts_with(path) { e } else { format!("{path}: {e}") };
    let doc = scene_doc(path).map_err(prefix)?;
    let sc = scene::Scene::from_doc(&doc).map_err(prefix)?;
    let tl = scene::Timeline::build(&sc).map_err(prefix)?;
    Ok((sc, tl))
}

/// The timeline as `--check` prints it; warnings name the scene, so that an editor
/// can show them on its file.
fn timeline_text(path: &str, sc: &scene::Scene, tl: &scene::Timeline) -> String {
    let mut t = format!("Timeline ({:.1} s total):\n", tl.duration);
    for l in &tl.log {
        t.push_str(&format!("  {l}\n"));
    }
    for w in lint::warnings(sc, tl) {
        t.push_str(&format!("warning: {path}: {w}\n"));
    }
    t
}

/// What to render of a scene.
#[derive(Clone, Copy, Default)]
struct VideoOpts {
    from: Option<f64>,
    to: Option<f64>,
    preview: bool,
}

impl VideoOpts {
    /// Not the full video.
    fn partial(&self) -> bool {
        self.from.is_some() || self.to.is_some() || self.preview
    }
}

/// Default video path: out/<scene file name>.mp4, or _preview.mp4 for a partial
/// render, which must not replace the full video.
fn default_out(scene_path: &str, opts: VideoOpts) -> String {
    let stem = Path::new(scene_path).file_stem().and_then(|s| s.to_str()).unwrap_or("out");
    let tail = if opts.partial() { "_preview" } else { "" };
    format!("out/{stem}{tail}.mp4")
}

/// Frames to render: size, rate, first instant and count.
#[derive(Debug)]
struct Plan {
    w: u32,
    h: u32,
    fps: u32,
    start: f64,
    frames: usize,
}

const PREVIEW_FPS: u32 = 15;

fn plan(video: &scene::Video, duration: f64, opts: VideoOpts) -> Result<Plan, String> {
    let (start, end) = (opts.from.unwrap_or(0.0), opts.to.unwrap_or(duration).min(duration));
    if start < 0.0 || start >= end {
        return Err(format!("nothing to render from {start:.1}s to {end:.1}s (the scene lasts {duration:.1}s)"));
    }
    // Half size, kept even as libx264 requires.
    let half = |v: u32| (v / 2).max(2) & !1;
    let (w, h, fps) = if opts.preview {
        (half(video.width), half(video.height), PREVIEW_FPS.min(video.fps))
    } else {
        (video.width, video.height, video.fps)
    };
    Ok(Plan { w, h, fps, start, frames: ((end - start) * fps as f64).ceil() as usize })
}

/// Renders one scene to a video with `jobs` threads.
fn render_video(sc: &scene::Scene, tl: &scene::Timeline, out: &str, opts: VideoOpts, jobs: usize, opt: &usvg::Options, progress: bool) -> Result<(), String> {
    // A full video comes with its layout image: where the points and devices are.
    if !opts.partial() {
        write_layout(sc, tl, &layout_path(out))?;
    }
    let Plan { w, h, fps, start, frames: n } = plan(&sc.video, tl.duration, opts)?;
    let mut ff = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgba"])
        .args(["-s", &format!("{w}x{h}"), "-framerate", &fps.to_string(), "-i", "-"])
        .args(["-c:v", "libx264", "-preset", "veryfast", "-crf", "18", "-pix_fmt", "yuv420p", "-movflags", "+faststart"])
        .arg(out)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run ffmpeg: {e}"))?;
    let mut ff_in = ff.stdin.take().unwrap();

    let next = AtomicUsize::new(0);
    let result: Result<(), String> = std::thread::scope(|s| {
        let (tx, rx) = mpsc::channel::<Result<(usize, Vec<u8>), String>>();
        for _ in 0..jobs.max(1) {
            let tx = tx.clone();
            let next = &next;
            s.spawn(move || loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= n {
                    break;
                }
                let svg = render::frame(sc, tl, start + i as f64 / fps as f64);
                // Raw RGBA straight to ffmpeg: the background is opaque, so
                // premultiplied and straight alpha are the same here.
                let r = rasterize(opt, &svg, w, h).map(|pm| (i, pm.take()));
                let failed = r.is_err();
                if tx.send(r).is_err() || failed {
                    next.store(n, Ordering::SeqCst);
                    break;
                }
            });
        }
        drop(tx);
        let mut pending = BTreeMap::new();
        let mut written = 0usize;
        for r in rx {
            let (i, rgba) = r?;
            pending.insert(i, rgba);
            while let Some(rgba) = pending.remove(&written) {
                ff_in.write_all(&rgba).map_err(|e| format!("ffmpeg pipe: {e}"))?;
                written += 1;
                if progress && (written.is_multiple_of(fps as usize * 5) || written == n) {
                    eprint!("\rframes {written}/{n}");
                }
            }
        }
        if progress {
            eprintln!();
        }
        if written != n {
            return Err(format!("only {written}/{n} frames rendered"));
        }
        Ok(())
    });
    drop(ff_in);
    let status = ff.wait().map_err(|e| e.to_string())?;
    result?;
    if !status.success() {
        return Err("ffmpeg failed".into());
    }
    Ok(())
}

/// Splits `jobs` threads among the scenes: returns (scenes rendered at the same
/// time, threads for each one).
fn share_jobs(jobs: usize, scenes: usize) -> (usize, usize) {
    let parallel = scenes.min(jobs).max(1);
    (parallel, (jobs / parallel).max(1))
}

/// Renders several scenes in parallel. Every scene is attempted: a failure does
/// not stop the others, and all the failures are reported at the end.
fn render_many(files: &[String], opts: VideoOpts, jobs: usize) -> Result<(), String> {
    let mut outs: Vec<String> = files.iter().map(|f| default_out(f, opts)).collect();
    outs.sort();
    if let Some(w) = outs.windows(2).find(|w| w[0] == w[1]) {
        return Err(format!("two scenes would write the same video: {}", w[0]));
    }
    std::fs::create_dir_all("out").map_err(|e| format!("cannot create out/: {e}"))?;
    let (parallel, each) = share_jobs(jobs, files.len());
    eprintln!("{} scenes: {parallel} at a time, {each} thread(s) each", files.len());
    let opt = raster_options();
    let next = AtomicUsize::new(0);
    let failures = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..parallel {
            s.spawn(|| {
                while let Some(path) = files.get(next.fetch_add(1, Ordering::SeqCst)) {
                    let out = default_out(path, opts);
                    let r = load(path).and_then(|(sc, tl)| {
                        for w in lint::warnings(&sc, &tl) {
                            eprintln!("warning: {path}: {w}");
                        }
                        render_video(&sc, &tl, &out, opts, each, &opt, false)
                    });
                    match r {
                        Ok(()) => eprintln!("wrote {out}"),
                        Err(e) => {
                            eprintln!("failed {path}");
                            failures.lock().unwrap().push(if e.starts_with(path.as_str()) { e } else { format!("{path}: {e}") });
                        }
                    }
                }
            });
        }
    });
    report_failures(failures.into_inner().unwrap(), files.len())
}

fn report_failures(failures: Vec<String>, total: usize) -> Result<(), String> {
    if failures.is_empty() {
        return Ok(());
    }
    Err(format!("{} of {total} scenes failed:\n{}", failures.len(), failures.join("\n")))
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "new") {
        return new_cmd(&args[1..]);
    }
    if args.first().is_some_and(|a| a == "index") {
        return index_cmd(&args[1..]);
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
        let (sc, tl) = load(scene_path)?;
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

/// `trackviz new <file> [--geometry name] [--carriers n]`: writes the skeleton of a
/// new scene. A bare name goes to scenes/<name>.mmd; an existing file is never
/// overwritten.
fn new_cmd(args: &[String]) -> Result<(), String> {
    let usage = || format!("usage: trackviz new <scene> [--geometry {}] [--carriers n]", template::geometry_names().join("|"));
    let (mut file, mut geometry, mut carriers) = (None, "sampling".to_string(), 1usize);
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--geometry" => {
                i += 1;
                geometry = args.get(i).ok_or_else(usage)?.clone();
            }
            "--carriers" => {
                i += 1;
                carriers = args.get(i).and_then(|n| n.parse().ok()).ok_or_else(usage)?;
            }
            a if a.starts_with('-') => return Err(format!("unknown option '{a}'\n{}", usage())),
            a if file.is_none() => file = Some(a.to_string()),
            _ => return Err(usage()),
        }
        i += 1;
    }
    let file = file.ok_or_else(usage)?;
    let mut path = PathBuf::from(&file);
    if path.extension().is_none() {
        path.set_extension("mmd");
    }
    if path.parent().is_none_or(|p| p.as_os_str().is_empty()) {
        path = Path::new("scenes").join(path);
    }
    if path.exists() {
        return Err(format!("{} already exists: choose another name", path.display()));
    }
    let name = path.file_stem().map(|s| s.to_string_lossy().replace('_', " ")).unwrap_or_default();
    let text = template::scene(&name, &geometry, carriers)?;
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    eprintln!("wrote {} (geometry {geometry}, {carriers} carrier{})", path.display(), if carriers == 1 { "" } else { "s" });
    eprintln!("check it with: trackviz {} --check", path.display());
    Ok(())
}

/// The layout image next to a video: out/a.mp4 -> out/a_layout.png.
fn layout_path(video: &str) -> String {
    let stem = video.strip_suffix(".mp4").unwrap_or(video);
    format!("{stem}_layout.png")
}

/// Writes the overview of the points and devices of a scene as a PNG.
fn write_layout(sc: &scene::Scene, tl: &scene::Timeline, path: &str) -> Result<(), String> {
    let svg = render::layout(sc, tl);
    let png = rasterize(&raster_options(), &svg, sc.video.width, sc.video.height)?.encode_png().map_err(|e| e.to_string())?;
    std::fs::write(path, png).map_err(|e| format!("{path}: {e}"))?;
    eprintln!("wrote {path}");
    Ok(())
}

/// `trackviz index [scene...] [-o page.html]`: the page that lists the videos
/// (default: every scene in scenes/, page out/index.html). The videos are looked
/// for next to the page, with the names a full render gives them.
fn index_cmd(args: &[String]) -> Result<(), String> {
    let usage = || "usage: trackviz index [<scene.mmd>...] [-o out/index.html]".to_string();
    let (mut scenes, mut out) = (Vec::new(), "out/index.html".to_string());
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" => {
                i += 1;
                out = args.get(i).ok_or_else(usage)?.clone();
            }
            a if a.starts_with('-') => return Err(format!("unknown option '{a}'\n{}", usage())),
            a => scenes.push(a.to_string()),
        }
        i += 1;
    }
    if scenes.is_empty() {
        scenes.push("scenes/*.mmd".into());
    }
    let mut files = Vec::new();
    for a in &scenes {
        for f in expand(a)? {
            if !files.contains(&f) {
                files.push(f);
            }
        }
    }
    let dir = Path::new(&out).parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new(".")).to_path_buf();
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let opt = raster_options();
    let (mut entries, mut failures, mut missing) = (Vec::new(), Vec::new(), Vec::new());
    let mut title = String::new();
    for f in &files {
        let (sc, tl) = match load(f) {
            Ok(x) => x,
            Err(e) => {
                failures.push(e);
                continue;
            }
        };
        if title.is_empty() {
            title = sc.video.title.clone();
        }
        let video = Path::new(&default_out(f, VideoOpts::default())).file_name().unwrap().to_string_lossy().to_string();
        let exists = dir.join(&video).exists();
        if !exists {
            missing.push(video.clone());
        }
        let mut chapters: Vec<(f64, String, String)> = tl
            .events
            .iter()
            .filter_map(|e| match &e.ev {
                scene::Ev::Caption { title, text } => Some((e.t0, title.clone(), text.clone())),
                _ => None,
            })
            .collect();
        chapters.sort_by(|a, b| a.0.total_cmp(&b.0));
        // The cover: the end of the middle caption, when most of the scene is on screen.
        let times = sheet_times(&tl);
        let t = times.get(times.len() / 2).copied().unwrap_or(tl.duration / 2.0);
        let cover = rasterize(&opt, &render::frame(&sc, &tl, t), 960, 540)?.encode_png().map_err(|e| e.to_string())?;
        entries.push(index::Entry { video, exists, subtitle: sc.video.subtitle.clone(), duration: tl.duration, chapters, cover: Some(cover) });
    }
    if entries.is_empty() {
        return report_failures(failures, files.len()).and(Err("no scene to list".into()));
    }
    std::fs::write(&out, index::page(&title, &entries)).map_err(|e| format!("{out}: {e}"))?;
    eprintln!("wrote {out} ({} videos)", entries.len());
    if !missing.is_empty() {
        eprintln!("warning: {} videos are not next to the page yet (render them with: trackviz \"scenes/*.mmd\"): {}", missing.len(), missing.join(", "));
    }
    report_failures(failures, files.len())
}

/// The contact sheet next to a video: out/a.mp4 -> out/a_sheet.png.
fn sheet_path(video: &str) -> String {
    let stem = video.strip_suffix(".mp4").unwrap_or(video);
    format!("{stem}_sheet.png")
}

/// The instants of the contact sheet: for every caption, the last moment before
/// the next one appears (the end of the scene for the last one), when what it
/// describes has happened; never before the caption has faded in.
fn sheet_times(tl: &scene::Timeline) -> Vec<f64> {
    let mut starts: Vec<f64> = tl.events.iter().filter(|e| matches!(e.ev, scene::Ev::Caption { .. })).map(|e| e.t0).collect();
    starts.sort_by(f64::total_cmp);
    starts.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    (0..starts.len())
        .map(|i| {
            let end = starts.get(i + 1).map_or(tl.duration, |&n| n - 0.1);
            end.max(starts[i] + 0.5).min(tl.duration)
        })
        .collect()
}

/// Writes the contact sheet of a scene: one frame per caption, two per row, at
/// half size.
fn write_sheet(sc: &scene::Scene, tl: &scene::Timeline, path: &str, jobs: usize) -> Result<(), String> {
    const COLS: u32 = 2;
    const GAP: u32 = 16;
    let times = sheet_times(tl);
    if times.is_empty() {
        return Err(format!("{path}: the scene has no captions, so there is no contact sheet"));
    }
    let (w, h) = ((sc.video.width / 2).max(2), (sc.video.height / 2).max(2));
    let opt = raster_options();
    // The frames, a few at a time.
    let mut tiles: Vec<Option<Result<tiny_skia::Pixmap, String>>> = (0..times.len()).map(|_| None).collect();
    for chunk in tiles.chunks_mut(jobs.max(1)).enumerate() {
        let (ci, slots) = chunk;
        std::thread::scope(|scope| {
            for (k, slot) in slots.iter_mut().enumerate() {
                let t = times[ci * jobs.max(1) + k];
                let opt = &opt;
                scope.spawn(move || *slot = Some(rasterize(opt, &render::frame(sc, tl, t), w, h)));
            }
        });
    }
    let rows = (times.len() as u32).div_ceil(COLS);
    let (sw, sh) = (COLS * w + (COLS + 1) * GAP, rows * h + (rows + 1) * GAP);
    let mut sheet = tiny_skia::Pixmap::new(sw, sh).ok_or_else(|| format!("invalid sheet size {sw}x{sh}"))?;
    sheet.fill(tiny_skia::Color::from_rgba8(0xCB, 0xD5, 0xE1, 0xFF));
    for (i, tile) in tiles.into_iter().enumerate() {
        let tile = tile.expect("every tile is rendered")?;
        let (c, r) = (i as u32 % COLS, i as u32 / COLS);
        let (x, y) = (GAP + c * (w + GAP), GAP + r * (h + GAP));
        sheet.draw_pixmap(x as i32, y as i32, tile.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
    }
    let png = sheet.encode_png().map_err(|e| e.to_string())?;
    std::fs::write(path, png).map_err(|e| format!("{path}: {e}"))?;
    eprintln!("wrote {path} ({} frames)", times.len());
    Ok(())
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

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(p: &str, n: &str) -> bool {
        wildcard_match(&p.chars().collect::<Vec<_>>(), &n.chars().collect::<Vec<_>>())
    }

    #[test]
    fn the_contact_sheet_shows_every_caption_at_its_end() {
        let (_, tl) = load(concat!(env!("CARGO_MANIFEST_DIR"), "/scenes/02_turnstile_three_carriers.mmd")).unwrap();
        let mut caps: Vec<f64> = tl.events.iter().filter(|e| matches!(e.ev, scene::Ev::Caption { .. })).map(|e| e.t0).collect();
        caps.sort_by(f64::total_cmp);
        let times = sheet_times(&tl);
        assert_eq!(times.len(), caps.len());
        for (i, &t) in times.iter().enumerate() {
            // The frame shows caption i: after it has faded in, before the next one.
            assert!(t >= caps[i] + 0.4 && caps.get(i + 1).is_none_or(|&n| t < n), "caption {i}: {t}");
        }
        assert!((times.last().unwrap() - tl.duration).abs() < 1e-9);
    }

    #[test]
    fn a_contact_sheet_is_a_grid_of_frames() {
        let (sc, tl) = load(concat!(env!("CARGO_MANIFEST_DIR"), "/scenes/01_prepare_sample_release.mmd")).unwrap();
        let path = std::env::temp_dir().join("trackviz_sheet_test.png");
        write_sheet(&sc, &tl, path.to_str().unwrap(), 4).unwrap();
        let img = tiny_skia::Pixmap::load_png(&path).unwrap();
        let rows = (sheet_times(&tl).len() as u32).div_ceil(2);
        assert_eq!((img.width(), img.height()), (2 * 960 + 3 * 16, rows * 540 + (rows + 1) * 16));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn wildcards_in_names() {
        assert!(m("*.mmd", "01_a.mmd"));
        assert!(m("0?_a.mmd", "01_a.mmd"));
        assert!(m("*spur*", "09_spur_happy_path.mmd"));
        assert!(m("*", "x"));
        assert!(!m("*.mmd", "a.mmd.bak"));
        assert!(!m("0?_a.mmd", "012_a.mmd"));
        assert!(!m("?", ""));
    }

    #[test]
    fn expand_lists_matching_files_sorted() {
        let dir = std::env::temp_dir().join(format!("trackviz_expand_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub.mmd")).unwrap();
        for f in ["b.mmd", "a.mmd", "c.txt"] {
            std::fs::write(dir.join(f), "").unwrap();
        }
        let d = dir.to_string_lossy();
        // Directories are skipped, the result is sorted.
        assert_eq!(expand(&format!("{d}/*.mmd")).unwrap(), vec![format!("{d}/a.mmd"), format!("{d}/b.mmd")]);
        assert!(expand(&format!("{d}/*.zip")).unwrap_err().contains("no matching files"));
        assert!(expand("sc*nes/a.mmd").unwrap_err().contains("only in the file name"));
        // Without wildcards the argument is left to the loader.
        assert_eq!(expand("missing.mmd").unwrap(), vec!["missing.mmd"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn threads_are_shared_among_the_scenes() {
        assert_eq!(share_jobs(10, 3), (3, 3));
        assert_eq!(share_jobs(10, 16), (10, 1));
        assert_eq!(share_jobs(1, 4), (1, 1));
    }

    fn video() -> scene::Video {
        scene::Video { title: String::new(), subtitle: String::new(), width: 1920, height: 1080, fps: 30, tail: 0.0, show_clock: true, show_times: true, belt: scene::BELT_SPEED, message: 1.5 }
    }

    #[test]
    fn a_full_render_covers_the_whole_scene() {
        let p = plan(&video(), 10.0, VideoOpts::default()).unwrap();
        assert_eq!((p.w, p.h, p.fps, p.start, p.frames), (1920, 1080, 30, 0.0, 300));
    }

    #[test]
    fn a_part_of_the_scene_and_the_preview_settings() {
        let part = VideoOpts { from: Some(2.0), to: Some(4.0), preview: false };
        let p = plan(&video(), 10.0, part).unwrap();
        assert_eq!((p.w, p.fps, p.start, p.frames), (1920, 30, 2.0, 60));
        let prev = VideoOpts { preview: true, ..VideoOpts::default() };
        let p = plan(&video(), 10.0, prev).unwrap();
        assert_eq!((p.w, p.h, p.fps, p.frames), (960, 540, 15, 150));
        // Beyond the end: cut at the end of the scene.
        let late = VideoOpts { from: Some(8.0), to: Some(99.0), preview: false };
        assert_eq!(plan(&video(), 10.0, late).unwrap().frames, 60);
        // A slow scene is not sped up, odd sizes become even.
        let odd = scene::Video { width: 1001, height: 501, fps: 10, ..video() };
        let p = plan(&odd, 10.0, prev).unwrap();
        assert_eq!((p.w, p.h, p.fps), (500, 250, 10));
    }

    #[test]
    fn an_empty_range_is_an_error() {
        for (from, to) in [(5.0, 2.0), (12.0, 20.0), (-1.0, 2.0)] {
            let o = VideoOpts { from: Some(from), to: Some(to), preview: false };
            assert!(plan(&video(), 10.0, o).unwrap_err().contains("nothing to render"));
        }
    }

    #[test]
    fn a_partial_render_keeps_the_full_video() {
        assert_eq!(default_out("scenes/a.mmd", VideoOpts::default()), "out/a.mp4");
        let prev = VideoOpts { preview: true, ..VideoOpts::default() };
        assert_eq!(default_out("scenes/a.mmd", prev), "out/a_preview.mp4");
        let part = VideoOpts { to: Some(3.0), ..VideoOpts::default() };
        assert_eq!(default_out("scenes/a.mmd", part), "out/a_preview.mp4");
    }

    #[test]
    fn a_change_is_seen_in_every_watched_file() {
        let dir = std::env::temp_dir().join(format!("trackviz_watch_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b) = (dir.join("a.mmd"), dir.join("b.mmd"));
        std::fs::write(&a, "x").unwrap();
        std::fs::write(&b, "x").unwrap();
        let files = vec![a, b.clone()];
        let before = stamps(&files);
        std::fs::File::options().write(true).open(&b).unwrap().set_modified(SystemTime::now() + Duration::from_secs(5)).unwrap();
        assert_ne!(stamps(&files), before);
        std::fs::remove_file(&b).unwrap();
        assert_eq!(stamps(&files)[1], None, "a file being replaced doesn't stop the watch");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_diagrams_in_scenes_are_valid_and_clean() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/scenes");
        let list: Vec<PathBuf> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|e| e == "mmd")).collect();
        assert!(!list.is_empty());
        for p in list {
            let (sc, tl) = load(&p.to_string_lossy()).unwrap();
            let w = lint::warnings(&sc, &tl);
            assert!(w.is_empty(), "{}: {w:?}", p.display());
        }
    }

    #[test]
    fn an_error_in_a_diagram_names_its_line() {
        let dir = std::env::temp_dir().join(format!("trackviz_flow_line_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("common")).unwrap();
        let common = concat!(env!("CARGO_MANIFEST_DIR"), "/scenes/common");
        for e in std::fs::read_dir(common).unwrap() {
            let p = e.unwrap().path();
            std::fs::copy(&p, dir.join("common").join(p.file_name().unwrap())).unwrap();
        }
        let f = dir.join("a.mmd");
        std::fs::write(&f, "%% geometry spur\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n\n%% move C9 to nsd\n").unwrap();
        let e = load(&f.to_string_lossy()).err().unwrap();
        assert!(e.contains("line 5") && e.contains("unknown carrier 'C9'"), "{e}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn help_shows_version_and_repository() {
        let u = usage();
        assert!(u.contains(VERSION) && u.contains("https://github.com/specialk74/trackviz"));
    }
}
