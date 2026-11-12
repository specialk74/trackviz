//! Scene files: wildcards, where `common/` is, loading a scene, the timeline as
//! `--check` prints it.

use crate::{doc, lint, mermaid, scene};
use std::path::{Path, PathBuf};

/// Does `name` match `pattern`? `*` stands for any text, `?` for one character.
pub(super) fn wildcard_match(pattern: &[char], name: &[char]) -> bool {
    match pattern.split_first() {
        None => name.is_empty(),
        Some(('*', rest)) => (0..=name.len()).any(|k| wildcard_match(rest, &name[k..])),
        Some(('?', rest)) => !name.is_empty() && wildcard_match(rest, &name[1..]),
        Some((c, rest)) => name.first() == Some(c) && wildcard_match(rest, &name[1..]),
    }
}

/// Expands the wildcards of a scene argument (only in the file name) into the
/// matching files, sorted. An argument without wildcards is returned as it is.
pub(super) fn expand(arg: &str) -> Result<Vec<String>, String> {
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
pub(super) fn common_base(path: &Path) -> PathBuf {
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    if dir.join("common").is_dir() || !Path::new("scenes/common").is_dir() {
        dir.to_path_buf()
    } else {
        PathBuf::from("scenes")
    }
}

/// The data of a scene, with the files read to build it.
pub(crate) fn scene_doc(path: &str) -> Result<doc::Doc, String> {
    if !Path::new(path).extension().is_some_and(|e| e == "mmd") {
        return Err("a scene is a Mermaid diagram, a .mmd file".into());
    }
    mermaid::load(Path::new(path), &common_base(Path::new(path)))
}

pub(crate) fn load(path: &str) -> Result<(scene::Scene, scene::Timeline), String> {
    let prefix = |e: String| if e.starts_with(path) { e } else { format!("{path}: {e}") };
    let doc = scene_doc(path).map_err(prefix)?;
    let sc = scene::Scene::from_doc(&doc).map_err(prefix)?;
    let tl = scene::Timeline::build(&sc).map_err(prefix)?;
    Ok((sc, tl))
}

/// The timeline as `--check` prints it; warnings name the scene, so that an editor
/// can show them on its file.
pub(super) fn timeline_text(path: &str, sc: &scene::Scene, tl: &scene::Timeline) -> String {
    let mut t = format!("Timeline ({:.1} s total):\n", tl.duration);
    for l in &tl.log {
        t.push_str(&format!("  {l}\n"));
    }
    for w in lint::warnings_at(sc, tl) {
        t.push_str(&format!("warning: {}\n", w.show(path)));
    }
    t
}

pub(super) fn report_failures(failures: Vec<String>, total: usize) -> Result<(), String> {
    if failures.is_empty() {
        return Ok(());
    }
    Err(format!("{} of {total} scenes failed:\n{}", failures.len(), failures.join("\n")))
}
