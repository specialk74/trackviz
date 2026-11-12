//! `trackviz sort`: the steps of a scene in time order, same video.

use super::*;
use crate::sort;
use std::path::Path;

/// `trackviz sort`: the steps of each scene in time order. The rewritten scene is
/// built and must give the same timeline before the file is replaced.
pub(super) fn sort_cmd(args: &[String]) -> Result<(), String> {
    let dry = args.iter().any(|a| a == "--dry-run");
    let names: Vec<&String> = args.iter().filter(|a| *a != "--dry-run").collect();
    if names.is_empty() {
        return Err("usage: trackviz sort <scene.mmd>... [--dry-run]".into());
    }
    if let Some(o) = names.iter().find(|a| a.starts_with('-')) {
        return Err(format!("sort: unknown option {o}"));
    }
    let mut failures = Vec::new();
    let mut total = 0;
    for name in names {
        for path in expand(name)? {
            total += 1;
            if let Err(e) = sort_one(&path, dry) {
                failures.push(format!("{path}: {e}"));
            }
        }
    }
    report_failures(failures, total)
}

pub(super) fn sort_one(path: &str, dry: bool) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let (sc, tl) = load(path)?;
    let Some((new, moved, anchors)) = sort::sorted(path, &text, &sc, &tl)? else {
        println!("{path}: already in time order");
        return Ok(());
    };
    // Built next to the scene, so that its common/ files are found.
    let p = Path::new(path);
    let tmp = p.with_file_name(format!(".{}.sorting.mmd", p.file_stem().and_then(|s| s.to_str()).unwrap_or("scene")));
    std::fs::write(&tmp, &new).map_err(|e| e.to_string())?;
    let check = load(&tmp.to_string_lossy()).and_then(|(_, tl2)| sort::same_timeline(&tl, &tl2));
    let _ = std::fs::remove_file(&tmp);
    check.map_err(|e| format!("not sorted, the file is unchanged: {e}"))?;
    let what = format!("{moved} steps moved, {anchors} anchored with an id");
    if dry {
        println!("{path}: would be sorted ({what})");
    } else {
        std::fs::write(path, new).map_err(|e| e.to_string())?;
        println!("{path}: sorted ({what}); the video is the same");
    }
    Ok(())
}
