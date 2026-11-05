//! trackviz — renders scenario files (track + message sequence) to MP4.
//!
//! Usage:
//!   trackviz <scene.toml>... [-o out.mp4] [-j jobs]   (default: out/<scene name>.mp4)
//!   trackviz <scene.toml> --still <seconds> [-o frame.png]
//!   trackviz <scene.toml>... --check         (validate + print the timeline)
//!
//! Scene arguments may contain the wildcards `*` and `?` in the file name: several
//! scenes are rendered in parallel, sharing the `-j` threads.
//!
//! Frames are rasterized in-process with resvg; only `ffmpeg` is needed on PATH
//! (and only for video output).

mod render;
mod scene;
mod toml;

use resvg::{tiny_skia, usvg};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Mutex};

/// Version and repository from Cargo.toml.
const VERSION: &str = env!("CARGO_PKG_VERSION");
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

fn usage() -> String {
    format!(
        "trackviz {VERSION}\n\
         {REPOSITORY}\n\
         usage: trackviz <scene.toml>... [-o out.mp4] [-j jobs] [--still SECONDS [--svg]] [--check]\n\
         \x20 scene files may use the wildcards * and ? (e.g. \"scenes/*.toml\"): they are rendered in parallel\n\
         \x20 without -o the video is written to out/<scene name>.mp4 (-o and --still take a single scene)"
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

fn load(path: &str) -> Result<(scene::Scene, scene::Timeline), String> {
    let doc = toml::parse_file(Path::new(path)).map_err(|e| if e.starts_with(path) { e } else { format!("{path}: {e}") })?;
    let sc = scene::Scene::from_doc(&doc).map_err(|e| format!("{path}: {e}"))?;
    let tl = scene::Timeline::build(&sc).map_err(|e| format!("{path}: {e}"))?;
    Ok((sc, tl))
}

fn timeline_text(tl: &scene::Timeline) -> String {
    let mut t = format!("Timeline ({:.1} s total):\n", tl.duration);
    for l in &tl.log {
        t.push_str(&format!("  {l}\n"));
    }
    t
}

/// Default video path: out/<scene file name>.mp4
fn default_out(scene_path: &str) -> String {
    let stem = Path::new(scene_path).file_stem().and_then(|s| s.to_str()).unwrap_or("out");
    format!("out/{stem}.mp4")
}

/// Renders one scene to a video with `jobs` threads.
fn render_video(sc: &scene::Scene, tl: &scene::Timeline, out: &str, jobs: usize, opt: &usvg::Options, progress: bool) -> Result<(), String> {
    let (w, h, fps) = (sc.video.width, sc.video.height, sc.video.fps);
    let n = (tl.duration * fps as f64).ceil() as usize;
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
                let svg = render::frame(sc, tl, i as f64 / fps as f64);
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
fn render_many(files: &[String], jobs: usize) -> Result<(), String> {
    let mut outs: Vec<String> = files.iter().map(|f| default_out(f)).collect();
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
                    let out = default_out(path);
                    let r = load(path).and_then(|(sc, tl)| render_video(&sc, &tl, &out, each, &opt, false));
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
    let mut scene_args = Vec::new();
    let mut out = None;
    let mut still: Option<f64> = None;
    let mut svg_out = false;
    let mut check = false;
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
            "--svg" => svg_out = true,
            "--check" => check = true,
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
    if files.len() > 1 && (out.is_some() || still.is_some()) {
        return Err(format!("-o and --still take a single scene, but {} were given", files.len()));
    }

    if check {
        let mut failures = Vec::new();
        for f in &files {
            if files.len() > 1 {
                eprintln!("== {f}");
            }
            match load(f) {
                Ok((_, tl)) => eprint!("{}", timeline_text(&tl)),
                Err(e) if files.len() == 1 => return Err(e),
                Err(e) => failures.push(e),
            }
        }
        return report_failures(failures, files.len());
    }
    if files.len() > 1 {
        return render_many(&files, jobs.max(1));
    }

    let scene_path = &files[0];
    let (sc, tl) = load(scene_path)?;
    eprint!("{}", timeline_text(&tl));
    if let Some(t) = still {
        let (w, h) = (sc.video.width, sc.video.height);
        let svg = render::frame(&sc, &tl, t);
        let path = out.unwrap_or_else(|| if svg_out { "frame.svg".into() } else { "frame.png".into() });
        if svg_out {
            std::fs::write(&path, svg).map_err(|e| e.to_string())?;
        } else {
            let png = rasterize(&raster_options(), &svg, w, h)?.encode_png().map_err(|e| e.to_string())?;
            std::fs::write(&path, png).map_err(|e| e.to_string())?;
        }
        eprintln!("wrote {path}");
        return Ok(());
    }
    let out = match out {
        Some(o) => o,
        None => {
            std::fs::create_dir_all("out").map_err(|e| format!("cannot create out/: {e}"))?;
            default_out(scene_path)
        }
    };
    render_video(&sc, &tl, &out, jobs, &raster_options(), true)?;
    eprintln!("wrote {out}");
    Ok(())
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
    fn wildcards_in_names() {
        assert!(m("*.toml", "01_a.toml"));
        assert!(m("0?_a.toml", "01_a.toml"));
        assert!(m("*spur*", "09_spur_happy_path.toml"));
        assert!(m("*", "x"));
        assert!(!m("*.toml", "a.toml.bak"));
        assert!(!m("0?_a.toml", "012_a.toml"));
        assert!(!m("?", ""));
    }

    #[test]
    fn expand_lists_matching_files_sorted() {
        let dir = std::env::temp_dir().join(format!("trackviz_expand_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub.toml")).unwrap();
        for f in ["b.toml", "a.toml", "c.txt"] {
            std::fs::write(dir.join(f), "").unwrap();
        }
        let d = dir.to_string_lossy();
        // Directories are skipped, the result is sorted.
        assert_eq!(expand(&format!("{d}/*.toml")).unwrap(), vec![format!("{d}/a.toml"), format!("{d}/b.toml")]);
        assert!(expand(&format!("{d}/*.zip")).unwrap_err().contains("no matching files"));
        assert!(expand("sc*nes/a.toml").unwrap_err().contains("only in the file name"));
        // Without wildcards the argument is left to the loader.
        assert_eq!(expand("missing.toml").unwrap(), vec!["missing.toml"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn threads_are_shared_among_the_scenes() {
        assert_eq!(share_jobs(10, 3), (3, 3));
        assert_eq!(share_jobs(10, 16), (10, 1));
        assert_eq!(share_jobs(1, 4), (1, 1));
    }

    #[test]
    fn help_shows_version_and_repository() {
        let u = usage();
        assert!(u.contains(VERSION) && u.contains("https://github.com/specialk74/trackviz"));
    }
}
