//! trackviz — renders scenario files (track + message sequence) to MP4.
//!
//! Usage:
//!   trackviz <scene.toml> [-o out.mp4] [-j jobs]
//!   trackviz <scene.toml> --still <seconds> [-o frame.png]
//!   trackviz <scene.toml> --check            (validate + print the timeline)
//!
//! Requires `rsvg-convert` (librsvg) and `ffmpeg` on PATH.

mod render;
mod scene;
mod toml;

use std::collections::BTreeMap;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

fn rasterize(svg: &str, w: u32, h: u32) -> Result<Vec<u8>, String> {
    let mut child = Command::new("rsvg-convert")
        .args(["-w", &w.to_string(), "-h", &h.to_string(), "-f", "png"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run rsvg-convert: {e}"))?;
    child.stdin.take().unwrap().write_all(svg.as_bytes()).map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("rsvg-convert failed: {}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(out.stdout)
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
            std::fs::write(&path, rasterize(&svg, w, h)?).map_err(|e| e.to_string())?;
        }
        eprintln!("wrote {path}");
        return Ok(());
    }

    let out = out.unwrap_or_else(|| "out.mp4".into());
    let n = (tl.duration * fps as f64).ceil() as usize;
    let mut ff = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "image2pipe", "-c:v", "png"])
        .args(["-framerate", &fps.to_string(), "-i", "-"])
        .args(["-c:v", "libx264", "-preset", "veryfast", "-crf", "18", "-pix_fmt", "yuv420p", "-movflags", "+faststart"])
        .arg(&out)
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run ffmpeg: {e}"))?;
    let mut ff_in = ff.stdin.take().unwrap();

    let next = AtomicUsize::new(0);
    let result: Result<(), String> = std::thread::scope(|s| {
        let (tx, rx) = mpsc::channel::<Result<(usize, Vec<u8>), String>>();
        for _ in 0..jobs.max(1) {
            let tx = tx.clone();
            let (sc, tl, next) = (&sc, &tl, &next);
            s.spawn(move || loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= n {
                    break;
                }
                let svg = render::frame(sc, tl, i as f64 / fps as f64);
                let r = rasterize(&svg, w, h).map(|png| (i, png));
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
            let (i, png) = r?;
            pending.insert(i, png);
            while let Some(png) = pending.remove(&written) {
                ff_in.write_all(&png).map_err(|e| format!("ffmpeg pipe: {e}"))?;
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
