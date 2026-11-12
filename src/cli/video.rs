//! Videos: which frames, rendered by worker threads and piped to ffmpeg; several
//! scenes in parallel.

use super::*;
use crate::{lint, render, scene};
use resvg::usvg;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Mutex};

/// What to render of a scene.
#[derive(Clone, Copy, Default)]
pub(super) struct VideoOpts {
    pub(super) from: Option<f64>,
    pub(super) to: Option<f64>,
    pub(super) preview: bool,
}

impl VideoOpts {
    /// Not the full video.
    pub(super) fn partial(&self) -> bool {
        self.from.is_some() || self.to.is_some() || self.preview
    }
}

/// Default video path: out/<scene file name>.mp4, or _preview.mp4 for a partial
/// render, which must not replace the full video.
pub(super) fn default_out(scene_path: &str, opts: VideoOpts) -> String {
    let stem = Path::new(scene_path).file_stem().and_then(|s| s.to_str()).unwrap_or("out");
    let tail = if opts.partial() { "_preview" } else { "" };
    format!("out/{stem}{tail}.mp4")
}

/// Frames to render: size, rate, first instant and count.
#[derive(Debug)]
pub(super) struct Plan {
    pub(super) w: u32,
    pub(super) h: u32,
    pub(super) fps: u32,
    pub(super) start: f64,
    pub(super) frames: usize,
}

pub(super) const PREVIEW_FPS: u32 = 15;

pub(super) fn plan(video: &scene::Video, duration: f64, opts: VideoOpts) -> Result<Plan, String> {
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
pub(super) fn render_video(sc: &scene::Scene, tl: &scene::Timeline, out: &str, opts: VideoOpts, jobs: usize, opt: &usvg::Options, progress: bool) -> Result<(), String> {
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
pub(super) fn share_jobs(jobs: usize, scenes: usize) -> (usize, usize) {
    let parallel = scenes.min(jobs).max(1);
    (parallel, (jobs / parallel).max(1))
}

/// Renders several scenes in parallel. Every scene is attempted: a failure does
/// not stop the others, and all the failures are reported at the end.
pub(super) fn render_many(files: &[String], opts: VideoOpts, jobs: usize) -> Result<(), String> {
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
                        for w in lint::warnings_at(&sc, &tl) {
                            eprintln!("warning: {}", w.show(path));
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
