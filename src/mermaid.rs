//! Mermaid sequence diagrams: `export` writes the messages of a scene, `import`
//! builds a scene from a diagram whose `%%` comments describe the physical side.

use crate::scene::{Ev, Scene, Timeline};
use crate::toml::Table;
use std::collections::HashMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------- export

/// A hex color as Mermaid wants it in `rect`, faded to stay readable.
fn rect_color(hex: &str) -> String {
    let c = |i: usize| u8::from_str_radix(hex.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    format!("rgba({}, {}, {}, 0.15)", c(1), c(3), c(5))
}

/// The sequence diagram of a scene, in row order.
pub fn export(sc: &Scene, tl: &Timeline) -> String {
    let mut o = String::from("sequenceDiagram\n");
    for a in &sc.actors {
        let _ = writeln!(o, "    participant {} as {}", a.id, a.label);
    }
    let clean = |s: &str| s.replace(';', ",");
    let mut rows: Vec<_> = tl
        .events
        .iter()
        .filter_map(|e| match &e.ev {
            Ev::Message { row, .. } | Ev::Note { row, .. } => Some((*row, &e.ev)),
            _ => None,
        })
        .collect();
    rows.sort_by_key(|r| r.0);
    for (_, ev) in rows {
        match ev {
            Ev::Message { from, to, text, params, reply, color, .. } => {
                let arrow = if *reply { "-->>" } else { "->>" };
                let label = if params.is_empty() { text.clone() } else { format!("{text} {params}") };
                let line = format!("{}{arrow}{}: {}", sc.actors[*from].id, sc.actors[*to].id, clean(&label));
                match color {
                    Some(c) => {
                        let _ = writeln!(o, "    rect {}\n        {line}\n    end", rect_color(c));
                    }
                    None => {
                        let _ = writeln!(o, "    {line}");
                    }
                }
            }
            Ev::Note { actor, text, .. } => {
                let _ = writeln!(o, "    Note over {}: {}", sc.actors[*actor].id, clean(text));
            }
            _ => {}
        }
    }
    o
}

// ---------------------------------------------------------------- import

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

/// Replaces a line of the generated scene in an error with the diagram line it
/// comes from (each block of the scene starts with a `# from` comment).
pub fn locate(text: &str, err: &str) -> String {
    let Some(at) = err.find("at line ") else { return err.to_string() };
    let digits: String = err[at + 8..].chars().take_while(char::is_ascii_digit).collect();
    let Ok(n) = digits.parse::<usize>() else { return err.to_string() };
    let origin = text.lines().take(n).filter_map(|l| l.strip_prefix("# from ")).last();
    origin.map_or_else(|| err.to_string(), |o| err.replacen(&format!("at line {n}"), &format!("at {o}"), 1))
}

/// A message of the shared library.
pub struct MsgType {
    pub id: String,
    pub from: String,
    pub to: String,
    pub text: String,
    pub params: String,
    pub reply: bool,
    pub colored: bool,
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
            colored: t.str_opt("color")?.is_some(),
        })
    }
}

/// What the shared files offer: (actor id, label) and the named messages.
pub struct Library {
    pub actors: Vec<(String, String)>,
    pub messages: Vec<MsgType>,
}

/// What the keyword directives need to know of a geometry.
#[derive(Default)]
pub struct Geometry {
    /// Points where a carrier is diverted or read, in file order: NSD, antenna, diverts.
    pub diverts: Vec<String>,
    /// Sampling devices in file order: (device id, point; None for a turnstile).
    pub samplers: Vec<(String, Option<String>)>,
    /// Where a carrier has left the interface module.
    pub exit: String,
}

impl Geometry {
    pub fn from_doc(doc: &crate::toml::Doc) -> Result<Geometry, String> {
        let mut g = Geometry::default();
        let mut points = Vec::new();
        for t in doc.arrays.get("point").map(|v| v.as_slice()).unwrap_or(&[]) {
            points.push(t.str_req("id")?);
        }
        for t in doc.arrays.get("device").map(|v| v.as_slice()).unwrap_or(&[]) {
            match t.str_req("kind")?.as_str() {
                "nsd" | "antenna" | "divert" => g.diverts.push(t.str_req("at")?),
                "sampling" => g.samplers.push((t.str_req("id")?, Some(t.str_req("at")?))),
                "turnstile" => g.samplers.push((t.str_req("id")?, None)),
                _ => {}
            }
        }
        g.exit = if points.iter().any(|p| p == "iface_out") { "iface_out" } else { "main_out" }.to_string();
        Ok(g)
    }
}

/// Keyword directives: the next message is sent at a moment the program knows.
const KEYWORDS: [&str; 8] = ["Prepare", "OK", "Query", "Tests", "Sample Acquisition", "Sample Complete", "Release", "Results"];

/// Directive names, for suggestions.
const DIRECTIVES: [&str; 24] = [
    "title", "subtitle", "geometry", "carrier", "include", "when", "at", "delay", "with", "after_end", "after_start", "for", "duration", "move", "pipette", "rotate", "highlight",
    "operator", "needle", "status", "show", "hide", "wait", "caption",
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

/// The closest keyword or directive to a mistyped one.
fn suggest(d: &str) -> Option<&'static str> {
    let names = KEYWORDS.iter().chain(DIRECTIVES.iter());
    names
        .map(|&n| {
            let words = n.split(' ').count();
            let head: Vec<&str> = d.split_whitespace().take(words).collect();
            (distance(&head.join(" "), n), n)
        })
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

const GEOMETRIES: &str = "sampling, turnstile, spur, spur2";

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
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

#[derive(Default)]
struct Importer {
    title: String,
    subtitle: String,
    geometry: Option<String>,
    /// Mermaid id -> actor id.
    actors: Vec<(String, String, String)>,
    carriers: Vec<String>,
    /// (carrier id, sample id)
    samples: Vec<(String, String)>,
    prepared: Vec<String>,
    steps: Vec<String>,
    /// Timing keys for the next step.
    timing: String,
    last_pipette: bool,
    /// Where the line being read comes from.
    place: String,
    /// `%% for`: the carrier of the next message, when its text doesn't say it.
    next_carrier: Option<String>,
    /// A keyword directive waiting for its message: (keyword, number).
    keyword: Option<(&'static str, Option<usize>)>,
    geo: Geometry,
    /// The carrier of the latest message, and its event name.
    last_message: Option<(Option<String>, String)>,
    /// The sampling device each carrier was last at.
    sampled_at: HashMap<String, String>,
    queried: bool,
    /// The event name of the latest OK to each carrier.
    ok_of: HashMap<String, String>,
}

impl Importer {
    fn step(&mut self, action: &str, keys: &str) {
        let t = std::mem::take(&mut self.timing);
        self.steps.push(format!("# from {}\n[[step]]\naction = \"{action}\"\n{t}{keys}\n", self.place));
        self.last_pipette = action == "pipette";
    }

    fn timing_key(&mut self, line: String) -> Result<(), String> {
        let chosen = ["when =", "at =", "start =", "after_start =", "after_end ="].iter().any(|k| self.timing.contains(k));
        if chosen && !line.starts_with("delay") {
            return Err("only one of when, at, with, after_start, after_end for a step".into());
        }
        self.timing.push_str(&line);
        Ok(())
    }

    fn participant(&mut self, id: &str, label: &str, lib: &Library) {
        if self.actors.iter().any(|a| a.0 == id) {
            return;
        }
        let known = lib.actors.iter().find(|a| a.0 == id || a.1 == label).map(|a| a.0.clone());
        self.actors.push((id.to_string(), known.clone().unwrap_or_else(|| id.to_string()), if known.is_some() { String::new() } else { label.to_string() }));
    }

    fn actor(&self, id: &str) -> String {
        self.actors.iter().find(|a| a.0 == id).map_or_else(|| id.to_string(), |a| a.1.clone())
    }

    fn duration(o: &HashMap<&str, &str>) -> String {
        o.get("duration").map_or(String::new(), |d| format!("duration = {d}\n"))
    }

    fn directive(&mut self, d: &str) -> Result<(), String> {
        if self.keyword.is_some() {
            return Err("a keyword directive must be followed by its message".into());
        }
        if let Some(kw) = KEYWORDS.iter().find(|k| d == **k || d.starts_with(&format!("{k} "))) {
            let rest = d[kw.len()..].trim();
            let n = if rest.is_empty() {
                None
            } else {
                Some(rest.parse::<usize>().ok().filter(|&n| n > 0).ok_or_else(|| format!("'{kw}' takes a number from 1, not '{rest}'"))?)
            };
            match *kw {
                "Prepare" if n.is_none() => return Err("'Prepare' needs the number of the divert point: Prepare 1 is the first".into()),
                "Prepare" | "Sample Acquisition" => {}
                _ if n.is_some() => return Err(format!("'{kw}' takes no number")),
                _ => {}
            }
            if !self.timing.is_empty() {
                return Err(format!("'{kw}' sets the moment itself: no when, at, with or delay before it"));
            }
            self.keyword = Some((kw, n));
            return Ok(());
        }
        let words: Vec<&str> = d.split_whitespace().collect();
        let Some((&cmd, rest)) = words.split_first() else { return Ok(()) };
        let tail = d[cmd.len()..].trim();
        let (plain, o) = options(rest);
        let need = |n: usize, usage: &str| if plain.len() < n { Err(format!("'{cmd}' is written: {usage}")) } else { Ok(()) };
        match cmd {
            "#" => {}
            "title" => self.title = tail.to_string(),
            "subtitle" => self.subtitle = tail.to_string(),
            "geometry" => {
                need(1, "geometry <name>")?;
                self.geometry = Some(plain[0].to_string());
            }
            "carrier" => {
                need(2, "carrier <id> <sample id> [needle=1|2|both] [route=a,b,c] [color=#hex] [speed=n] [ghost] [hidden]")?;
                self.carrier(&plain, &o)?;
            }
            "when" | "at" | "delay" | "with" | "after_end" | "after_start" => {
                let line = match cmd {
                    "when" => format!("when = {}\n", quote(tail)),
                    "with" => "start = \"with\"\n".to_string(),
                    _ => {
                        need(1, &format!("{cmd} <seconds>"))?;
                        plain[0].parse::<f64>().map_err(|_| format!("'{cmd}' needs seconds, not '{}'", plain[0]))?;
                        format!("{cmd} = {}\n", plain[0])
                    }
                };
                self.timing_key(line)?;
            }
            "for" => {
                need(1, "for <carrier>")?;
                self.next_carrier = Some(plain[0].to_string());
            }
            "duration" => {
                need(1, "duration <seconds>")?;
                plain[0].parse::<f64>().map_err(|_| format!("'duration' needs seconds, not '{}'", plain[0]))?;
                self.timing.push_str(&format!("duration = {}\n", plain[0]));
            }
            "move" => {
                if plain.len() < 3 || plain[1] != "to" {
                    return Err("'move' is written: move <carrier> to <point> [ease=in|out|inout|linear] [speed=n] [duration=s] [runaway]".into());
                }
                let mut k = format!("carrier = {}\nto = {}\n", quote(plain[0]), quote(plain[2]));
                if let Some(e) = o.get("ease") {
                    let _ = writeln!(k, "ease = {}", quote(e));
                }
                if let Some(v) = o.get("speed") {
                    let _ = writeln!(k, "speed = {v}");
                }
                k.push_str(&Self::duration(&o));
                if plain.contains(&"runaway") {
                    k.push_str("runaway = true\n");
                }
                self.step("move", &k);
            }
            "pipette" | "rotate" | "highlight" | "operator" => {
                need(1, &format!("{cmd} <device> [duration=s]"))?;
                let k = format!("device = {}\n{}", quote(plain[0]), Self::duration(&o));
                self.step(cmd, &k);
            }
            "needle" => {
                need(2, "needle <device> down|up [duration=s]")?;
                let k = format!("device = {}\nto = {}\n{}", quote(plain[0]), quote(plain[1]), Self::duration(&o));
                self.step("needle", &k);
            }
            "status" => {
                need(2, "status <device> error|ready|normal")?;
                let k = format!("device = {}\nstate = {}\n", quote(plain[0]), quote(plain[1]));
                self.step("status", &k);
            }
            "show" | "hide" => {
                need(1, &format!("{cmd} <carrier>"))?;
                let k = format!("carrier = {}\n{}", quote(plain[0]), Self::duration(&o));
                self.step(cmd, &k);
            }
            "wait" => {
                need(1, "wait <seconds>")?;
                let k = format!("duration = {}\n", plain[0]);
                self.step("wait", &k);
            }
            "caption" => {
                let (title, text) = tail.split_once('|').ok_or("'caption' is written: caption <title> | <text>")?;
                let k = format!("title = {}\ntext = {}\n", quote(title.trim()), quote(text.trim()));
                self.step("caption", &k);
            }
            o => {
                let hint = suggest(d).map_or(String::new(), |s| format!(": did you mean '{s}'?"));
                return Err(format!("unknown directive '{o}'{hint} (for a plain comment write '%% # text')"));
            }
        }
        Ok(())
    }

    fn carrier(&mut self, plain: &[&str], o: &HashMap<&str, &str>) -> Result<(), String> {
        let geometry = self.geometry.clone().ok_or("the geometry must be given before the carriers")?;
        let (id, sample) = (plain[0], plain[1]);
        let ghost = plain.contains(&"ghost");
        let hidden = plain.contains(&"hidden");
        let route: Vec<String> = match o.get("route") {
            Some(r) => r.split(',').map(String::from).collect(),
            None => {
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
        let list: Vec<String> = route.iter().map(|p| quote(p)).collect();
        let speed = o.get("speed").map_or_else(|| if geometry.starts_with("spur") { "110" } else { "75" }.to_string(), |s| s.to_string());
        let mut c = format!("# from {}\n[[carrier]]\nid = {}\ntube = true\nsample_id = {}\nroute = [{}]\nspeed = {speed}\n", self.place, quote(id), quote(sample), list.join(", "));
        if let Some(col) = o.get("color") {
            let _ = writeln!(c, "cap_color = {}", quote(col));
        }
        if ghost {
            c.push_str("ghost = true\n");
        }
        if hidden {
            c.push_str("visible = false\n");
        }
        if let Some(t) = o.get("tag") {
            let _ = writeln!(c, "tag = {}", quote(&t.replace('_', " ")));
        }
        self.carriers.push(c);
        self.samples.push((id.to_string(), sample.to_string()));
        Ok(())
    }

    fn message(&mut self, line: &str, lib: &Library) -> Result<(), String> {
        let (left, label) = line.split_once(':').ok_or("a message needs ': text'")?;
        let arrow = ["-->>", "->>", "--x", "-x", "--)", "-)", "-->", "->"].into_iter().find(|a| left.contains(a)).ok_or("no arrow in the message")?;
        let (a, b) = left.split_once(arrow).unwrap();
        let (a, b) = (a.trim(), b.trim());
        self.participant(a, a, lib);
        self.participant(b, b, lib);
        let (from, to) = (self.actor(a), self.actor(b));
        let reply = arrow.starts_with("--");
        // "Sample Complete SampleId=X Needle=1": the parameters start at the first key=value.
        let words: Vec<&str> = label.split_whitespace().collect();
        let split = words.iter().position(|w| w.contains('=')).unwrap_or(words.len());
        let (text, params) = (words[..split].join(" "), &words[split..]);
        if text.is_empty() {
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
        let mut shape: Vec<String> = Vec::new();
        for p in params {
            shape.push(match p.split_once('=') {
                Some(("SampleId", _)) => "SampleId={SampleId}".to_string(),
                Some(("Needle", _)) => "Needle={Needle}".to_string(),
                _ => p.to_string(),
            });
        }
        let shape = shape.join(" ");
        let again = text == "Prepare" && sample.is_some_and(|s| self.prepared.iter().any(|p| p == s));
        if text == "Prepare" {
            self.prepared.push(sample.unwrap_or_default().to_string());
        }
        if let Some((kw, n)) = self.keyword.take() {
            return self.keyword_message(kw, n, &from, &to, &text, &params.join(" "), reply, carrier);
        }
        let found = lib.messages.iter().find(|m| m.from == from && m.to == to && m.text == text && m.params == shape && m.reply == reply && m.colored == again);
        let mut k = String::new();
        match found {
            Some(m) => {
                let _ = writeln!(k, "type = {}", quote(&m.id));
            }
            None => {
                let _ = writeln!(k, "from = {}\nto = {}\ntext = {}", quote(&from), quote(&to), quote(&text));
                if !shape.is_empty() {
                    let _ = writeln!(k, "params = {}", quote(&shape));
                }
                if reply {
                    k.push_str("reply = true\n");
                }
            }
        }
        if let Some(c) = &carrier {
            let _ = writeln!(k, "carrier = {}", quote(c));
        }
        if let Some(n) = needle {
            let _ = writeln!(k, "needle = {n}");
        }
        let event = found.map_or_else(|| text.to_lowercase().replace(' ', "_"), |m| m.id.clone());
        if found.is_none() {
            let _ = writeln!(k, "event = {}", quote(&event));
        }
        self.remember(&text, needle.and_then(|n| n.parse().ok()), &carrier, &event);
        self.last_message = Some((carrier, event));
        self.step("message", &k);
        Ok(())
    }

    /// What the keyword directives after this message need to know. `role` is
    /// the keyword, or the text of a message written without one.
    fn remember(&mut self, role: &str, needle: Option<usize>, carrier: &Option<String>, event: &str) {
        match (role, carrier) {
            ("OK", Some(c)) => {
                self.ok_of.insert(c.clone(), event.to_string());
            }
            ("Query", _) => self.queried = true,
            ("Sample Acquisition", Some(c)) => {
                if let Some((dev, _)) = self.geo.samplers.get(needle.unwrap_or(1).saturating_sub(1)) {
                    self.sampled_at.insert(c.clone(), dev.clone());
                }
            }
            _ => {}
        }
    }

    /// A message after a keyword directive: its text as written, at the moment
    /// the keyword stands for.
    #[allow(clippy::too_many_arguments)]
    fn keyword_message(&mut self, kw: &str, n: Option<usize>, from: &str, to: &str, text: &str, params: &str, reply: bool, carrier: Option<String>) -> Result<(), String> {
        let carrier = carrier.or_else(|| self.last_message.as_ref().and_then(|m| m.0.clone()));
        let need_carrier = || carrier.clone().ok_or_else(|| format!("'{kw}': no SampleId in the message and no carrier in the message before"));
        let event = kw.to_lowercase().replace(' ', "_");
        let mut purple = false;
        let when = match kw {
            "Prepare" => {
                let n = n.unwrap();
                let point = self.geo.diverts.get(n - 1).ok_or_else(|| format!("'Prepare {n}': this geometry has {} divert point(s)", self.geo.diverts.len()))?;
                purple = n > 1;
                format!("when = {}\n", quote(&format!("{} at {point}", need_carrier()?)))
            }
            "OK" => {
                let (c, e) = self.last_message.clone().ok_or("'OK' answers a message: there is none before it")?;
                format!("when = {}\n", quote(&c.map_or_else(|| format!("{e} arrived"), |c| format!("{e} {c} arrived"))))
            }
            "Query" => {
                let c = need_carrier()?;
                let ok = self.ok_of.get(&c).ok_or_else(|| format!("'Query': no OK to carrier '{c}' before it"))?;
                let mut w = format!("{ok} {c} arrived");
                if self.queried {
                    w.push_str(" and tests arrived");
                }
                format!("when = {}\n", quote(&w))
            }
            "Tests" => format!("when = {}\n", quote(&format!("query {} arrived", need_carrier()?))),
            "Sample Acquisition" => {
                let count = self.geo.samplers.len();
                let n = match n {
                    Some(n) => n,
                    None if count == 1 => 1,
                    None => return Err(format!("'Sample Acquisition' needs the number of the sampling point: this geometry has {count}")),
                };
                let (dev, point) = self.geo.samplers.get(n - 1).cloned().ok_or_else(|| format!("'Sample Acquisition {n}': this geometry has {count} sampling point(s)"))?;
                let c = need_carrier()?;
                self.sampled_at.insert(c.clone(), dev.clone());
                // In a turnstile the carrier reaches the needle with a rotation.
                let w = point.map_or_else(|| format!("rotate {dev} done"), |p| format!("{c} at {p}"));
                format!("when = {}\n", quote(&w))
            }
            "Sample Complete" => {
                let c = need_carrier()?;
                let dev = self.sampled_at.get(&c).ok_or_else(|| format!("'Sample Complete': no Sample Acquisition for carrier '{c}' before it"))?;
                format!("when = {}\n", quote(&format!("pipette {dev} done")))
            }
            "Release" => format!("when = {}\n", quote(&format!("{} at {}", need_carrier()?, self.geo.exit))),
            _ => "start = \"after_all\"\n".to_string(), // Results: at the end of the scene
        };
        self.timing = when;
        let mut k = format!("from = {}\nto = {}\ntext = {}\n", quote(from), quote(to), quote(text));
        if !params.is_empty() {
            let _ = writeln!(k, "params = {}", quote(params));
        }
        if reply {
            k.push_str("reply = true\n");
        }
        if purple {
            k.push_str("color = \"#9333EA\"\n");
        }
        if let Some(c) = &carrier {
            let _ = writeln!(k, "carrier = {}", quote(c));
        }
        let _ = writeln!(k, "event = {}", quote(&event));
        self.remember(kw, n, &carrier, &event);
        self.last_message = Some((carrier, event));
        self.step("message", &k);
        Ok(())
    }

    fn note(&mut self, line: &str, lib: &Library) -> Result<(), String> {
        if self.keyword.is_some() {
            return Err("a keyword directive must be followed by its message, not a note".into());
        }
        let (place, text) = line.split_once(':').ok_or("a note needs ': text'")?;
        let who = place.split_whitespace().last().unwrap_or_default().split(',').next().unwrap_or_default().to_string();
        self.participant(&who, &who, lib);
        // A pipette already adds this note.
        if self.last_pipette && text.trim().starts_with("Pipetting") {
            return Ok(());
        }
        let k = format!("actor = {}\ntext = {}\n", quote(&self.actor(&who)), quote(text.trim()));
        self.step("note", &k);
        Ok(())
    }
}

/// The geometry named in the diagram, if any.
pub fn geometry_of(lines: &[Line]) -> Option<String> {
    lines.iter().find_map(|l| l.text.trim().strip_prefix("%%")?.trim().strip_prefix("geometry ").map(|g| g.trim().to_string()))
}

/// Builds the text of a scene from a diagram given as text.
#[cfg(test)]
pub fn import(src: &str, name: &str, geometry: Option<&str>, lib: &Library) -> Result<String, String> {
    let lines: Vec<Line> = src.lines().enumerate().map(|(i, t)| Line { file: String::new(), no: i + 1, text: t.to_string() }).collect();
    let g = geometry.map(String::from).or_else(|| geometry_of(&lines));
    let geo = match g {
        Some(g) => {
            let path = format!("{}/scenes/common/{g}.toml", env!("CARGO_MANIFEST_DIR"));
            crate::toml::parse_file(Path::new(&path)).ok().map(|d| Geometry::from_doc(&d).unwrap()).unwrap_or_default()
        }
        None => Geometry::default(),
    };
    import_lines(&lines, name, geometry, lib, geo)
}

/// Builds the text of a scene from a diagram. `geometry` wins over the one in the file.
pub fn import_lines(lines: &[Line], name: &str, geometry: Option<&str>, lib: &Library, geo: Geometry) -> Result<String, String> {
    let mut im = Importer { geometry: geometry.map(String::from), geo, ..Default::default() };
    let mut started = false;
    for l in lines {
        let line = l.text.trim();
        let place = l.place();
        im.place = place.clone();
        let at = |e: String| format!("{place}: {e}");
        if line.is_empty() || line.starts_with("%%{") {
            continue;
        }
        if let Some(d) = line.strip_prefix("%%") {
            if !(geometry.is_some() && d.trim().starts_with("geometry ")) {
                im.directive(d.trim()).map_err(at)?;
            }
            continue;
        }
        if !started {
            if line != "sequenceDiagram" {
                return Err(at("the diagram must start with 'sequenceDiagram'".to_string()));
            }
            started = true;
            continue;
        }
        let first = line.split_whitespace().next().unwrap_or_default();
        match first {
            "participant" | "actor" => {
                let rest = line[first.len()..].trim();
                let (id, label) = rest.split_once(" as ").unwrap_or((rest, rest));
                im.participant(id.trim(), label.trim(), lib);
            }
            "Note" | "note" => im.note(line, lib).map_err(at)?,
            "autonumber" | "activate" | "deactivate" | "rect" | "end" => {}
            "alt" | "else" | "opt" | "loop" | "par" | "and" | "critical" | "break" => {
                return Err(at(format!("'{first}' is not supported: a scene is one path, write one diagram for each case")));
            }
            _ => im.message(line, lib).map_err(at)?,
        }
    }
    if !started {
        return Err("the diagram must start with 'sequenceDiagram'".into());
    }
    if let Some((kw, _)) = im.keyword {
        return Err(format!("'{kw}' at the end of the diagram: its message is missing"));
    }
    let g = im.geometry.clone().ok_or_else(|| format!("no geometry: add '%% geometry <name>' or --geometry ({GEOMETRIES})"))?;
    if im.steps.is_empty() {
        return Err("the diagram has no messages or actions".into());
    }
    let mut o = format!("# Scene generated from the Mermaid diagram {name}.\n\n");
    let _ = writeln!(o, "include = [\"common/video.toml\", \"common/actors.toml\", \"common/messages.toml\", \"common/{g}.toml\"]\n");
    o.push_str("[video]\n");
    if !im.title.is_empty() {
        let _ = writeln!(o, "title = {}", quote(&im.title));
    }
    let _ = writeln!(o, "subtitle = {}\n", quote(if im.subtitle.is_empty() { name } else { &im.subtitle }));
    for (_, id, label) in im.actors.iter().filter(|a| !a.2.is_empty()) {
        let _ = writeln!(o, "[[actor]]\nid = {}\nlabel = {}\n", quote(id), quote(label));
    }
    for c in &im.carriers {
        o.push_str(c);
        o.push('\n');
    }
    for s in &im.steps {
        o.push_str(s);
        o.push('\n');
    }
    Ok(o)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toml;
    use std::path::{Path, PathBuf};

    fn library() -> Library {
        let doc = toml::parse_file(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/scenes/common/messages.toml"))).unwrap();
        let actors = toml::parse_file(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/scenes/common/actors.toml"))).unwrap();
        Library {
            actors: actors.arrays["actor"].iter().map(|t| (t.str_req("id").unwrap(), t.str_req("label").unwrap())).collect(),
            messages: doc.arrays["message"].iter().map(|t| MsgType::from_table(t).unwrap()).collect(),
        }
    }

    /// Compiles generated scene text next to the shared files.
    fn build(text: &str, tag: &str) -> (Scene, Timeline) {
        let dir: PathBuf = std::env::temp_dir().join(format!("trackviz_mmd_{tag}_{}", std::process::id()));
        let common = dir.join("common");
        std::fs::create_dir_all(&common).unwrap();
        for e in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/scenes/common")).unwrap() {
            let p = e.unwrap().path();
            std::fs::copy(&p, common.join(p.file_name().unwrap())).unwrap();
        }
        let f = dir.join("s.toml");
        std::fs::write(&f, text).unwrap();
        let doc = toml::parse_file(&f).unwrap();
        let sc = Scene::from_doc(&doc).unwrap();
        let tl = Timeline::build(&sc).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
        (sc, tl)
    }

    const EX: &str = "%% geometry spur\nsequenceDiagram\n%% carrier C1 SID-1\nIF->>AN: Prepare SampleId=SID-1\nAN-->>IF: OK SampleId=SID-1\nAN->>DMX: Query SampleId=SID-1\nDMX-->>AN: Tests to perform SampleId=SID-1\nIF->>AN: Sample Acquisition SampleId=SID-1\nAN-->>IF: OK\n%% pipette sp\nNote over AN: Pipetting\nAN->>IF: Sample Complete SampleId=SID-1\nIF-->>AN: OK\n";

    /// The arrows of a diagram, without comments and layout.
    fn lines(s: &str) -> Vec<String> {
        s.lines().map(str::trim).filter(|l| l.contains("->>") || l.starts_with("Note")).filter(|l| !l.starts_with("%%")).map(String::from).collect()
    }

    #[test]
    fn a_diagram_builds_a_valid_scene_and_exports_the_same_messages() {
        let text = import(EX, "example", None, &library()).unwrap();
        let (sc, tl) = build(&text, "example");
        assert!(crate::lint::warnings(&sc, &tl).is_empty(), "{:?}", crate::lint::warnings(&sc, &tl));
        assert_eq!(lines(&export(&sc, &tl)), lines(EX));
    }

    const HEAD: &str = "%% geometry spur\nsequenceDiagram\n%% carrier C1 SID-1\n";

    #[test]
    fn messages_use_the_library_and_a_second_prepare_is_the_purple_one() {
        let src = format!("{HEAD}IF->>AN: Prepare SampleId=SID-1\nAN-->>IF: OK SampleId=SID-1\nIF->>AN: Prepare SampleId=SID-1\nIF->>AN: Sample Acquisition SampleId=SID-1 Needle=2\n");
        let t = import(&src, "x", None, &library()).unwrap();
        for ty in ["prepare", "sample_ok", "prepare_again", "sample_acquisition_needle"] {
            assert!(t.contains(&format!("type = \"{ty}\"")), "{ty}: {t}");
        }
        assert!(t.contains("needle = 2"));
    }

    #[test]
    fn participants_are_found_by_label_and_new_ones_are_added() {
        let src = "%% geometry spur\nsequenceDiagram\nparticipant I as Interface\nparticipant L as Lab\n%% carrier C1 SID-1\nI->>AN: Prepare SampleId=SID-1\nI->>L: Hello\n";
        let t = import(src, "x", None, &library()).unwrap();
        assert!(t.contains("type = \"prepare\""), "{t}");
        assert!(t.contains("[[actor]]\nid = \"L\"\nlabel = \"Lab\""), "{t}");
        assert!(t.contains("text = \"Hello\""));
    }

    #[test]
    fn timing_directives_apply_to_the_next_step() {
        let src = format!("{HEAD}%% move C1 to nsd ease=in\n%% when C1 at nsd\n%% delay 0.5\nIF->>AN: Prepare SampleId=SID-1\n%% with\n%% highlight nsd duration=2\n");
        let t = import(&src, "x", None, &library()).unwrap();
        assert!(t.contains("when = \"C1 at nsd\"\ndelay = 0.5\ntype = \"prepare\""), "{t}");
        assert!(t.contains("start = \"with\"\ndevice = \"nsd\"\nduration = 2"), "{t}");
    }

    #[test]
    fn for_duration_and_runaway() {
        let src = format!("{HEAD}%% move C1 to nsd runaway\n%% for C1\nAN-->>IF: OK\n%% duration 6\nNote over AN: Busy\n");
        let t = import(&src, "x", None, &library()).unwrap();
        assert!(t.contains("runaway = true"), "{t}");
        assert!(t.contains("type = \"analyzer_ok\"\ncarrier = \"C1\""), "{t}");
        assert!(t.contains("duration = 6\nactor = \"AN\""), "{t}");
    }

    const KW: &str = "%% geometry spur2\nsequenceDiagram\n%% carrier C1 SID-1 needle=both\n%% carrier C2 SID-2 needle=2\n\
        %% move C1 to d1\n%% move C1 to s1\n%% move C2 to w1\n\
        %% Prepare 1\nIF->>AN: Prepare it now SampleId=SID-1\n%% OK\nAN-->>IF: Fine\n%% Query\nAN->>DMX: Which tests? SampleId=SID-1\n%% Tests\nDMX-->>AN: These\n\
        %% Prepare 2\nIF->>AN: Prepare SampleId=SID-1\n%% Prepare 1\nIF->>AN: Prepare SampleId=SID-2\n%% OK\nAN-->>IF: OK\n%% Query\nAN->>DMX: Query SampleId=SID-2\n\
        %% Sample Acquisition 1\nIF->>AN: Go SampleId=SID-1\n%% pipette sp1 duration=3\n%% Sample Complete\nAN->>IF: Done SampleId=SID-1\n%% Results\nAN->>DMX: Results SampleId=SID-1\n";

    fn kw_times(src: &str) -> Vec<(String, f64)> {
        let (_, tl) = build(&import(src, "x", None, &library()).unwrap(), "kw");
        tl.events.iter().filter_map(|e| match &e.ev { Ev::Message { text, .. } => Some((text.clone(), (e.t0 * 100.0).round() / 100.0)), _ => None }).collect()
    }

    #[test]
    fn keywords_send_the_message_written_by_the_user_at_its_moment() {
        let t = import(KW, "x", None, &library()).unwrap();
        // The text as written, not a library message; the second divert point is purple.
        assert!(t.contains("text = \"Prepare it now\"\nparams = \"SampleId=SID-1\""), "{t}");
        assert!(t.contains("when = \"C1 at d1\"") && t.contains("color = \"#9333EA\""), "{t}");
        assert!(t.contains("when = \"C1 at nsd\"") && t.contains("when = \"C2 at nsd\""), "{t}");
        assert!(t.contains("when = \"ok C1 arrived\"") && t.contains("when = \"ok C2 arrived and tests arrived\""), "{t}");
        assert!(t.contains("when = \"C1 at s1\"") && t.contains("when = \"pipette sp1 done\"") && t.contains("start = \"after_all\""), "{t}");
        let m = kw_times(KW);
        let at = |name: &str| m.iter().find(|x| x.0 == name).unwrap().1;
        let same = |a: f64, b: f64| assert!((a - b).abs() < 0.011, "{a} != {b}");
        // OK as soon as the Prepare has arrived, Tests as soon as the Query has.
        same(at("Fine"), at("Prepare it now") + 1.5);
        same(at("These"), at("Which tests?") + 1.5);
        // The second Query waits for the reply to the first one.
        assert!(at("Query") >= at("These") + 1.49);
        // The pipette starts when the message has arrived and lasts 3 s.
        same(at("Done"), at("Go") + 1.5 + 3.0);
    }

    #[test]
    fn keyword_errors() {
        let k = |body: &str| format!("{KW_HEAD}{body}");
        fails(&k("%% Sanple Acquistion 1\nIF->>AN: X SampleId=SID-1\n"), "did you mean 'Sample Acquisition'");
        fails(&k("%% Prepare\nIF->>AN: X SampleId=SID-1\n"), "needs the number of the divert point");
        fails(&k("%% Prepare 4\nIF->>AN: X SampleId=SID-1\n"), "has 3 divert point(s)");
        fails(&k("%% Prepare 1\n%% move C1 to d1\n"), "must be followed by its message");
        fails(&k("%% Prepare 1\nNote over AN: x\n"), "not a note");
        fails(&k("%% Prepare 1\n"), "its message is missing");
        fails(&k("%% Sample Acquisition\nIF->>AN: X SampleId=SID-1\n"), "this geometry has 2");
        fails(&k("%% OK\nAN-->>IF: OK\n"), "there is none before it");
        fails(&k("%% Query\nAN->>DMX: Q SampleId=SID-1\n"), "no OK to carrier 'C1'");
        fails(&k("%% when C1 at d1\n%% Prepare 1\nIF->>AN: X SampleId=SID-1\n"), "sets the moment itself");
        fails(&k("%% Release 2\nIF->>AN: X SampleId=SID-1\n"), "takes no number");
    }

    const KW_HEAD: &str = "%% geometry spur2\nsequenceDiagram\n%% carrier C1 SID-1\n";

    #[test]
    fn a_note_after_a_pipette_is_not_repeated() {
        let src = format!("{HEAD}%% pipette sp\nNote over AN: Pipetting\nNote over AN: Other\n");
        let t = import(&src, "x", None, &library()).unwrap();
        assert_eq!(t.matches("action = \"note\"").count(), 1, "{t}");
    }

    #[test]
    fn every_default_route_exists() {
        for (g, needle) in [("sampling", "1"), ("turnstile", "1"), ("spur", "1"), ("spur2", "1"), ("spur2", "2"), ("spur2", "both")] {
            let src = format!("%% geometry {g}\nsequenceDiagram\n%% carrier C1 SID-1 needle={needle}\n%% carrier C2 SID-2 needle={needle}\n%% move C1 to merge\n");
            let t = import(&src, "x", None, &library()).unwrap_or_else(|e| panic!("{g}: {e}"));
            if g == "turnstile" {
                continue; // a move through the wheel needs rotations
            }
            build(&t, &format!("{g}{needle}"));
        }
    }

    fn fails(src: &str, what: &str) {
        let e = import(src, "x", None, &library()).unwrap_err();
        assert!(e.contains(what), "'{e}' does not contain '{what}'");
    }

    #[test]
    fn import_errors() {
        fails("graph TD\n", "must start with");
        fails("sequenceDiagram\nIF->>AN: Hello\n", "no geometry");
        fails(&format!("{HEAD}%% mvoe C1 to nsd\n"), "unknown directive 'mvoe'");
        fails(&format!("{HEAD}%% move C1 nsd\n"), "'move' is written");
        fails(&format!("{HEAD}alt ok\nIF->>AN: A\nend\n"), "'alt' is not supported");
        fails(&format!("{HEAD}IF->>AN: Prepare SampleId=SID-9\n"), "no carrier has sample id 'SID-9'");
        fails("sequenceDiagram\n%% carrier C1 SID-1\n", "geometry must be given before");
        fails(&format!("{HEAD}%% when C1 at nsd\n%% at 3\nIF->>AN: A\n"), "only one of");
        fails(&format!("{HEAD}%% delay soon\nIF->>AN: A\n"), "needs seconds");
    }

    #[test]
    fn a_plain_comment_and_mermaid_init_are_skipped() {
        let src = format!("{HEAD}%% # just a note\n%%{{init: {{}}}}%%\nIF->>AN: Hello\n");
        assert!(import(&src, "x", None, &library()).is_ok());
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
                ("common/b.mmd", "%% geometry spur\n%% include c.mmd\n\n%% caption T | x\n"),
                ("common/c.mmd", "%% carrier C1 SID-1\n"),
            ],
        );
        let mut files = Vec::new();
        let lines = read_lines(&dir.join("a.mmd"), &mut files).unwrap();
        assert_eq!(files.len(), 3);
        let t = import_lines(&lines, "a", None, &library(), Geometry::default()).unwrap();
        assert!(t.contains("id = \"C1\"") && t.contains("type = \"prepare\"") && t.contains("title = \"T\""), "{t}");
        // Every block says where it comes from, in the file that holds it.
        assert!(t.contains("c.mmd line 1"), "{t}");
        assert!(t.contains("a.mmd line 3"), "{t}");
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

    #[test]
    fn errors_point_to_the_line_of_the_diagram() {
        let text = "x\n# from a.mmd line 7\n[[step]]\naction = \"move\"\n";
        assert_eq!(locate(text, "[step] at line 3: unknown carrier"), "[step] at a.mmd line 7: unknown carrier");
        assert_eq!(locate(text, "no place here"), "no place here");
    }

    #[test]
    fn colored_messages_are_framed_in_the_export() {
        assert_eq!(rect_color("#9333EA"), "rgba(147, 51, 234, 0.15)");
    }
}
