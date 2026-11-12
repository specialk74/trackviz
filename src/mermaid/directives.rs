//! The `%%` directives.

use super::*;
use super::suggest::suggest;
use crate::doc::{parse_line, Table, Value};
use std::collections::HashMap;

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

impl Importer {
    pub(super) fn directive(&mut self, d: &str) -> Result<(), String> {
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
                self.set_geometry(plain[0])?;
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
                self.set_color(plain[0])?;
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
            "move" => self.move_step(&plain, &o)?,
            "pipette" | "rotate" | "highlight" | "operator" => {
                need(1, &format!("{cmd} <device> [duration=s]"))?;
                self.device_step(cmd, plain[0], &o)?;
            }
            "pass" | "divert" => self.pass_step(cmd, &plain, &o)?,
            "needle" => {
                need(2, "needle <antenna> down|up [duration=s]")?;
                self.needle_step(plain[0], plain[1], &o)?;
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

    /// `%% geometry <name>`: the track of the scene, from `common/<name>.mmd`.
    fn set_geometry(&mut self, name: &str) -> Result<(), String> {
        let base = self.base.clone().ok_or("'geometry' needs a diagram file, with its common/ folder")?;
        let path = base.join("common").join(format!("{name}.mmd"));
        if !path.exists() {
            return Err(format!("geometry '{name}': {} not found", path.display()));
        }
        self.geometry = Some(name.to_string());
        self.include(&path)
    }

    /// `%% color #RRGGBB`: the color of the next message.
    fn set_color(&mut self, c: &str) -> Result<(), String> {
        if !(c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|h| h.is_ascii_hexdigit())) {
            return Err(format!("'color' needs a color like #9333EA, not '{c}'"));
        }
        self.color = Some(c.to_string());
        Ok(())
    }

    /// `%% move <carrier> to <antenna>|<point>`.
    fn move_step(&mut self, plain: &[&str], o: &HashMap<&str, &str>) -> Result<(), String> {
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
        k.extend(duration(o)?);
        if plain.contains(&"runaway") {
            k.push(("runaway", Value::Bool(true)));
        }
        self.step("move", k)
    }

    /// `%% pipette|rotate|highlight|operator <device>`; a pipette works on the
    /// antenna of a sampling point (`probe`).
    fn device_step(&mut self, cmd: &str, name: &str, o: &HashMap<&str, &str>) -> Result<(), String> {
        let device = if cmd == "pipette" { self.probe(cmd, name)? } else { name.to_string() };
        let mut k: Keys = vec![("device", text(&device))];
        k.extend(duration(o)?);
        // The sample the analyzer aspirates, for its note: sid=.
        match (cmd, o.get("sid")) {
            ("pipette", Some(v)) => k.push(("sample_id", text(v))),
            (_, Some(_)) => return Err(format!("'{cmd}' takes no sid: only pipette does")),
            _ => {}
        }
        self.step(cmd, k)
    }

    /// `%% pass|divert <antenna> <carrier>`: the carrier stopped on the antenna goes
    /// on (pass) or is diverted (divert).
    fn pass_step(&mut self, cmd: &str, plain: &[&str], o: &HashMap<&str, &str>) -> Result<(), String> {
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
        k.extend(duration(o)?);
        self.step("pass", k)
    }

    /// `%% needle <antenna> down|up`.
    fn needle_step(&mut self, antenna: &str, to: &str, o: &HashMap<&str, &str>) -> Result<(), String> {
        let device = self.probe("needle", antenna)?;
        let mut k: Keys = vec![("device", text(&device)), ("to", text(to))];
        k.extend(duration(o)?);
        self.step("needle", k)
    }

    pub(super) fn carrier(&mut self, plain: &[&str], o: &HashMap<&str, &str>) -> Result<(), String> {
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
}
