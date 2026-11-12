//! The lines of a diagram, with `%% include` replaced by the lines of the file.

use std::path::{Path, PathBuf};

/// A line of a diagram, with where it comes from (empty file: the text given).
pub struct Line {
    pub(super) file: String,
    pub(super) no: usize,
    pub(super) text: String,
}

impl Line {
    pub(super) fn place(&self) -> String {
        if self.file.is_empty() {
            format!("line {}", self.no)
        } else {
            format!("{} line {}", self.file, self.no)
        }
    }
}

/// Reads a diagram replacing every `%% include <file>` with the lines of that file.
/// Included files hold directives only: a message there would be missing from the
/// diagram that Mermaid shows. `files` collects everything read.
pub fn read_lines(path: &Path, files: &mut Vec<PathBuf>) -> Result<Vec<Line>, String> {
    let mut out = Vec::new();
    read_into(path, false, files, &mut Vec::new(), &mut out)?;
    Ok(out)
}

pub(super) fn read_into(path: &Path, nested: bool, files: &mut Vec<PathBuf>, stack: &mut Vec<PathBuf>, out: &mut Vec<Line>) -> Result<(), String> {
    let name = path.display().to_string();
    let real = path.canonicalize().map_err(|e| format!("{name}: {e}"))?;
    if stack.contains(&real) {
        return Err(format!("{name}: circular include"));
    }
    let src = std::fs::read_to_string(path).map_err(|e| format!("{name}: {e}"))?;
    files.push(path.to_path_buf());
    stack.push(real);
    for (i, text) in src.lines().enumerate() {
        let line = text.trim();
        let directive = line.strip_prefix("%%").map(str::trim);
        if let Some(target) = directive.and_then(|d| d.strip_prefix("include ")) {
            read_into(&path.parent().unwrap_or(Path::new("")).join(target.trim()), true, files, stack, out)?;
        } else if nested && !line.is_empty() && directive.is_none() {
            return Err(format!("{name} line {}: an included file can only hold %% directives, so that the diagram stays complete", i + 1));
        } else {
            out.push(Line { file: name.clone(), no: i + 1, text: text.to_string() });
        }
    }
    stack.pop();
    Ok(())
}
