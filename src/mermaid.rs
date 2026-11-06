//! Mermaid sequence diagrams: `export` writes the messages of a scene, `import`
//! builds a scene from a diagram whose `%%` comments describe the physical side.

use crate::scene::{Ev, Scene, Timeline};
use crate::toml::Table;
use std::collections::HashMap;
use std::fmt::Write;

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
}

impl Importer {
    fn step(&mut self, action: &str, keys: &str) {
        let t = std::mem::take(&mut self.timing);
        self.steps.push(format!("[[step]]\naction = \"{action}\"\n{t}{keys}\n"));
        self.last_pipette = action == "pipette";
    }

    fn timing_key(&mut self, line: String) -> Result<(), String> {
        if self.timing.contains("delay") && !line.starts_with("delay") || (!self.timing.is_empty() && !self.timing.starts_with("delay") && !line.starts_with("delay")) {
            return Err("only one of when, at, with for a step".into());
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
            "move" => {
                if plain.len() != 3 || plain[1] != "to" {
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
                let k = format!("carrier = {}\n", quote(plain[0]));
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
            o => return Err(format!("unknown directive '{o}' (for a plain comment write '%% # text')")),
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
        let mut c = format!("[[carrier]]\nid = {}\ntube = true\nsample_id = {}\nroute = [{}]\nspeed = {speed}\n", quote(id), quote(sample), list.join(", "));
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
        let carrier = match sample {
            Some(s) => Some(self.samples.iter().find(|c| c.1 == s).ok_or_else(|| format!("no carrier has sample id '{s}'"))?.0.clone()),
            None => None,
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
        self.step("message", &k);
        Ok(())
    }

    fn note(&mut self, line: &str, lib: &Library) -> Result<(), String> {
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
pub fn geometry_of(src: &str) -> Option<String> {
    src.lines().find_map(|l| l.trim().strip_prefix("%%")?.trim().strip_prefix("geometry ").map(|g| g.trim().to_string()))
}

/// Builds the text of a scene from a diagram. `geometry` wins over the one in the file.
pub fn import(src: &str, name: &str, geometry: Option<&str>, lib: &Library) -> Result<String, String> {
    let mut im = Importer { geometry: geometry.map(String::from), ..Default::default() };
    let mut started = false;
    for (i, raw) in src.lines().enumerate() {
        let line = raw.trim();
        let at = |e: String| format!("line {}: {e}", i + 1);
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

    const EX: &str = include_str!("../examples/spur_happy_path.mmd");

    /// The arrows of a diagram, without comments and layout.
    fn lines(s: &str) -> Vec<String> {
        s.lines().map(str::trim).filter(|l| l.contains("->>") || l.starts_with("Note")).filter(|l| !l.starts_with("%%")).map(String::from).collect()
    }

    #[test]
    fn the_example_builds_a_valid_scene_and_exports_the_same_messages() {
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

    #[test]
    fn colored_messages_are_framed_in_the_export() {
        assert_eq!(rect_color("#9333EA"), "rgba(147, 51, 234, 0.15)");
    }
}
