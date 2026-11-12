//! The importer: the state while a diagram is read, line by line, and the
//! tables it adds.

use super::*;

/// Turns a diagram into the data of a scene.
pub(super) struct Importer {
    pub(super) doc: Doc,
    /// Where `common/` is; None for a diagram given as text.
    pub(super) base: Option<PathBuf>,
    pub(super) files: Vec<PathBuf>,
    pub(super) started: bool,
    pub(super) geometry: Option<String>,
    /// Mermaid participant id -> actor id.
    pub(super) actors: Vec<(String, String)>,
    /// (carrier id, sample id; empty for an empty carrier)
    pub(super) samples: Vec<(String, String)>,
    /// Timing keys for the next step.
    pub(super) timing: Keys,
    pub(super) last_pipette: bool,
    /// Where the line being read comes from: (file, line).
    pub(super) file: String,
    pub(super) line: usize,
    /// `%% for`: the carrier of the next message, when its text doesn't say it.
    pub(super) next_carrier: Option<String>,
    /// `%% on`: the next step starts when a carrier is over this antenna:
    /// (point, antenna, carrier if written).
    pub(super) on: Option<(String, String, Option<String>)>,
    /// `%% color`: the color of the next message.
    pub(super) color: Option<String>,
    /// `%% id`: the name of the next step, for `%% after <seconds> <id>`.
    pub(super) next_id: Option<String>,
    /// The names given so far.
    pub(super) ids: Vec<String>,
}

impl Importer {
    pub(super) fn new(base: Option<PathBuf>) -> Importer {
        let mut doc = Doc::default();
        doc.tables.insert(String::new(), Table { name: "(root)".into(), ..Default::default() });
        Importer {
            doc,
            base,
            files: Vec::new(),
            started: false,
            geometry: None,
            actors: Vec::new(),
            samples: Vec::new(),
            timing: Vec::new(),
            last_pipette: false,
            file: String::new(),
            line: 0,
            next_carrier: None,
            on: None,
            color: None,
            next_id: None,
            ids: Vec::new(),
        }
    }

    pub(super) fn add(&mut self, name: &str, keys: Keys) {
        let entries = keys.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        let t = Table { name: name.to_string(), line: self.line, file: self.file.clone(), entries };
        self.doc.add(t);
    }

    pub(super) fn step(&mut self, action: &str, keys: Keys) -> Result<(), String> {
        if self.color.is_some() && action != "message" {
            return Err(format!("'color' applies to the next message, not to a {action}"));
        }
        if let Some((point, antenna, who)) = self.on.take() {
            let own = keys.iter().find_map(|(k, v)| match (*k, v) {
                ("carrier", Value::Str(c)) => Some(c.clone()),
                _ => None,
            });
            let c = who.or(own).ok_or_else(|| format!("'on {antenna}' before a {action}: write the carrier, on {antenna} <carrier>"))?;
            self.timing.push(("when", text(&format!("{c} at {point}"))));
        }
        let mut all: Keys = vec![("action", text(action))];
        if let Some(id) = self.next_id.take() {
            all.push(("id", text(&id)));
        }
        all.append(&mut self.timing);
        all.extend(keys);
        self.add("step", all);
        self.last_pipette = action == "pipette";
        Ok(())
    }

    pub(super) fn list(&self, section: &str) -> &[Table] {
        self.doc.arrays.get(section).map_or(&[], |v| v.as_slice())
    }

    /// Reads the directives of a shared file here.
    pub(super) fn include(&mut self, path: &Path) -> Result<(), String> {
        let mut lines = Vec::new();
        read_into(path, true, &mut self.files, &mut Vec::new(), &mut lines)?;
        let (file, line) = (std::mem::take(&mut self.file), self.line);
        for l in &lines {
            self.read(l)?;
        }
        (self.file, self.line) = (file, line);
        Ok(())
    }

    pub(super) fn participant(&mut self, id: &str, label: &str) {
        if self.actors.iter().any(|a| a.0 == id) {
            return;
        }
        let known = self.list("actor").iter().find_map(|t| {
            let (aid, alabel) = (t.str_opt("id").ok()??, t.str_opt("label").ok()?);
            (aid == id || alabel.as_deref() == Some(label)).then_some(aid)
        });
        let actor = known.clone().unwrap_or_else(|| id.to_string());
        if known.is_none() {
            self.add("actor", vec![("id", text(id)), ("label", text(label))]);
        }
        self.actors.push((id.to_string(), actor));
    }

    pub(super) fn actor(&self, id: &str) -> String {
        self.actors.iter().find(|a| a.0 == id).map_or_else(|| id.to_string(), |a| a.1.clone())
    }

    /// One line of a diagram or of a shared file.
    pub(super) fn read(&mut self, l: &Line) -> Result<(), String> {
        let line = l.text.trim();
        (self.file, self.line) = (l.file.clone(), l.no);
        let at = |e: String| format!("{}: {e}", l.place());
        if line.is_empty() || line.starts_with("%%{") {
            return Ok(());
        }
        if let Some(d) = line.strip_prefix("%%") {
            return self.directive(d.trim()).map_err(at);
        }
        if !self.started {
            if line != "sequenceDiagram" {
                return Err(at("the diagram must start with 'sequenceDiagram'".to_string()));
            }
            self.started = true;
            return Ok(());
        }
        let first = line.split_whitespace().next().unwrap_or_default();
        match first {
            "participant" | "actor" => {
                let rest = line[first.len()..].trim();
                let (id, label) = rest.split_once(" as ").unwrap_or((rest, rest));
                self.participant(id.trim(), label.trim());
                Ok(())
            }
            "Note" | "note" => self.note(line).map_err(at),
            "autonumber" | "activate" | "deactivate" | "rect" | "end" => Ok(()),
            "alt" | "else" | "opt" | "loop" | "par" | "and" | "critical" | "break" => {
                Err(at(format!("'{first}' is not supported: a scene is one path, write one diagram for each case")))
            }
            _ => self.message(line).map_err(at),
        }
    }

    pub(super) fn finish(mut self, name: &str) -> Result<Doc, String> {
        if let Some((_, antenna, _)) = &self.on {
            return Err(format!("'on {antenna}' at the end of the diagram: its step is missing"));
        }
        if self.color.is_some() {
            return Err("'color' at the end of the diagram: its message is missing".into());
        }
        if let Some(id) = &self.next_id {
            return Err(format!("'id {id}' at the end of the diagram: its line is missing"));
        }
        let video = self.doc.tables.get("video");
        if !video.is_some_and(|v| v.entries.contains_key("subtitle")) {
            self.add("video", vec![("subtitle", text(name))]);
        }
        self.doc.files = self.files;
        Ok(self.doc)
    }
}
