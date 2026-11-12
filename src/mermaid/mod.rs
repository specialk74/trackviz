//! Scenes are Mermaid sequence diagrams: this reads one, with its `%%` comments
//! that describe the physical side, into the data of a scene.
//!
//! - `lines`: the lines of a diagram, with its includes;
//! - `importer`: the state while reading, line by line;
//! - `directives`, `timing`, `antennas`: the `%%` directives;
//! - `diagram`: messages and notes;
//! - `suggest`: the closest directive to a mistyped one.

mod antennas;
mod diagram;
mod directives;
mod importer;
mod lines;
mod suggest;
mod timing;

use crate::doc::{parse_line, Doc, Table, Value};
use importer::Importer;
use lines::{read_into, read_lines, Line};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use suggest::suggest;

/// Keys of a table being built.
type Keys = Vec<(&'static str, Value)>;

fn text(s: &str) -> Value {
    Value::Str(s.to_string())
}

fn seconds(what: &str, s: &str) -> Result<Value, String> {
    s.parse::<f64>().map(Value::Num).map_err(|_| format!("'{what}' needs seconds, not '{s}'"))
}

fn duration(o: &HashMap<&str, &str>) -> Result<Keys, String> {
    Ok(match o.get("duration") {
        Some(d) => vec![("duration", seconds("duration", d)?)],
        None => Vec::new(),
    })
}

/// The shared files every scene starts from, in `common/`.
const SHARED: [&str; 3] = ["video", "actors", "messages"];

/// The data of a scene written as a Mermaid diagram. `base` is where `common/` is:
/// its video, actors and messages come first, a geometry with `%% geometry`.
pub fn load(path: &Path, base: &Path) -> Result<Doc, String> {
    let mut im = Importer::new(Some(base.to_path_buf()));
    for name in SHARED {
        let f = base.join("common").join(format!("{name}.mmd"));
        if f.exists() {
            im.include(&f)?;
        }
    }
    let mut files = Vec::new();
    let lines = read_lines(path, &mut files)?;
    for l in &lines {
        im.read(l)?;
    }
    im.files.splice(0..0, files);
    im.finish(path.file_stem().and_then(|s| s.to_str()).unwrap_or("scene"))
}

/// The data of a scene given as text, without shared files (used by the tests).
#[cfg(test)]
pub fn parse_text(src: &str) -> Result<Doc, String> {
    let mut im = Importer::new(None);
    for (i, t) in src.lines().enumerate() {
        im.read(&Line { file: String::new(), no: i + 1, text: t.to_string() })?;
    }
    im.finish("test")
}

#[cfg(test)]
mod tests;
