//! `trackviz new`: the skeleton of a new scene.

use crate::template;
use std::path::{Path, PathBuf};

/// `trackviz new <file> [--geometry name] [--carriers n]`: writes the skeleton of a
/// new scene. A bare name goes to scenes/<name>.mmd; an existing file is never
/// overwritten.
pub(super) fn new_cmd(args: &[String]) -> Result<(), String> {
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
