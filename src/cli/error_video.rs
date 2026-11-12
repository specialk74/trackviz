//! The video of a scene that breaks, up to where it breaks.

use super::*;
use crate::{doc, render, scene};
use std::path::Path;

/// When a scene breaks: the longest beginning of it that works (its first steps),
/// ending with a red caption that shows the error, so that the video shows the
/// carriers up to where it breaks. None if not even the scene without steps works.
pub(super) fn until_error(path: &str, err: &str) -> Option<(scene::Scene, scene::Timeline, usize)> {
    let doc = scene_doc(path).ok()?;
    let steps = doc.arrays.get("step").cloned().unwrap_or_default();
    // Where: "... line 102: what" -> ("line 102", "what").
    // Only the error, not the log after it.
    let msg = err.strip_prefix(path).unwrap_or(err).trim_start_matches(": ").lines().next().unwrap_or("");
    let (title, text) = match msg.find("line ") {
        Some(i) => {
            let rest = &msg[i..];
            let end = rest.find(": ").unwrap_or(rest.len());
            (format!("{} · {}", render::ERROR_TITLE, &rest[..end]), rest[end..].trim_start_matches(": ").to_string())
        }
        None => (render::ERROR_TITLE.to_string(), msg.to_string()),
    };
    let build = |k: usize| -> Option<(scene::Scene, scene::Timeline)> {
        let mut kept = steps[..k].to_vec();
        let mut cap = doc::Table { name: "step".into(), line: 0, file: String::new(), entries: Default::default() };
        cap.entries.insert("action".into(), doc::Value::Str("caption".into()));
        cap.entries.insert("title".into(), doc::Value::Str(title.clone()));
        cap.entries.insert("text".into(), doc::Value::Str(text.clone()));
        kept.push(cap);
        let mut arrays = doc.arrays.clone();
        arrays.insert("step".into(), kept);
        let d = doc::Doc { tables: doc.tables.clone(), arrays, files: doc.files.clone() };
        let sc = scene::Scene::from_doc(&d).ok()?;
        let tl = scene::Timeline::build(&sc).ok()?;
        Some((sc, tl))
    };
    // The step that breaks, from the line in the error; then back one step at a
    // time, so that the video goes as near to the error as it can (a shorter
    // scene can break elsewhere: a carrier left on a gate without its pass).
    let err_line: Option<usize> = msg.split("line ").nth(1).and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next()).and_then(|n| n.parse().ok());
    let at = err_line.and_then(|n| steps.iter().position(|t| t.line >= n)).unwrap_or(steps.len());
    let mut lo = None;
    for k in (0..=at.min(steps.len())).rev().take(60) {
        if let Some(r) = build(k) {
            lo = Some((k, r));
            break;
        }
    }
    // Far from it: the most steps that work, by halves.
    let (lo, best) = match lo {
        Some(found) => found,
        None => {
            let mut best = build(0)?;
            let (mut lo, mut hi) = (0, at.min(steps.len()));
            while hi - lo > 1 {
                let mid = (lo + hi) / 2;
                match build(mid) {
                    Some(r) => {
                        best = r;
                        lo = mid;
                    }
                    None => hi = mid,
                }
            }
            (lo, best)
        }
    };
    let line = steps.get(lo).map_or(0, |t| t.line);
    Some((best.0, best.1, line))
}

/// The video of a scene that breaks: out/<name>_error.mp4.
pub(super) fn render_until_error(path: &str, err: &str, out: Option<&str>, opts: VideoOpts, jobs: usize) {
    let Some((sc, tl, line)) = until_error(path, err) else { return };
    let stem = Path::new(path).file_stem().and_then(|s| s.to_str()).unwrap_or("out");
    let out = out.map_or_else(|| format!("out/{stem}_error.mp4"), str::to_string);
    if std::fs::create_dir_all("out").is_err() {
        return;
    }
    match render_video(&sc, &tl, &out, opts, jobs, &raster_options(), true) {
        Ok(()) => eprintln!("wrote {out}: the video up to line {line}, where the scene breaks"),
        Err(e) => eprintln!("could not write the video up to the error: {e}"),
    }
}
