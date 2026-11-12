//! Scenes are Mermaid sequence diagrams: this reads one, with its `%%` comments
//! that describe the physical side, into the data of a scene.

use crate::doc::{parse_line, Doc, Table, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A line of a diagram, with where it comes from (empty file: the text given).
pub struct Line {
    file: String,
    no: usize,
    text: String,
}

impl Line {
    fn place(&self) -> String {
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

fn read_into(path: &Path, nested: bool, files: &mut Vec<PathBuf>, stack: &mut Vec<PathBuf>, out: &mut Vec<Line>) -> Result<(), String> {
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

/// A message of the shared library.
pub struct MsgType {
    pub id: String,
    pub from: String,
    pub to: String,
    pub text: String,
    pub params: String,
    pub reply: bool,
    pub color: Option<String>,
}

impl MsgType {
    pub fn from_table(t: &Table) -> Result<MsgType, String> {
        Ok(MsgType {
            id: t.str_req("id")?,
            from: t.str_req("from")?,
            to: t.str_req("to")?,
            text: t.str_req("text")?,
            params: t.str_opt("params")?.unwrap_or_default(),
            reply: t.bool_opt("reply")?.unwrap_or(false),
            color: t.str_opt("color")?,
        })
    }
}

/// Directive names, for suggestions.
const DIRECTIVES: [&str; 25] = [
    "title", "subtitle", "geometry", "carrier", "include", "on", "after", "at", "color", "for", "id", "duration", "move", "pipette", "rotate", "highlight", "operator", "needle", "status",
    "show", "hide", "wait", "caption", "pass", "divert",
];

fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut prev = row[0];
        row[0] = i;
        for j in 1..=b.len() {
            let cur = row[j];
            row[j] = (row[j] + 1).min(row[j - 1] + 1).min(prev + usize::from(a[i - 1] != b[j - 1]));
            prev = cur;
        }
    }
    row[b.len()]
}

/// The closest directive to a mistyped one.
fn suggest(d: &str) -> Option<&'static str> {
    let head = d.split_whitespace().next().unwrap_or_default();
    DIRECTIVES
        .iter()
        .map(|&n| (distance(head, n), n))
        .filter(|&(dist, n)| dist > 0 && dist <= 2.max(n.len() / 4))
        .min()
        .map(|x| x.1)
}

/// `key=value` options after the words of a directive.
fn options<'a>(words: &[&'a str]) -> (Vec<&'a str>, HashMap<&'a str, &'a str>) {
    let (mut plain, mut opts) = (Vec::new(), HashMap::new());
    for w in words {
        match w.split_once('=') {
            Some((k, v)) => {
                opts.insert(k, v);
            }
            None => plain.push(*w),
        }
    }
    (plain, opts)
}

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

/// Turns a diagram into the data of a scene.
struct Importer {
    doc: Doc,
    /// Where `common/` is; None for a diagram given as text.
    base: Option<PathBuf>,
    files: Vec<PathBuf>,
    started: bool,
    geometry: Option<String>,
    /// Mermaid participant id -> actor id.
    actors: Vec<(String, String)>,
    /// (carrier id, sample id; empty for an empty carrier)
    samples: Vec<(String, String)>,
    /// Timing keys for the next step.
    timing: Keys,
    last_pipette: bool,
    /// Where the line being read comes from: (file, line).
    file: String,
    line: usize,
    /// `%% for`: the carrier of the next message, when its text doesn't say it.
    next_carrier: Option<String>,
    /// `%% on`: the next step starts when a carrier is over this antenna:
    /// (point, antenna, carrier if written).
    on: Option<(String, String, Option<String>)>,
    /// `%% color`: the color of the next message.
    color: Option<String>,
    /// `%% id`: the name of the next step, for `%% after <seconds> <id>`.
    next_id: Option<String>,
    /// The names given so far.
    ids: Vec<String>,
}

impl Importer {
    fn new(base: Option<PathBuf>) -> Importer {
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

    fn add(&mut self, name: &str, keys: Keys) {
        let entries = keys.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        let t = Table { name: name.to_string(), line: self.line, file: self.file.clone(), entries };
        self.doc.add(t);
    }

    fn step(&mut self, action: &str, keys: Keys) -> Result<(), String> {
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

    /// A step has one starting rule: only `delay` is added to another.
    fn no_start_yet(&self) -> Result<(), String> {
        if self.on.is_some() || self.timing.iter().any(|(k, _)| ["when", "at", "after"].contains(k)) {
            return Err("only one of on, after, at for a step".into());
        }
        Ok(())
    }

    fn timing_key(&mut self, key: &'static str, value: Value) -> Result<(), String> {
        if key != "delay" {
            self.no_start_yet()?;
        }
        self.timing.push((key, value));
        Ok(())
    }

    fn list(&self, section: &str) -> &[Table] {
        self.doc.arrays.get(section).map_or(&[], |v| v.as_slice())
    }

    /// Reads the directives of a shared file here.
    fn include(&mut self, path: &Path) -> Result<(), String> {
        let mut lines = Vec::new();
        read_into(path, true, &mut self.files, &mut Vec::new(), &mut lines)?;
        let (file, line) = (std::mem::take(&mut self.file), self.line);
        for l in &lines {
            self.read(l)?;
        }
        (self.file, self.line) = (file, line);
        Ok(())
    }

    fn participant(&mut self, id: &str, label: &str) {
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

    fn actor(&self, id: &str) -> String {
        self.actors.iter().find(|a| a.0 == id).map_or_else(|| id.to_string(), |a| a.1.clone())
    }

    /// One line of a diagram or of a shared file.
    fn read(&mut self, l: &Line) -> Result<(), String> {
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

    fn directive(&mut self, d: &str) -> Result<(), String> {
        if let Some(raw) = d.strip_prefix('@') {
            let (name, entries) = parse_line(raw)?;
            // The steps are written with the directives and the messages of the
            // diagram; the tables of steps are only the program's own (and the tests').
            if name == "step" && self.base.is_some() {
                return Err("'@step' is not written in a scene: use the directives (move, pass, pipette, caption, ...) and the messages of the diagram".into());
            }
            let t = Table { name, line: self.line, file: self.file.clone(), entries };
            self.doc.add(t);
            return Ok(());
        }
        let words: Vec<&str> = d.split_whitespace().collect();
        let Some((&cmd, rest)) = words.split_first() else { return Ok(()) };
        let tail = d[cmd.len()..].trim();
        let (plain, o) = options(rest);
        let need = |n: usize, usage: &str| if plain.len() < n { Err(format!("'{cmd}' is written: {usage}")) } else { Ok(()) };
        match cmd {
            "#" => {}
            "title" | "subtitle" => {
                let key = if cmd == "title" { "title" } else { "subtitle" };
                self.add("video", vec![(key, text(tail))]);
            }
            "geometry" => {
                need(1, "geometry <name>")?;
                let base = self.base.clone().ok_or("'geometry' needs a diagram file, with its common/ folder")?;
                let path = base.join("common").join(format!("{}.mmd", plain[0]));
                if !path.exists() {
                    return Err(format!("geometry '{}': {} not found", plain[0], path.display()));
                }
                self.geometry = Some(plain[0].to_string());
                self.include(&path)?;
            }
            "carrier" => {
                if !o.contains_key("id") {
                    return Err("'carrier' is written: carrier id=<id> [sid=<sample id>] [start=<antenna|point>] [color=#hex] [speed=n] [tag=text] [ghost] [hidden]".into());
                }
                self.carrier(&plain, &o)?;
            }
            "on" => {
                need(1, "on <antenna> [<carrier>]")?;
                self.on_antenna(plain[0], plain.get(1).copied())?;
            }
            "after" => {
                need(1, "after <seconds> [<id> | <antenna> <carrier> | pipette|rotate|needle <device>]")?;
                self.after(seconds("after", plain[0])?, &plain[1..])?;
            }
            "color" => {
                need(1, "color #RRGGBB")?;
                let c = plain[0];
                if !(c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|h| h.is_ascii_hexdigit())) {
                    return Err(format!("'color' needs a color like #9333EA, not '{c}'"));
                }
                self.color = Some(c.to_string());
            }
            "at" => {
                need(1, "at <seconds>")?;
                self.timing_key("at", seconds(cmd, plain[0])?)?;
            }
            "id" => {
                if plain.len() != 1 {
                    return Err("'id' is written: id <name>".into());
                }
                self.name_next(plain[0])?;
            }
            "for" => {
                need(1, "for <carrier>")?;
                self.next_carrier = Some(plain[0].to_string());
            }
            "duration" => {
                need(1, "duration <seconds>")?;
                self.timing.push(("duration", seconds("duration", plain[0])?));
            }
            "move" => {
                if plain.len() < 3 || plain[1] != "to" {
                    return Err("'move' is written: move <carrier> to <antenna>|<point> [ease=in|out|inout|linear] [speed=n] [duration=s] [runaway]".into());
                }
                // An antenna stands for its point.
                let to = self.antennas().into_iter().find(|a| a.0 == plain[2]).map_or_else(|| plain[2].to_string(), |a| a.1);
                let points: Vec<String> = self.list("point").iter().filter_map(|t| t.str_opt("id").ok().flatten()).collect();
                if !points.is_empty() && !points.contains(&to) {
                    let mut names: Vec<String> = self.antennas().into_iter().map(|a| a.0).collect();
                    names.sort();
                    return Err(format!("'move {} to {to}': '{to}' is neither an antenna nor a point of this scene (antennas: {})", plain[0], names.join(", ")));
                }
                let mut k: Keys = vec![("carrier", text(plain[0])), ("to", text(&to))];
                if let Some(e) = o.get("ease") {
                    k.push(("ease", text(e)));
                }
                if let Some(v) = o.get("speed") {
                    k.push(("speed", seconds("speed", v)?));
                }
                k.extend(duration(&o)?);
                if plain.contains(&"runaway") {
                    k.push(("runaway", Value::Bool(true)));
                }
                self.step("move", k)?;
            }
            "pipette" | "rotate" | "highlight" | "operator" => {
                need(1, &format!("{cmd} <device> [duration=s]"))?;
                let device = if cmd == "pipette" { self.probe(cmd, plain[0])? } else { plain[0].to_string() };
                let mut k: Keys = vec![("device", text(&device))];
                k.extend(duration(&o)?);
                // The sample the analyzer aspirates, for its note: sid=.
                match (cmd, o.get("sid")) {
                    ("pipette", Some(v)) => k.push(("sample_id", text(v))),
                    (_, Some(_)) => return Err(format!("'{cmd}' takes no sid: only pipette does")),
                    _ => {}
                }
                self.step(cmd, k)?;
            }
            // The carrier stopped on the antenna goes on (pass) or is diverted (divert).
            "pass" | "divert" => {
                let [name, carrier] = plain[..] else {
                    return Err(format!("'{cmd}' is written: {cmd} <antenna> <carrier> [speed=n] [duration=s]"));
                };
                self.known_carrier(&format!("{cmd} {name} {carrier}"), carrier)?;
                if !self.antennas().iter().any(|a| a.0 == name) {
                    let own = self.list("device").iter().find_map(|t| {
                        (t.str_opt("id").ok()?.as_deref() == Some(name)).then(|| t.str_opt("antenna").ok().flatten()).flatten()
                    });
                    return Err(match own {
                        Some(a) => format!("'{cmd} {name}': write the antenna, {cmd} {a}"),
                        None => self.antenna_point(name).unwrap_err(),
                    });
                }
                let mut k: Keys = vec![("device", text(name)), ("carrier", text(carrier))];
                if cmd == "divert" {
                    k.push(("divert", Value::Bool(true)));
                }
                if let Some(v) = o.get("speed") {
                    k.push(("speed", seconds("speed", v)?));
                }
                k.extend(duration(&o)?);
                self.step("pass", k)?;
            }
            "needle" => {
                need(2, "needle <antenna> down|up [duration=s]")?;
                let device = self.probe(cmd, plain[0])?;
                let mut k: Keys = vec![("device", text(&device)), ("to", text(plain[1]))];
                k.extend(duration(&o)?);
                self.step("needle", k)?;
            }
            "status" => {
                need(2, "status <device> error|ready|normal")?;
                self.step("status", vec![("device", text(plain[0])), ("state", text(plain[1]))])?;
            }
            "show" | "hide" => {
                need(1, &format!("{cmd} <carrier>"))?;
                let mut k: Keys = vec![("carrier", text(plain[0]))];
                k.extend(duration(&o)?);
                self.step(cmd, k)?;
            }
            "wait" => {
                need(1, "wait <seconds>")?;
                self.step("wait", vec![("duration", seconds("wait", plain[0])?)])?;
            }
            "caption" => {
                let (title, body) = tail.split_once('|').ok_or("'caption' is written: caption <title> | <text>")?;
                self.step("caption", vec![("title", text(title.trim())), ("text", text(body.trim()))])?;
            }
            "when" | "with" | "delay" | "after_start" | "after_end" => {
                return Err(format!("'{cmd}' no longer exists: a step starts with on <antenna>, after <seconds> [<what>] or at <seconds>"));
            }
            o => {
                let hint = suggest(d).map_or(String::new(), |s| format!(": did you mean '{s}'?"));
                return Err(format!("unknown directive '{o}'{hint} (for a plain comment write '%% # text')"));
            }
        }
        Ok(())
    }

    /// `%% on <antenna> [<carrier>]`: the next step starts when the carrier is over
    /// the antenna; without a carrier, the one of the step (its SampleId, or the
    /// carrier it moves or shows).
    /// The antennas of the scene, also those declared on a device: (id, point).
    fn antennas(&self) -> Vec<(String, String)> {
        let field = |t: &Table, k: &str| t.str_opt(k).ok().flatten().unwrap_or_default();
        self.list("device")
            .iter()
            .filter_map(|t| match (field(t, "kind").as_str(), field(t, "antenna")) {
                ("antenna", _) => Some((field(t, "id"), field(t, "at"))),
                // The antenna of a turnstile is under the needle.
                ("turnstile", a) if !a.is_empty() => Some((a, format!("{}.needle", field(t, "id")))),
                // An antenna declared on another device: antenna=antenna2.
                (_, a) if !a.is_empty() => Some((a, field(t, "at"))),
                _ => None,
            })
            .collect()
    }

    /// The antenna under the needle of a turnstile: the wheel brings carriers
    /// there, so a step can't wait for one to get on it.
    fn not_in_a_wheel(cmd: &str, antenna: &str, point: &str) -> Result<(), String> {
        match point.strip_suffix(".needle") {
            Some(ts) => Err(format!("'{cmd} {antenna}': the wheel brings the carrier under the needle; start from the rotation: after <seconds> rotate {ts}")),
            None => Ok(()),
        }
    }

    /// The point an antenna reads.
    fn antenna_point(&self, antenna: &str) -> Result<String, String> {
        let antennas = self.antennas();
        antennas.iter().find(|a| a.0 == antenna).map(|a| a.1.clone()).ok_or_else(|| {
            let mut names: Vec<&str> = antennas.iter().map(|a| a.0.as_str()).collect();
            names.sort();
            format!("'{antenna}' is not an antenna of this scene (antennas: {})", if names.is_empty() { "none".to_string() } else { names.join(", ") })
        })
    }

    /// Where `pipette` and `needle` work: a sampling point by its antenna (the
    /// device's own name is refused), a turnstile by its name (no antenna there).
    fn probe(&self, cmd: &str, name: &str) -> Result<String, String> {
        let field = |t: &Table, k: &str| t.str_opt(k).ok().flatten().unwrap_or_default();
        let devices = self.list("device");
        if let Some(t) = devices.iter().find(|t| field(t, "antenna") == name) {
            return match field(t, "kind").as_str() {
                "sampling" | "turnstile" => Ok(field(t, "id")),
                _ => Err(format!("'{cmd} {name}': {name} is not on a sampling point")),
            };
        }
        match devices.iter().find(|t| field(t, "id") == name) {
            Some(t) if ["sampling", "turnstile"].contains(&field(t, "kind").as_str()) && !field(t, "antenna").is_empty() => {
                Err(format!("'{cmd} {name}': write the antenna, {cmd} {}", field(t, "antenna")))
            }
            // A turnstile, or a sampling point without an antenna: by its name.
            Some(t) if ["turnstile", "sampling"].contains(&field(t, "kind").as_str()) => Ok(name.to_string()),
            Some(_) => Err(format!("'{cmd} {name}': {name} is not on a sampling point")),
            None if self.antennas().iter().any(|a| a.0 == name) => Err(format!("'{cmd} {name}': {name} is not on a sampling point")),
            None => Err(format!("'{cmd} {name}': not an antenna of this scene")),
        }
    }

    fn known_carrier(&self, what: &str, c: &str) -> Result<(), String> {
        if self.samples.iter().any(|s| s.0 == c) {
            Ok(())
        } else {
            Err(format!("'{what}': no carrier '{c}'"))
        }
    }

    fn on_antenna(&mut self, antenna: &str, carrier: Option<&str>) -> Result<(), String> {
        let point = self.antenna_point(antenna)?;
        Self::not_in_a_wheel("on", antenna, &point)?;
        if let Some(c) = carrier {
            self.known_carrier(&format!("on {antenna} {c}"), c)?;
        }
        self.no_start_yet()?;
        self.on = Some((point, antenna.to_string(), carrier.map(String::from)));
        Ok(())
    }

    /// `%% after <seconds>`: after the end of the step before. With what it waits
    /// for: `<id>` when the step named so with `%% id` ends; `<antenna> <carrier>`
    /// when the carrier is over the antenna; `pipette|rotate|needle <device>` when
    /// that action ends.
    fn after(&mut self, secs: Value, refer: &[&str]) -> Result<(), String> {
        if refer.is_empty() {
            return self.timing_key("after", secs);
        }
        if self.antennas().iter().any(|a| a.0 == refer[0]) {
            let [antenna, c] = refer else {
                return Err(format!("'after <seconds> {}' needs the carrier: after <seconds> {} <carrier>", refer[0], refer[0]));
            };
            let point = self.antenna_point(antenna)?;
            Self::not_in_a_wheel("after <seconds>", antenna, &point)?;
            self.known_carrier(&format!("after {antenna} {c}"), c)?;
            self.timing_key("when", text(&format!("{c} at {point}")))?;
            return self.timing_key("delay", secs);
        }
        if let [kind @ ("pipette" | "rotate" | "needle"), device] = refer {
            let device = if *kind == "rotate" { device.to_string() } else { self.probe(kind, device)? };
            self.timing_key("when", text(&format!("{kind} {device} done")))?;
            return self.timing_key("delay", secs);
        }
        let [id] = refer else {
            return Err(format!(
                "'after <seconds> {}': write the id of a line (after <seconds> <id>, with %% id <id> above that line), an antenna and a carrier, or pipette|rotate|needle <device>",
                refer.join(" ")
            ));
        };
        if !self.ids.iter().any(|i| i == id) {
            return Err(format!("'after': no line with id '{id}' before this one (write %% id {id} above the line to wait for)"));
        }
        self.timing_key("when", text(&format!("step {id} done")))?;
        self.timing_key("delay", secs)
    }

    /// `%% id <name>`: names the next step, so that a later `%% after` can wait for it.
    fn name_next(&mut self, id: &str) -> Result<(), String> {
        if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err(format!("an id has only letters, digits, '-' and '_': '{id}'"));
        }
        if ["pipette", "rotate", "needle"].contains(&id) || self.antennas().iter().any(|a| a.0 == id) {
            return Err(format!("'{id}' is already the name of an antenna or an action: choose another id"));
        }
        if self.ids.iter().any(|i| i == id) {
            return Err(format!("the id '{id}' is already used: each line has its own"));
        }
        if let Some(prev) = &self.next_id {
            return Err(format!("'id {id}' after 'id {prev}': a line has one id"));
        }
        self.ids.push(id.to_string());
        self.next_id = Some(id.to_string());
        Ok(())
    }

    fn carrier(&mut self, plain: &[&str], o: &HashMap<&str, &str>) -> Result<(), String> {
        // Without sid= the carrier is empty: no tube, no sample.
        let (id, sample) = (o["id"], o.get("sid").copied());
        if o.contains_key("route") || o.contains_key("needle") {
            return Err("a carrier has no route= or needle=: its way comes from its move, pass and divert; where it starts, if not the default: start=<antenna|point>".into());
        }
        // Where it starts: the first carrier on the Main-Lane, the others off the
        // panel (if the geometry has a place for them); an antenna stands for its point.
        let start = match o.get("start") {
            Some(s) => self.antennas().into_iter().find(|a| a.0 == *s).map_or_else(|| s.to_string(), |a| a.1),
            None if !self.samples.is_empty() && self.list("point").iter().any(|t| t.str_opt("id").ok().flatten().as_deref() == Some("spawn")) => "spawn".to_string(),
            None => "main_in".to_string(),
        };
        let mut k: Keys = vec![("id", text(id)), ("tube", Value::Bool(sample.is_some())), ("start", text(&start))];
        if let Some(sid) = sample {
            k.push(("sample_id", text(sid)));
        }
        // Without speed= the carrier goes with the belt.
        if let Some(s) = o.get("speed") {
            k.push(("speed", seconds("speed", s)?));
        }
        if let Some(col) = o.get("color") {
            k.push(("cap_color", text(col)));
        }
        if plain.contains(&"ghost") {
            k.push(("ghost", Value::Bool(true)));
        }
        if plain.contains(&"hidden") {
            k.push(("visible", Value::Bool(false)));
        }
        if let Some(t) = o.get("tag") {
            k.push(("tag", text(&t.replace('_', " "))));
        }
        self.add("carrier", k);
        self.samples.push((id.to_string(), sample.unwrap_or_default().to_string()));
        Ok(())
    }

    fn message(&mut self, line: &str) -> Result<(), String> {
        let (left, label) = line.split_once(':').ok_or("a message needs ': text'")?;
        let arrow = ["-->>", "->>", "--x", "-x", "--)", "-)", "-->", "->"].into_iter().find(|a| left.contains(a)).ok_or("no arrow in the message")?;
        let (a, b) = left.split_once(arrow).unwrap();
        let (a, b) = (a.trim(), b.trim());
        self.participant(a, a);
        self.participant(b, b);
        let (from, to) = (self.actor(a), self.actor(b));
        let reply = arrow.starts_with("--");
        // "Sample Complete SampleId=X Needle=1": the parameters start at the first key=value.
        // `<br/>` starts a new line of the label (drawn so): it also ends a word.
        let mut label = label.to_string();
        for br in ["<br/>", "<br />", "<br>", "<BR/>", "<BR />", "<BR>"] {
            label = label.replace(br, " <br/> ");
        }
        let words: Vec<&str> = label.split_whitespace().collect();
        let split = words.iter().position(|w| w.contains('=')).unwrap_or(words.len());
        let (body, params) = (words[..split].join(" "), &words[split..]);
        if body.is_empty() {
            return Err("a message needs a name".into());
        }
        let value = |key: &str| params.iter().find_map(|p| p.strip_prefix(key)?.strip_prefix('='));
        let sample = value("SampleId");
        let carrier = match (self.next_carrier.take(), sample) {
            (Some(c), _) => Some(c),
            (None, Some(s)) => Some(self.samples.iter().find(|c| c.1 == s).ok_or_else(|| format!("no carrier has sample id '{s}'"))?.0.clone()),
            _ => None,
        };
        let needle = value("Needle");
        // The same message in the library's terms.
        let shape: Vec<String> = params
            .iter()
            .map(|p| match p.split_once('=') {
                Some(("SampleId", _)) => "SampleId={SampleId}".to_string(),
                Some(("Needle", _)) => "Needle={Needle}".to_string(),
                _ => p.to_string(),
            })
            .collect();
        let shape = shape.join(" ");
        let color = self.color.take();
        let types: Vec<MsgType> = self.list("message").iter().map(MsgType::from_table).collect::<Result<_, _>>()?;
        let found = types.iter().find(|m| m.from == from && m.to == to && m.text == body && m.params == shape && m.reply == reply && m.color == color);
        let mut k: Keys = Vec::new();
        match found {
            Some(m) => k.push(("type", text(&m.id))),
            None => {
                k.extend([("from", text(&from)), ("to", text(&to)), ("text", text(&body))]);
                if !shape.is_empty() {
                    k.push(("params", text(&shape)));
                }
                if reply {
                    k.push(("reply", Value::Bool(true)));
                }
                if let Some(c) = &color {
                    k.push(("color", text(c)));
                }
            }
        }
        if let Some(c) = &carrier {
            k.push(("carrier", text(c)));
        }
        let needle_n = match needle {
            Some(n) => Some(n.parse::<f64>().map_err(|_| format!("Needle={n} is not a number"))?),
            None => None,
        };
        if let Some(n) = needle_n {
            k.push(("needle", Value::Num(n)));
        }
        let event = found.map_or_else(|| body.to_lowercase().replace(' ', "_"), |m| m.id.clone());
        if found.is_none() {
            k.push(("event", text(&event)));
        }
        self.step("message", k)
    }

    fn note(&mut self, line: &str) -> Result<(), String> {
        let (place, body) = line.split_once(':').ok_or("a note needs ': text'")?;
        let who = place.split_whitespace().last().unwrap_or_default().split(',').next().unwrap_or_default().to_string();
        self.participant(&who, &who);
        // A note right after a pipette is the text of its note on the analyzer.
        if self.last_pipette {
            self.last_pipette = false;
            let step = self.doc.arrays.get_mut("step").and_then(|v| v.last_mut()).ok_or("a note after a pipette needs the pipette")?;
            step.entries.insert("note".to_string(), text(body.trim()));
            return Ok(());
        }
        let actor = self.actor(&who);
        self.step("note", vec![("actor", text(&actor)), ("text", text(body.trim()))])
    }

    fn finish(mut self, name: &str) -> Result<Doc, String> {
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
#[cfg(test)]
pub fn parse_text(src: &str) -> Result<Doc, String> {
    let mut im = Importer::new(None);
    for (i, t) in src.lines().enumerate() {
        im.read(&Line { file: String::new(), no: i + 1, text: t.to_string() })?;
    }
    im.finish("test")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Ev, Scene, Timeline};

    /// The messages and notes of a timeline as Mermaid arrows, in the order of the
    /// sequence diagram rows.
    fn arrows(sc: &Scene, tl: &Timeline) -> Vec<String> {
        let mut rows: Vec<(usize, String)> = tl
            .events
            .iter()
            .filter_map(|e| match &e.ev {
                Ev::Message { from, to, text, params, reply, row, .. } => {
                    let arrow = if *reply { "-->>" } else { "->>" };
                    let label = if params.is_empty() { text.clone() } else { format!("{text} {params}") };
                    Some((*row, format!("{}{arrow}{}: {label}", sc.actors[*from].id, sc.actors[*to].id)))
                }
                Ev::Note { actor, text, row } => Some((*row, format!("Note over {}: {text}", sc.actors[*actor].id))),
                _ => None,
            })
            .collect();
        rows.sort_by_key(|r| r.0);
        rows.into_iter().map(|r| r.1).collect()
    }

    const SCENES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/scenes");

    /// A diagram given as text, with the shared files of scenes/common.
    fn doc(src: &str) -> Result<Doc, String> {
        let mut im = Importer::new(Some(PathBuf::from(SCENES)));
        for name in SHARED {
            im.include(&Path::new(SCENES).join("common").join(format!("{name}.mmd")))?;
        }
        for (i, t) in src.lines().enumerate() {
            im.read(&Line { file: String::new(), no: i + 1, text: t.to_string() })?;
        }
        im.finish("test")
    }

    fn build(d: &Doc) -> (Scene, Timeline) {
        let sc = Scene::from_doc(d).unwrap_or_else(|e| panic!("{e}"));
        let tl = Timeline::build(&sc).unwrap_or_else(|e| panic!("{e}"));
        (sc, tl)
    }

    /// Does a table of the section hold all these keys with these values?
    fn has(d: &Doc, section: &str, keys: &[(&str, &str)]) -> bool {
        let same = |v: &Value, want: &str| match v {
            Value::Str(s) => s == want,
            Value::Num(n) => want.parse::<f64>().is_ok_and(|w| w == *n),
            Value::Bool(b) => want == if *b { "true" } else { "false" },
            Value::Arr(_) => false,
        };
        d.arrays.get(section).is_some_and(|ts| ts.iter().any(|t| keys.iter().all(|(k, w)| t.entries.get(*k).is_some_and(|v| same(v, w)))))
    }

    fn fails(src: &str, what: &str) {
        let e = doc(src).err().unwrap_or_else(|| panic!("expected an error with '{what}'"));
        assert!(e.contains(what), "'{e}' does not contain '{what}'");
    }

    /// The arrows of a diagram, without comments and layout.
    fn lines(s: &str) -> Vec<String> {
        s.lines().map(str::trim).filter(|l| l.contains("->>") || l.starts_with("Note")).filter(|l| !l.starts_with("%%")).map(String::from).collect()
    }

    const EX: &str = "%% geometry spur\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\nIF->>AN: Prepare SampleId=SID-1\nAN-->>IF: OK SampleId=SID-1\nAN->>DMX: Query SampleId=SID-1\nDMX-->>AN: Tests to perform SampleId=SID-1\nIF->>AN: Sample Acquisition SampleId=SID-1\nAN-->>IF: OK\n%% pipette antenna2\nNote over AN: Pipetting\nAN->>IF: Sample Complete sid=SID-1\nIF-->>AN: OK\n";

    #[test]
    fn a_diagram_builds_a_valid_scene_with_the_messages_written() {
        let (sc, tl) = build(&doc(EX).unwrap());
        assert!(crate::lint::warnings(&sc, &tl).is_empty(), "{:?}", crate::lint::warnings(&sc, &tl));
        assert_eq!(arrows(&sc, &tl), lines(EX));
    }

    const HEAD: &str = "%% geometry spur\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n";

    #[test]
    fn messages_use_the_library_and_color_picks_the_purple_prepare() {
        let src = format!("{HEAD}IF->>AN: Prepare SampleId=SID-1\nAN-->>IF: OK SampleId=SID-1\n%% color #9333EA\nIF->>AN: Prepare SampleId=SID-1\nIF->>AN: Sample Acquisition SampleId=SID-1 Needle=2\n%% color #DC2626\nIF->>AN: Prepare SampleId=SID-1\n");
        let d = doc(&src).unwrap();
        for ty in ["prepare", "sample_ok", "prepare_again"] {
            assert!(has(&d, "step", &[("type", ty), ("carrier", "C1")]), "{ty}");
        }
        assert!(has(&d, "step", &[("type", "sample_acquisition_needle"), ("needle", "2")]));
        // Another color is not a library message: the text with that color.
        assert!(has(&d, "step", &[("text", "Prepare"), ("color", "#DC2626")]));
    }

    #[test]
    fn participants_are_found_by_label_and_new_ones_are_added() {
        let d = doc("%% geometry spur\nsequenceDiagram\nparticipant I as Interface\nparticipant L as Lab\n%% carrier id=C1 sid=SID-1\nI->>AN: Prepare SampleId=SID-1\nI->>L: Hello\n").unwrap();
        assert!(has(&d, "step", &[("type", "prepare")]));
        assert!(has(&d, "actor", &[("id", "L"), ("label", "Lab")]));
        assert!(has(&d, "step", &[("from", "IF"), ("to", "L"), ("text", "Hello")]));
    }

    #[test]
    fn timing_directives_apply_to_the_next_step() {
        let d = doc(&format!("{HEAD}%% move C1 to nsd ease=in\n%% on antenna1\nIF->>AN: Prepare SampleId=SID-1\n%% at 3\n%% highlight nsd duration=2\n%% after 0.5\n%% caption a | b\n")).unwrap();
        assert!(has(&d, "step", &[("when", "C1 at nsd"), ("type", "prepare")]));
        assert!(has(&d, "step", &[("at", "3"), ("device", "nsd"), ("duration", "2")]));
        assert!(has(&d, "step", &[("after", "0.5"), ("action", "caption")]));
        assert!(has(&d, "step", &[("action", "move"), ("ease", "in")]) && !has(&d, "step", &[("action", "move"), ("when", "C1 at nsd")]));
    }

    #[test]
    fn for_duration_and_runaway() {
        let d = doc(&format!("{HEAD}%% move C1 to nsd runaway\n%% for C1\nAN-->>IF: OK\n%% duration 6\nNote over AN: Busy\n")).unwrap();
        assert!(has(&d, "step", &[("action", "move"), ("runaway", "true")]));
        assert!(has(&d, "step", &[("type", "analyzer_ok"), ("carrier", "C1")]));
        assert!(has(&d, "step", &[("action", "note"), ("duration", "6"), ("actor", "AN")]));
    }

    const ON: &str = "%% geometry spur2\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n%% carrier id=C2 sid=SID-2\n\
        %% move C1 to d1\n%% move C1 to s1\n%% at 0\n%% move C2 to w1\n\
        %% on antenna1\n%% id tube1\nIF->>AN: Add Tube SampleId=SID-1\nAN-->>IF: Fine\nAN->>DMX: Which tests? SampleId=SID-1\nDMX-->>AN: These\n\
        %% on antenna1 C2\n%% caption Second | The next one.\n%% on antenna1\nIF->>AN: Add Tube SampleId=SID-2\n%% after 0.2 tube1\nAN-->>IF: Late\n\
        %% color #9333EA\n%% on antenna3\nIF->>AN: Again SampleId=SID-1\n%% after 1\nIF->>AN: Go SampleId=SID-1\n\
        %% after 0.5 antenna3 C1\n%% caption Read | x\n%% pipette antenna4 duration=2\n%% after 0.2 pipette antenna4\nAN->>IF: Done sid=SID-1\n";

    #[test]
    fn on_after_and_color_set_the_moment_and_the_look() {
        let d = doc(ON).unwrap();
        // The text as written, at the antenna of its carrier.
        assert!(has(&d, "step", &[("text", "Add Tube"), ("params", "SampleId={SampleId}"), ("carrier", "C1"), ("when", "C1 at nsd")]));
        assert!(has(&d, "step", &[("action", "caption"), ("when", "C2 at nsd")]) && has(&d, "step", &[("carrier", "C2"), ("when", "C2 at nsd")]));
        assert!(has(&d, "step", &[("text", "Again"), ("when", "C1 at d1"), ("color", "#9333EA")]));
        assert!(has(&d, "step", &[("text", "Add Tube"), ("id", "tube1")]));
        assert!(has(&d, "step", &[("text", "Late"), ("when", "step tube1 done"), ("delay", "0.2")]));
        assert!(has(&d, "step", &[("text", "Go"), ("after", "1")]));
        assert!(has(&d, "step", &[("action", "caption"), ("when", "C1 at d1"), ("delay", "0.5")]));
        assert!(has(&d, "step", &[("text", "Done"), ("when", "pipette sp1 done"), ("delay", "0.2")]));
        let (_, tl) = build(&d);
        let at = |name: &str| tl.events.iter().find(|e| matches!(&e.ev, Ev::Message { text, .. } if text == name)).unwrap().t0;
        let same = |a: f64, b: f64| assert!((a - b).abs() < 0.011, "{a} != {b}");
        // Without a directive a message starts when the one before has arrived.
        same(at("Fine"), at("Add Tube") + 1.5);
        same(at("Late"), at("Add Tube") + 1.5 + 0.2);
        same(at("Go"), at("Again") + 1.5 + 1.0);
    }

    #[test]
    fn after_an_id_waits_for_the_end_of_that_line() {
        // The id of a move: it ends when the carrier arrives.
        let d = doc(&format!("{ON_HEAD}%% id go\n%% move C1 to d1\nIF->>AN: Here SampleId=SID-1\n%% id ok\nAN-->>IF: Fine\n%% after 0.5 go\nIF->>AN: Later SampleId=SID-1\n%% after 0.25 ok\nIF->>AN: Last SampleId=SID-1\n")).unwrap();
        assert!(has(&d, "step", &[("action", "move"), ("id", "go")]));
        let (_, tl) = build(&d);
        let at = |name: &str| tl.events.iter().find(|e| matches!(&e.ev, Ev::Message { text, .. } if text == name)).unwrap().t0;
        let same = |a: f64, b: f64| assert!((a - b).abs() < 0.011, "{a} != {b}");
        same(at("Later"), at("Here") + 0.5);
        same(at("Last"), at("Fine") + 1.5 + 0.25);
    }

    #[test]
    fn br_also_ends_a_word() {
        // The parameters are read across the lines of the label.
        let d = doc(&format!("{ON_HEAD}IF->>AN: OK SampleId=SID-1<br/>Needle=1\nIF->>AN: EAC^U07<br/>SAC-3: SID-1\n")).unwrap();
        assert!(has(&d, "step", &[("text", "OK"), ("carrier", "C1"), ("needle", "1"), ("params", "SampleId={SampleId} <br/> Needle={Needle}")]));
        assert!(has(&d, "step", &[("text", "EAC^U07 <br/> SAC-3: SID-1")]));
    }

    #[test]
    fn waiting_for_a_carrier_stuck_in_a_queue_names_the_one_in_front() {
        let src = "%% geometry spur2\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n%% carrier id=C2 sid=SID-2\n\
            %% move C1 to antenna2\n%% at 1\n%% move C2 to antenna2\n%% on antenna2 C2\nIF->>AN: A\n";
        let sc = Scene::from_doc(&doc(src).unwrap()).unwrap();
        let e = Timeline::build(&sc).err().unwrap();
        assert!(e.contains("'C2' never gets to antenna2: after line 7 it stops behind 'C1', which stays on antenna2 because no step in the file sends it on (e.g. pass antenna2 C1)"), "{e}");
    }

    #[test]
    fn a_fast_carrier_goes_in_before_a_slow_one_that_gets_to_the_merge_later() {
        // C1 is sampled on needle 1; C2 creeps up the spur. When C1 leaves its pit-lane
        // it gets to the merge (rj1) long before C2: it goes in front, C2 follows.
        let src = "%% geometry spur2\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n%% carrier id=C2 sid=SID-2\n\
            %% move C1 to antenna3\n%% at 0.5\n%% move C2 to antenna3\n%% on antenna3 C1\n%% divert antenna3 C1\n\
            %% on antenna4 C1\n%% id at-s1\n%% pass antenna3 C2 duration=15\n%% after 1 at-s1\n%% pass antenna4 C1\n\
            %% on antenna5 C1\n%% pass antenna5 C1\n%% on antenna5 C2\n%% pass antenna5 C2\n";
        let sc = Scene::from_doc(&doc(src).unwrap()).unwrap();
        let tl = Timeline::build(&sc).unwrap();
        let at_d2 = |c: &str| -> f64 {
            let l = tl.log.iter().find(|l| l.contains(&format!("move      {c} -> d2"))).unwrap();
            assert!(!l.contains("waits"), "{l}");
            l.split_whitespace().nth(2).unwrap().trim_end_matches('s').parse().unwrap()
        };
        assert!(at_d2("C1") < at_d2("C2"), "{:?}", tl.log);
    }

    #[test]
    fn a_pass_written_before_the_pass_of_the_carrier_in_front_is_right() {
        // C1 and C2 both on needle 1; the pass of C2 is written before the one of C1,
        // which happens earlier: when C2 is on antenna4, C1 has already gone.
        let src = "%% geometry spur2\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n%% carrier id=C2 sid=SID-2\n\
            %% move C1 to antenna3\n%% at 0.5\n%% move C2 to antenna3\n%% on antenna3 C1\n%% divert antenna3 C1\n\
            %% on antenna3 C2\n%% divert antenna3 C2\n%% on antenna4 C2\n%% pass antenna4 C2\n\
            %% on antenna4 C1\n%% id at-s1\n%% caption a | b\n%% after 1 at-s1\n%% pass antenna4 C1\n";
        let sc = Scene::from_doc(&doc(src).unwrap()).unwrap();
        let tl = Timeline::build(&sc).unwrap_or_else(|e| panic!("{e}"));
        assert!(tl.log.iter().any(|l| l.contains("C2 -> d2")), "{:?}", tl.log);
    }

    #[test]
    fn pass_and_divert_take_a_duration_or_a_speed() {
        let head = "%% geometry spur2\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n%% move C1 to antenna3\n";
        // How long the trip after the pass takes (the last move in the log).
        let trip = |body: &str| {
            let sc = Scene::from_doc(&doc(&format!("{head}{body}")).unwrap()).unwrap();
            let tl = Timeline::build(&sc).unwrap();
            let l = tl.log.iter().rfind(|l| l.contains("move      C1")).unwrap().clone();
            let t: Vec<f64> = l.split_whitespace().take(3).filter(|w| *w != "-").map(|w| w.trim_end_matches('s').parse().unwrap()).collect();
            t[1] - t[0]
        };
        let free = trip("%% divert antenna3 C1\n");
        assert!((trip("%% divert antenna3 C1 duration=4\n") - 4.0).abs() < 0.02);
        assert!((trip("%% pass antenna3 C1 duration=6\n") - 6.0).abs() < 0.02);
        // Half the speed of the belt: twice the time.
        let belt = Scene::from_doc(&doc(head).unwrap()).unwrap().carriers[0].speed;
        assert!((trip(&format!("%% divert antenna3 C1 speed={}\n", belt / 2.0)) - 2.0 * free).abs() < 0.05);
        fails(&format!("{head}%% pass antenna3 C1 speed=fast\n"), "speed");
    }

    #[test]
    fn the_exit_of_spur2_sends_a_carrier_out_or_round_again() {
        let head = "%% geometry spur2\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n";
        let lap = "%% pass antenna3 C1\n%% on antenna5 C1\n%% pass antenna5 C1\n%% on antenna7 C1\n";
        let body = format!("%% move C1 to antenna2\n%% pass antenna2 C1\n%% on antenna3 C1\n{lap}%% divert antenna7 C1\n%% on antenna3 C1\n%% caption a | b\n{lap}%% pass antenna7 C1\n");
        let sc = Scene::from_doc(&doc(&format!("{head}{body}")).unwrap()).unwrap();
        let route = sc.carriers[0].route.join(",");
        // divert: the return belt, back on the spur between antenna2 and antenna3; pass: to the Main-Lane.
        assert!(route.contains("ex,rcj,w1,d1"), "{route}");
        assert_eq!(route.matches("spur_rd").count(), 1, "{route}");
        assert!(route.ends_with("ex,po,merge,iface_out,main_out,gone"), "{route}");
        let tl = Timeline::build(&sc).unwrap();
        // The second `on antenna3 C1` waits for the second arrival there, after the lap.
        let cap = tl.events.iter().find(|e| matches!(e.ev, Ev::Caption { .. })).unwrap().t0;
        let arrivals: Vec<f64> = tl
            .log
            .iter()
            .filter(|l| l.contains("C1 -> d1"))
            .map(|l| l.split_whitespace().nth(2).unwrap().trim_end_matches('s').parse().unwrap())
            .collect();
        assert_eq!(arrivals.len(), 2);
        assert!((cap - arrivals[1]).abs() < 0.01, "caption at {cap}, arrivals {arrivals:?}");
    }

    #[test]
    fn pass_goes_on_and_divert_turns_into_the_lane() {
        // At Divert 1 (antenna3) C1 turns into needle 1, C2 goes on to Divert 2.
        let head = "%% geometry spur2\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n%% carrier id=C2 sid=SID-2\n";
        let d = doc(&format!("{head}%% divert antenna3 C1\n")).unwrap();
        assert!(has(&d, "step", &[("action", "pass"), ("device", "antenna3"), ("carrier", "C1"), ("divert", "true")]));
        let run = |body: &str| Scene::from_doc(&doc(&format!("{head}{body}")).unwrap()).and_then(|sc| Timeline::build(&sc).map(|_| sc));
        let sc = run("%% move C1 to antenna3\n%% divert antenna3 C1\n%% move C2 to antenna3\n%% pass antenna3 C2\n%% on antenna5 C2\n%% divert antenna5 C2\n").unwrap();
        let tail = |ci: usize, n: usize| sc.carriers[ci].route[sc.carriers[ci].route.len() - n..].join(",");
        assert_eq!(tail(0, 5), "d1,q2,pc1,q1,s1");
        assert_eq!(tail(1, 8), "d1,rj1,w2,d2,qb2,pc2,qb1,s2");
        let err = |body: &str, what: &str| {
            let e = run(body).err().unwrap_or_else(|| panic!("expected an error with '{what}'"));
            assert!(e.contains(what), "'{e}' does not contain '{what}'");
        };
        // A ghost too: it starts on Divert 1 and turns into needle 1.
        let g = Scene::from_doc(&doc(&format!("{head}%% carrier id=C2L sid=SID-2 start=antenna3 ghost\n%% divert antenna3 C2L\n")).unwrap()).unwrap();
        Timeline::build(&g).unwrap();
        assert_eq!(g.carriers[2].route.join(","), "d1,q2,pc1,q1,s1");
        // Antenna 2 can't divert.
        err("%% move C1 to antenna2\n%% divert antenna2 C1\n", "divert antenna2 C1: no lane turns off there: pass antenna2 C1");
        assert!(run("%% move C1 to antenna2\n%% pass antenna2 C1\n").is_ok());
        // The carrier written must be the one stopped there.
        err("%% move C1 to antenna3\n%% move C2 to antenna3\n%% pass antenna3 C2\n", "the carrier stopped there at t=");
        fails(&format!("{head}%% divert antenna3\n"), "'divert' is written: divert <antenna> <carrier>");
        fails(&format!("{head}%% pass antenna3 C9\n"), "no carrier 'C9'");
    }

    #[test]
    fn a_ghost_stops_behind_the_carrier_in_front_but_is_driven_through() {
        let log = |carriers: &str, body: &str| {
            let d = doc(&format!("%% geometry spur2\nsequenceDiagram\n{carriers}{body}")).unwrap();
            build(&d).1.log.join("\n")
        };
        // C1 waits at Divert 1; the ghost of C2 goes there too and stops behind it.
        let l = log("%% carrier id=C1 sid=SID-1\n%% carrier id=C2 sid=SID-2\n%% carrier id=C2L sid=SID-2 start=antenna1 ghost\n", "%% move C1 to antenna3\n%% move C2L to antenna3\n");
        assert!(l.contains("C2L -> d1  (stops behind C1"), "{l}");
        // A ghost standing on antenna 2: C1 drives through it.
        let l = log("%% carrier id=C1 sid=SID-1\n%% carrier id=C2L sid=SID-2 start=antenna2 ghost\n", "%% move C1 to antenna3\n");
        assert!(l.contains("move      C1 -> d1\n") || l.ends_with("move      C1 -> d1"), "{l}");
    }

    const ON_HEAD: &str = "%% geometry spur2\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n";

    #[test]
    fn on_after_and_color_errors() {
        let k = |body: &str| format!("{ON_HEAD}{body}");
        fails(&k("%% on antena1\nIF->>AN: X SampleId=SID-1\n"), "not an antenna of this scene (antennas: antenna1, antenna2");
        fails(&k("%% on antenna1 C9\n%% caption a | b\n"), "no carrier 'C9'");
        fails(&k("%% on antenna1\n%% caption a | b\n"), "write the carrier, on antenna1 <carrier>");
        fails(&k("%% on antenna1\n%% at 3\nIF->>AN: X SampleId=SID-1\n"), "only one of");
        fails(&k("%% after 1\n%% on antenna1\nIF->>AN: X SampleId=SID-1\n"), "only one of");
        fails(&k("%% after 1 antenna1\nIF->>AN: X SampleId=SID-1\n"), "needs the carrier");
        fails(&k("%% after 1 antenna1 C9\nIF->>AN: X SampleId=SID-1\n"), "no carrier 'C9'");
        fails(&k("%% on antenna1\n"), "its step is missing");
        fails(&k("%% after 1 nothing\nIF->>AN: X\n"), "no line with id 'nothing' before this one");
        fails(&k("%% after 1 Add Tube C1\nIF->>AN: X\n"), "write the id of a line");
        fails(&k("IF->>AN: X\n%% after 1 later\n%% id later\nIF->>AN: Y\n"), "no line with id 'later'");
        fails(&k("%% id a b\nIF->>AN: X\n"), "'id' is written: id <name>");
        fails(&k("%% id a.b\nIF->>AN: X\n"), "only letters, digits");
        fails(&k("%% id antenna1\nIF->>AN: X\n"), "already the name of an antenna");
        fails(&k("%% id a\nIF->>AN: X\n%% id a\nIF->>AN: Y\n"), "the id 'a' is already used");
        fails(&k("%% id a\n%% id b\nIF->>AN: X\n"), "a line has one id");
        fails(&k("%% id a\n"), "'id a' at the end of the diagram");
        fails(&k("%% color blue\nIF->>AN: X\n"), "like #9333EA");
        fails(&k("%% color #9333EA\n%% caption a | b\n"), "applies to the next message");
        fails(&k("%% color #9333EA\n"), "its message is missing");
        fails(&k("%% Prepare 1\nIF->>AN: X SampleId=SID-1\n"), "unknown directive 'Prepare'");
        fails(&k("%% onn antenna1\nIF->>AN: X SampleId=SID-1\n"), "did you mean 'on'");
    }

    #[test]
    fn a_note_after_a_pipette_is_its_text() {
        let d = doc(&format!("{HEAD}%% pipette antenna2\nNote over AN: Aspirating SID-1\nNote over AN: Other\n")).unwrap();
        assert_eq!(d.arrays["step"].iter().filter(|t| t.str_req("action").unwrap() == "note").count(), 1);
        assert!(has(&d, "step", &[("action", "pipette"), ("note", "Aspirating SID-1")]));
        assert!(has(&doc(&format!("{HEAD}%% pass antenna2 C1\n")).unwrap(), "step", &[("action", "pass"), ("device", "antenna2"), ("carrier", "C1")]));
        // Only antennas, not the names of the devices.
        fails(&format!("{HEAD}%% pass sp C1\n"), "'pass sp': write the antenna, pass antenna2");
        // pipette and needle: the antenna of the sampling point.
        assert!(has(&doc(&format!("{HEAD}%% pipette antenna2\n%% needle antenna2 down\n")).unwrap(), "step", &[("action", "needle"), ("device", "sp")]));
        fails(&format!("{HEAD}%% pipette sp\n"), "'pipette sp': write the antenna, pipette antenna2");
        fails(&format!("{HEAD}%% needle antenna1 down\n"), "'needle antenna1': antenna1 is not on a sampling point");
        fails(&format!("{HEAD}%% after 1 pipette sp\nIF->>AN: X\n"), "'pipette sp': write the antenna, pipette antenna2");
        // The turnstile: its antenna is under the needle.
        let ts = "%% geometry turnstile\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n";
        assert!(has(&doc(&format!("{ts}%% pipette antenna3\n")).unwrap(), "step", &[("action", "pipette"), ("device", "ts")]));
        fails(&format!("{ts}%% pipette ts\n"), "'pipette ts': write the antenna, pipette antenna3");
        fails(&format!("{ts}%% on antenna3 C1\n%% caption a | b\n"), "the wheel brings the carrier under the needle; start from the rotation: after <seconds> rotate ts");
        // A move to an antenna goes to its point.
        assert!(has(&doc(&format!("{HEAD}%% move C1 to antenna2\n")).unwrap(), "step", &[("action", "move"), ("to", "sampling")]));
        fails(&format!("{HEAD}%% move C1 to antenn2\n"), "'antenn2' is neither an antenna nor a point of this scene (antennas: antenna1, antenna2");
        fails(&format!("{HEAD}%% divert an C1\n"), "'an' is not an antenna of this scene");
        let d = doc(&format!("{HEAD}%% pipette antenna2 duration=4 sid=SID-1\n%% pipette antenna2 sid=SID-2\n")).unwrap();
        assert!(has(&d, "step", &[("action", "pipette"), ("sample_id", "SID-1"), ("duration", "4")]) && has(&d, "step", &[("sample_id", "SID-2")]));
        fails(&format!("{HEAD}%% rotate sp sid=SID-1\n"), "only pipette does");
        fails("%% geometry spur\nsequenceDiagram\n%% carrier C1 SID-1\n", "carrier id=<id> [sid=<sample id>]");
        // Without sid= the carrier is empty.
        let d = doc("%% geometry spur\nsequenceDiagram\n%% carrier id=E\n%% move E to nsd\n").unwrap();
        assert!(has(&d, "carrier", &[("id", "E"), ("tube", "false")]) && !d.arrays["carrier"][0].entries.contains_key("sample_id"));
    }

    #[test]
    fn the_way_of_a_carrier_comes_from_its_steps() {
        let way = |body: &str| {
            let d = doc(&format!("%% geometry sampling\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n%% carrier id=C2 sid=SID-2 start=antenna1 ghost\n{body}%% caption a | b\n")).unwrap();
            build(&d).0.carriers.iter().map(|c| c.route.join(",")).collect::<Vec<_>>()
        };
        // A move: the shortest way along the lanes; pass: on to the next stop or the end.
        assert_eq!(way("%% move C1 to antenna2\n%% pass antenna2 C1\n")[0], "main_in,nsd,pit_in,sampling,pit_out,merge,main_out,gone");
        assert_eq!(way("%% move C1 to main_out\n")[0], "main_in,nsd,merge,main_out");
        // start=: where it is at the beginning (an antenna stands for its point).
        assert_eq!(way("")[1], "nsd");
        let e = |body: &str| Scene::from_doc(&doc(&format!("%% geometry sampling\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n{body}")).unwrap()).err().unwrap();
        let msg = e("%% pass antenna2 C1\n");
        assert!(msg.contains("pass antenna2 C1: 'C1' is not on antenna2: it is still where it starts, on 'main_in'; send it there first (move C1 to antenna2"), "{msg}");
        let msg = e("%% move C1 to main_out\n%% move C1 to antenna1\n");
        assert!(msg.contains("move C1 to antenna1: 'C1' can't go there along the lanes from 'main_out', where it is"), "{msg}");
        // The first wrong step in the file, with who is on the antenna instead.
        let msg = e("%% carrier id=C2 sid=SID-2\n%% move C1 to antenna2\n%% move C2 to main_out\n%% pass antenna2 C2\n");
        assert!(msg.contains("pass antenna2 C2: 'C2' is not on antenna2: after line 6 (move C2 to 'main_out') it goes to 'main_out'; on antenna2 there is C1: did you mean pass antenna2 C1?"), "{msg}");
    }

    #[test]
    fn the_belt_speed_is_in_the_video_settings() {
        let speed = |extra: &str| {
            let d = doc(&format!("%% geometry sampling\n{extra}sequenceDiagram\n%% carrier id=C1 sid=SID-1\n%% caption a | b\n")).unwrap();
            build(&d).0.carriers[0].speed
        };
        // 11 px/cm on screen.
        assert!((speed("%% @video belt=17.5\n") - 192.5).abs() < 1e-9);
        assert!((speed("%% @video belt=10\n") - 110.0).abs() < 1e-9);
        let d = doc("%% geometry sampling\n%% @video belt=0\nsequenceDiagram\n%% caption a | b\n").unwrap();
        assert!(Scene::from_doc(&d).err().unwrap().contains("belt must be a speed"));
    }

    #[test]
    fn import_errors() {
        fails("graph TD\n", "must start with");
        fails(&format!("{HEAD}%% mvoe C1 to nsd\n"), "unknown directive 'mvoe'");
        fails(&format!("{HEAD}%% move C1 nsd\n"), "'move' is written");
        fails(&format!("{HEAD}alt ok\nIF->>AN: A\nend\n"), "'alt' is not supported");
        fails(&format!("{HEAD}IF->>AN: Prepare SampleId=SID-9\n"), "no carrier has sample id 'SID-9'");
        fails(&format!("{HEAD}%% carrier id=C2 sid=SID-2 route=main_in,pit,gone\n"), "a carrier has no route= or needle=");
        fails(&format!("{HEAD}%% on antenna1\n%% at 3\nIF->>AN: A SampleId=SID-1\n"), "only one of");
        fails(&format!("{HEAD}%% at soon\nIF->>AN: A\n"), "needs seconds");
        fails(&format!("{HEAD}%% when C1 at nsd\nIF->>AN: A\n"), "'when' no longer exists");
        fails("%% geometry nowhere\n", "nowhere.mmd not found");
        fails("%% @point id=a id=b\n", "duplicate key 'id'");
    }

    #[test]
    fn a_plain_comment_and_mermaid_init_are_skipped() {
        assert!(doc(&format!("{HEAD}%% # just a note\n%%{{init: {{}}}}%%\nIF->>AN: Hello\n")).is_ok());
    }

    #[test]
    fn at_lines_become_tables_with_their_place() {
        let d = parse_text("%% @video title=t\n%% @point id=a x=1 y=2\n%% @video tail=3\n").unwrap();
        assert!(has(&d, "point", &[("id", "a"), ("x", "1")]));
        assert_eq!(d.arrays["point"][0].line, 2);
        assert_eq!(d.tables["video"].num_req("tail").unwrap(), 3.0);
        assert_eq!(d.tables["video"].str_req("subtitle").unwrap(), "test", "the name is the default subtitle");
    }

    fn temp(tag: &str, list: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("trackviz_flow_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (name, text) in list {
            let p = dir.join(name);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        dir
    }

    #[test]
    fn includes_replace_the_line_with_their_directives() {
        let dir = temp(
            "inc",
            &[
                ("a.mmd", "%% include common/b.mmd\nsequenceDiagram\nIF->>AN: Prepare SampleId=SID-1\n"),
                ("common/b.mmd", "%% @point id=p x=1 y=1\n%% include c.mmd\n\n%% caption T | x\n"),
                ("common/c.mmd", "%% carrier id=C1 sid=SID-1 start=p\n"),
                ("common/messages.mmd", "%% @message id=prepare from=IF to=AN text=Prepare params=\"SampleId={SampleId}\"\n"),
            ],
        );
        let d = load(&dir.join("a.mmd"), &dir).unwrap();
        assert!(has(&d, "carrier", &[("id", "C1")]) && has(&d, "step", &[("type", "prepare")]) && has(&d, "step", &[("title", "T")]));
        assert_eq!(d.files.len(), 4, "{:?}", d.files);
        // Every table knows the file and line it comes from.
        let c1 = &d.arrays["carrier"][0];
        assert!(c1.file.ends_with("c.mmd") && c1.line == 1, "{} {}", c1.file, c1.line);
        let prepare = d.arrays["step"].iter().find(|t| t.entries.contains_key("type")).unwrap();
        assert!(prepare.file.ends_with("a.mmd") && prepare.line == 3);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_scene_file_has_no_step_tables() {
        let dir = temp("step", &[("a.mmd", "sequenceDiagram\n%% @step action=wait duration=1\n")]);
        let e = load(&dir.join("a.mmd"), &dir).err().unwrap();
        assert!(e.contains("'@step' is not written in a scene") && e.contains("line 2"), "{e}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn include_errors() {
        let dir = temp(
            "err",
            &[
                ("msg.mmd", "%% include m.mmd\n"),
                ("m.mmd", "IF->>AN: Hello\n"),
                ("loop.mmd", "%% include loop.mmd\n"),
                ("no.mmd", "%% include missing.mmd\n"),
            ],
        );
        let e = |f: &str| read_lines(&dir.join(f), &mut Vec::new()).err().unwrap();
        assert!(e("msg.mmd").contains("can only hold %% directives"), "{}", e("msg.mmd"));
        assert!(e("msg.mmd").contains("line 1"));
        assert!(e("loop.mmd").contains("circular include"));
        assert!(e("no.mmd").contains("missing.mmd"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
