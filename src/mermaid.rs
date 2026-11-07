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
const DIRECTIVES: [&str; 27] = [
    "title", "subtitle", "geometry", "carrier", "include", "on", "after", "color", "when", "at", "delay", "with", "after_end", "after_start", "for", "duration", "move", "pipette",
    "rotate", "highlight", "operator", "needle", "status", "show", "hide", "wait", "caption",
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

/// The default way through a geometry. Spur2 depends on the needles of the sample.
fn default_route(geometry: &str, needle: &str) -> Option<Vec<&'static str>> {
    let head = ["pit_in", "w1c", "w1b", "spur_in", "w1", "d1"];
    let n1 = ["q2", "pc1", "q1", "s1", "pt1u", "rj1", "w2", "d2"];
    let n2 = ["qb2", "pc2", "qb1", "s2", "pt2u", "rj2"];
    let tail = ["top_r", "top_l", "back", "po", "merge", "iface_out", "main_out", "gone"];
    Some(match (geometry, needle) {
        ("sampling", _) => vec!["main_in", "nsd", "pit_in", "sampling", "pit_out", "merge", "main_out"],
        ("turnstile", _) => vec!["main_in", "nsd", "pit_in", "pit_wait", "ts.in", "ts.out", "pit_out", "merge", "main_out", "gone"],
        ("spur", _) => vec![
            "main_in", "nsd", "pit_in", "spur_in", "queue2", "queue", "sampling", "spur_top_in", "spur_top_out", "spur_out", "pit_out", "merge", "iface_out", "main_out", "gone",
        ],
        ("spur2", "1") => ["main_in", "nsd"].iter().chain(&head).chain(&n1).chain(&["rj2"]).chain(&tail).copied().collect(),
        ("spur2", "2") => ["main_in", "nsd"].iter().chain(&head).chain(&["w2", "d2"]).chain(&n2).chain(&tail).copied().collect(),
        ("spur2", "both") => ["main_in", "nsd"].iter().chain(&head).chain(&n1).chain(&n2).chain(&tail).copied().collect(),
        _ => return None,
    })
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
    /// (carrier id, sample id)
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
    /// The messages so far, for `%% after <seconds> <message> <carrier>`:
    /// (text, carrier, event name).
    sent: Vec<(String, Option<String>, String)>,
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
            sent: Vec::new(),
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
        all.append(&mut self.timing);
        all.extend(keys);
        self.add("step", all);
        self.last_pipette = action == "pipette";
        Ok(())
    }

    /// A step has one starting rule: only `delay` is added to another.
    fn no_start_yet(&self) -> Result<(), String> {
        if self.on.is_some() || self.timing.iter().any(|(k, _)| ["when", "at", "start", "after_start", "after_end"].contains(k)) {
            return Err("only one of on, after, at, when, with, after_start, after_end for a step".into());
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
                need(2, "carrier <id> <sample id> [needle=1|2|both] [route=a,b,c] [color=#hex] [speed=n] [ghost] [hidden]")?;
                self.carrier(&plain, &o)?;
            }
            "on" => {
                need(1, "on <antenna> [<carrier>]")?;
                self.on_antenna(plain[0], plain.get(1).copied())?;
            }
            "after" => {
                need(1, "after <seconds> [<message> <carrier>]")?;
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
            "when" => self.timing_key("when", text(tail))?,
            "with" => self.timing_key("start", text("with"))?,
            "at" | "delay" | "after_end" | "after_start" => {
                need(1, &format!("{cmd} <seconds>"))?;
                let key = match cmd {
                    "at" => "at",
                    "delay" => "delay",
                    "after_end" => "after_end",
                    _ => "after_start",
                };
                self.timing_key(key, seconds(cmd, plain[0])?)?;
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
                    return Err("'move' is written: move <carrier> to <point> [ease=in|out|inout|linear] [speed=n] [duration=s] [runaway]".into());
                }
                let mut k: Keys = vec![("carrier", text(plain[0])), ("to", text(plain[2]))];
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
                let mut k: Keys = vec![("device", text(plain[0]))];
                k.extend(duration(&o)?);
                self.step(cmd, k)?;
            }
            "needle" => {
                need(2, "needle <device> down|up [duration=s]")?;
                let mut k: Keys = vec![("device", text(plain[0])), ("to", text(plain[1]))];
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
    fn on_antenna(&mut self, antenna: &str, carrier: Option<&str>) -> Result<(), String> {
        let field = |t: &Table, k: &str| t.str_opt(k).ok().flatten().unwrap_or_default();
        let antennas: Vec<(String, String)> = self.list("device").iter().filter(|t| field(t, "kind") == "antenna").map(|t| (field(t, "id"), field(t, "at"))).collect();
        let point = antennas.iter().find(|a| a.0 == antenna).map(|a| a.1.clone()).ok_or_else(|| {
            let names: Vec<&str> = antennas.iter().map(|a| a.0.as_str()).collect();
            format!("'{antenna}' is not an antenna of this scene (antennas: {})", if names.is_empty() { "none".to_string() } else { names.join(", ") })
        })?;
        if let Some(c) = carrier {
            if !self.samples.iter().any(|s| s.0 == c) {
                return Err(format!("'on {antenna} {c}': no carrier '{c}'"));
            }
        }
        self.no_start_yet()?;
        self.on = Some((point, antenna.to_string(), carrier.map(String::from)));
        Ok(())
    }

    /// `%% after <seconds>`: after the end of the step before; `%% after <seconds>
    /// <message> [<carrier>]`: after that message has arrived (the latest with
    /// that text, for that carrier).
    fn after(&mut self, secs: Value, refer: &[&str]) -> Result<(), String> {
        if refer.is_empty() {
            return self.timing_key("after_end", secs);
        }
        let mut name = refer.to_vec();
        let carrier = name.last().filter(|w| self.samples.iter().any(|s| s.0 == **w)).map(|w| w.to_string());
        if carrier.is_some() {
            name.pop();
        }
        let name = name.join(" ");
        let (_, c, event) = self
            .sent
            .iter()
            .rev()
            .find(|m| m.0.eq_ignore_ascii_case(&name) && (carrier.is_none() || m.1 == carrier))
            .ok_or_else(|| format!("'after': no message '{name}'{} before this line", carrier.as_ref().map_or(String::new(), |c| format!(" for {c}"))))?;
        let when = c.as_ref().map_or_else(|| format!("{event} arrived"), |c| format!("{event} {c} arrived"));
        self.timing_key("when", text(&when))?;
        self.timing_key("delay", secs)
    }

    fn carrier(&mut self, plain: &[&str], o: &HashMap<&str, &str>) -> Result<(), String> {
        let (id, sample) = (plain[0], plain[1]);
        let route: Vec<String> = match o.get("route") {
            Some(r) => r.split(',').map(String::from).collect(),
            None => {
                let geometry = self.geometry.clone().ok_or("the geometry must be given before a carrier without route=")?;
                let needle = o.get("needle").copied().unwrap_or("1");
                let mut r: Vec<String> = default_route(&geometry, needle)
                    .ok_or_else(|| format!("no default route for geometry '{geometry}' with needle={needle}: give route=a,b,c"))?
                    .into_iter()
                    .map(String::from)
                    .collect();
                // Only the first carrier starts on the panel.
                if !self.samples.is_empty() && geometry != "sampling" {
                    r.insert(0, "spawn".to_string());
                }
                r
            }
        };
        let speed = match o.get("speed") {
            Some(s) => seconds("speed", s)?,
            None => Value::Num(if self.geometry.as_deref().is_some_and(|g| g.starts_with("spur")) { 110.0 } else { 75.0 }),
        };
        let mut k: Keys = vec![
            ("id", text(id)),
            ("tube", Value::Bool(true)),
            ("sample_id", text(sample)),
            ("route", Value::Arr(route.iter().map(|p| text(p)).collect())),
            ("speed", speed),
        ];
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
        self.samples.push((id.to_string(), sample.to_string()));
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
        self.sent.push((body, carrier, event));
        self.step("message", k)
    }

    fn note(&mut self, line: &str) -> Result<(), String> {
        let (place, body) = line.split_once(':').ok_or("a note needs ': text'")?;
        let who = place.split_whitespace().last().unwrap_or_default().split(',').next().unwrap_or_default().to_string();
        self.participant(&who, &who);
        // A pipette already adds this note.
        if self.last_pipette && body.trim().starts_with("Pipetting") {
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

    const EX: &str = "%% geometry spur\nsequenceDiagram\n%% carrier C1 SID-1\nIF->>AN: Prepare SampleId=SID-1\nAN-->>IF: OK SampleId=SID-1\nAN->>DMX: Query SampleId=SID-1\nDMX-->>AN: Tests to perform SampleId=SID-1\nIF->>AN: Sample Acquisition SampleId=SID-1\nAN-->>IF: OK\n%% pipette sp\nNote over AN: Pipetting\nAN->>IF: Sample Complete SampleId=SID-1\nIF-->>AN: OK\n";

    #[test]
    fn a_diagram_builds_a_valid_scene_with_the_messages_written() {
        let (sc, tl) = build(&doc(EX).unwrap());
        assert!(crate::lint::warnings(&sc, &tl).is_empty(), "{:?}", crate::lint::warnings(&sc, &tl));
        assert_eq!(arrows(&sc, &tl), lines(EX));
    }

    const HEAD: &str = "%% geometry spur\nsequenceDiagram\n%% carrier C1 SID-1\n";

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
        let d = doc("%% geometry spur\nsequenceDiagram\nparticipant I as Interface\nparticipant L as Lab\n%% carrier C1 SID-1\nI->>AN: Prepare SampleId=SID-1\nI->>L: Hello\n").unwrap();
        assert!(has(&d, "step", &[("type", "prepare")]));
        assert!(has(&d, "actor", &[("id", "L"), ("label", "Lab")]));
        assert!(has(&d, "step", &[("from", "IF"), ("to", "L"), ("text", "Hello")]));
    }

    #[test]
    fn timing_directives_apply_to_the_next_step() {
        let d = doc(&format!("{HEAD}%% move C1 to nsd ease=in\n%% when C1 at nsd\n%% delay 0.5\nIF->>AN: Prepare SampleId=SID-1\n%% with\n%% highlight nsd duration=2\n")).unwrap();
        assert!(has(&d, "step", &[("when", "C1 at nsd"), ("delay", "0.5"), ("type", "prepare")]));
        assert!(has(&d, "step", &[("start", "with"), ("device", "nsd"), ("duration", "2")]));
        assert!(has(&d, "step", &[("action", "move"), ("ease", "in")]) && !has(&d, "step", &[("action", "move"), ("when", "C1 at nsd")]));
    }

    #[test]
    fn for_duration_and_runaway() {
        let d = doc(&format!("{HEAD}%% move C1 to nsd runaway\n%% for C1\nAN-->>IF: OK\n%% duration 6\nNote over AN: Busy\n")).unwrap();
        assert!(has(&d, "step", &[("action", "move"), ("runaway", "true")]));
        assert!(has(&d, "step", &[("type", "analyzer_ok"), ("carrier", "C1")]));
        assert!(has(&d, "step", &[("action", "note"), ("duration", "6"), ("actor", "AN")]));
    }

    const ON: &str = "%% geometry spur2\nsequenceDiagram\n%% carrier C1 SID-1 needle=both\n%% carrier C2 SID-2 needle=2\n\
        %% move C1 to d1\n%% move C1 to s1\n%% with\n%% move C2 to w1\n\
        %% on antenna1\nIF->>AN: Add Tube SampleId=SID-1\nAN-->>IF: Fine\nAN->>DMX: Which tests? SampleId=SID-1\nDMX-->>AN: These\n\
        %% on antenna1 C2\n%% caption Second | The next one.\n%% on antenna1\nIF->>AN: Add Tube SampleId=SID-2\n%% after 0.2 Add Tube C1\nAN-->>IF: Late\n\
        %% color #9333EA\n%% on antenna2\nIF->>AN: Again SampleId=SID-1\n%% after 1\nIF->>AN: Go SampleId=SID-1\n";

    #[test]
    fn on_after_and_color_set_the_moment_and_the_look() {
        let d = doc(ON).unwrap();
        // The text as written, at the antenna of its carrier.
        assert!(has(&d, "step", &[("text", "Add Tube"), ("params", "SampleId={SampleId}"), ("carrier", "C1"), ("when", "C1 at nsd")]));
        assert!(has(&d, "step", &[("action", "caption"), ("when", "C2 at nsd")]) && has(&d, "step", &[("carrier", "C2"), ("when", "C2 at nsd")]));
        assert!(has(&d, "step", &[("text", "Again"), ("when", "C1 at d1"), ("color", "#9333EA")]));
        assert!(has(&d, "step", &[("text", "Late"), ("when", "add_tube C1 arrived"), ("delay", "0.2")]));
        assert!(has(&d, "step", &[("text", "Go"), ("after_end", "1")]));
        let (_, tl) = build(&d);
        let at = |name: &str| tl.events.iter().find(|e| matches!(&e.ev, Ev::Message { text, .. } if text == name)).unwrap().t0;
        let same = |a: f64, b: f64| assert!((a - b).abs() < 0.011, "{a} != {b}");
        // Without a directive a message starts when the one before has arrived.
        same(at("Fine"), at("Add Tube") + 1.5);
        same(at("Late"), at("Add Tube") + 1.5 + 0.2);
        same(at("Go"), at("Again") + 1.5 + 1.0);
    }

    const ON_HEAD: &str = "%% geometry spur2\nsequenceDiagram\n%% carrier C1 SID-1\n";

    #[test]
    fn on_after_and_color_errors() {
        let k = |body: &str| format!("{ON_HEAD}{body}");
        fails(&k("%% on antena1\nIF->>AN: X SampleId=SID-1\n"), "not an antenna of this scene (antennas: antenna1, antenna2");
        fails(&k("%% on antenna1 C9\n%% caption a | b\n"), "no carrier 'C9'");
        fails(&k("%% on antenna1\n%% caption a | b\n"), "write the carrier, on antenna1 <carrier>");
        fails(&k("%% on antenna1\n%% at 3\nIF->>AN: X SampleId=SID-1\n"), "only one of");
        fails(&k("%% when C1 at d1\n%% on antenna1\nIF->>AN: X SampleId=SID-1\n"), "only one of");
        fails(&k("%% on antenna1\n"), "its step is missing");
        fails(&k("%% after 1 Nothing C1\nIF->>AN: X\n"), "no message 'Nothing' for C1");
        fails(&k("%% color blue\nIF->>AN: X\n"), "like #9333EA");
        fails(&k("%% color #9333EA\n%% caption a | b\n"), "applies to the next message");
        fails(&k("%% color #9333EA\n"), "its message is missing");
        fails(&k("%% Prepare 1\nIF->>AN: X SampleId=SID-1\n"), "unknown directive 'Prepare'");
        fails(&k("%% onn antenna1\nIF->>AN: X SampleId=SID-1\n"), "did you mean 'on'");
    }

    #[test]
    fn a_note_after_a_pipette_is_not_repeated() {
        let d = doc(&format!("{HEAD}%% pipette sp\nNote over AN: Pipetting\nNote over AN: Other\n")).unwrap();
        assert_eq!(d.arrays["step"].iter().filter(|t| t.str_req("action").unwrap() == "note").count(), 1);
    }

    #[test]
    fn every_default_route_exists() {
        for (g, needle) in [("sampling", "1"), ("turnstile", "1"), ("spur", "1"), ("spur2", "1"), ("spur2", "2"), ("spur2", "both")] {
            let src = format!("%% geometry {g}\nsequenceDiagram\n%% carrier C1 SID-1 needle={needle}\n%% carrier C2 SID-2 needle={needle}\n%% move C1 to merge\n");
            let d = doc(&src).unwrap_or_else(|e| panic!("{g}: {e}"));
            if g != "turnstile" {
                build(&d); // a move through the wheel needs rotations
            }
        }
    }

    #[test]
    fn import_errors() {
        fails("graph TD\n", "must start with");
        fails(&format!("{HEAD}%% mvoe C1 to nsd\n"), "unknown directive 'mvoe'");
        fails(&format!("{HEAD}%% move C1 nsd\n"), "'move' is written");
        fails(&format!("{HEAD}alt ok\nIF->>AN: A\nend\n"), "'alt' is not supported");
        fails(&format!("{HEAD}IF->>AN: Prepare SampleId=SID-9\n"), "no carrier has sample id 'SID-9'");
        fails("sequenceDiagram\n%% carrier C1 SID-1\n", "geometry must be given before");
        fails(&format!("{HEAD}%% when C1 at nsd\n%% at 3\nIF->>AN: A\n"), "only one of");
        fails(&format!("{HEAD}%% delay soon\nIF->>AN: A\n"), "needs seconds");
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
                ("common/c.mmd", "%% carrier C1 SID-1 route=p,p\n"),
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
