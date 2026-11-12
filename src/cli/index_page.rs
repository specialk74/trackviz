//! `trackviz index`: the page that lists the videos.

use super::*;
use crate::{index, render, scene};
use std::path::Path;

/// `trackviz index [scene...] [-o page.html]`: the page that lists the videos
/// (default: every scene in scenes/, page out/index.html). The videos are looked
/// for next to the page, with the names a full render gives them.
pub(super) fn index_cmd(args: &[String]) -> Result<(), String> {
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
