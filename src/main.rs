//! trackviz — renders scenario files (track + message sequence) to MP4.
//!
//! Usage:
//!   trackviz <scene.toml> [-o out.mp4] [-j jobs]   (default: out/<scene name>.mp4)
//!   trackviz <scene.toml> --still <seconds> [-o frame.png]
//!   trackviz <scene.toml> --check            (validate + print the timeline)
//!
//! Frames are rasterized in-process with resvg; only `ffmpeg` is needed on PATH
//! (and only for video output).

mod render;
mod scene;
mod toml;

use resvg::{tiny_skia, usvg};
use std::collections::BTreeMap;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

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

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut scene_path = None;
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
                println!("usage: trackviz <scene.toml> [-o out.mp4] [-j jobs] [--still SECONDS [--svg]] [--check]");
                return Ok(());
            }
            a if scene_path.is_none() && !a.starts_with('-') => scene_path = Some(a.to_string()),
            a => return Err(format!("unexpected argument '{a}'")),
        }
        i += 1;
    }
    let scene_path = scene_path.ok_or("missing scene file (try --help)")?;
    let src = std::fs::read_to_string(&scene_path).map_err(|e| format!("{scene_path}: {e}"))?;
    let doc = toml::parse(&src).map_err(|e| format!("{scene_path}: {e}"))?;
    let sc = scene::Scene::from_doc(&doc).map_err(|e| format!("{scene_path}: {e}"))?;
    let tl = scene::Timeline::build(&sc).map_err(|e| format!("{scene_path}: {e}"))?;

    eprintln!("Timeline ({:.1} s total):", tl.duration);
    for l in &tl.log {
        eprintln!("  {l}");
    }
    if check {
        return Ok(());
    }

    let (w, h, fps) = (sc.video.width, sc.video.height, sc.video.fps);

    if let Some(t) = still {
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

    // Default: out/<scene file name>.mp4
    let out = match out {
        Some(o) => o,
        None => {
            let stem = std::path::Path::new(&scene_path).file_stem().and_then(|s| s.to_str()).unwrap_or("out");
            std::fs::create_dir_all("out").map_err(|e| format!("cannot create out/: {e}"))?;
            format!("out/{stem}.mp4")
        }
    };
    let n = (tl.duration * fps as f64).ceil() as usize;
    let mut ff = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgba"])
        .args(["-s", &format!("{w}x{h}"), "-framerate", &fps.to_string(), "-i", "-"])
        .args(["-c:v", "libx264", "-preset", "veryfast", "-crf", "18", "-pix_fmt", "yuv420p", "-movflags", "+faststart"])
        .arg(&out)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run ffmpeg: {e}"))?;
    let mut ff_in = ff.stdin.take().unwrap();

    let opt = raster_options();
    let next = AtomicUsize::new(0);
    let result: Result<(), String> = std::thread::scope(|s| {
        let (tx, rx) = mpsc::channel::<Result<(usize, Vec<u8>), String>>();
        for _ in 0..jobs.max(1) {
            let tx = tx.clone();
            let (sc, tl, next, opt) = (&sc, &tl, &next, &opt);
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
                if written % (fps as usize * 5) == 0 || written == n {
                    eprint!("\rframes {written}/{n}");
                }
            }
        }
        eprintln!();
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
    eprintln!("wrote {out}");
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
