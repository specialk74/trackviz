//! Scene model (parsed from the scenario file) and timeline computation.

use crate::doc::{Doc, Table};
use crate::physics;
use std::collections::HashMap;

#[derive(Clone)]
pub struct Video {
    pub title: String,
    pub subtitle: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub tail: f64,
    pub show_clock: bool,
    pub show_times: bool,
    /// Speed of the belt under the carriers, in cm/s (`belt=`).
    pub belt: f64,
    /// How long the arrow of a message takes, in s, when the message has no
    /// `duration` (`message=`).
    pub message: f64,
}

impl Video {
    /// The belt speed on screen: a carrier 4 cm wide is drawn 44 px wide (11 px/cm).
    pub fn belt_px(&self) -> f64 {
        self.belt * PX_PER_CM
    }
}

#[derive(Clone)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(PartialEq, Clone, Copy)]
pub enum LaneKind {
    Main,
    Pit,
    Link,
}

#[derive(Clone)]
pub struct Lane {
    pub id: String,
    pub label: String,
    pub points: Vec<String>,
    /// The most carriers that can be in this lane at once, counted from their
    /// diversion off the Main-Lane until they pass the first antenna (and its
    /// gate); one more goes straight on.
    pub capacity: Option<usize>,
    pub kind: LaneKind,
    pub label_pos: Option<(f64, f64)>,
}

#[derive(PartialEq, Clone, Copy)]
pub enum DeviceKind {
    Nsd,
    Sampling,
    Analyzer,
    Zone,
    Turnstile,
    /// Barrier across a lane; opens by itself while a carrier drives through it.
    Gate,
    /// Reader on the lane, under the carriers; it reads a carrier over its point.
    /// With `gate=true` a gate right after it holds carriers on it (the belt under
    /// the carriers never stops); `visible=false` makes it a trigger only.
    Antenna,
    /// Standard diversion: every carrier stops here, is read, then is diverted or goes straight.
    Divert,
}

#[derive(Clone)]
pub struct Device {
    pub id: String,
    pub kind: DeviceKind,
    /// Reference point: given for nsd/sampling; for a turnstile it is the
    /// generated "<id>.needle" point (so pipette/highlight work unchanged).
    pub at: Option<String>,
    /// Box for analyzer/zone; for a turnstile (cx, cy, r, 0) with r the
    /// distance from the center to the pocket centers.
    pub rect: (f64, f64, f64, f64),
    pub label: String,
    /// Turnstile only: the entry position is on the right (flow right to left).
    pub entry_right: bool,
    /// Label position (center of the text) for nsd/sampling, if not the default.
    pub label_pos: Option<(f64, f64)>,
    /// Drawn (an antenna can be a trigger only).
    pub visible: bool,
    /// For an antenna or a gate declared on another device (`antenna=`, `gate=`):
    /// that device. In the layout they are named together, e.g. sp.antenna2.gate.
    pub part_of: Option<String>,
}

/// Default speed of the belt under the carriers (cm/s); `belt=` in `@video`
/// changes it.
pub const BELT_SPEED: f64 = 17.5;
/// Scale of the drawing: a carrier, 4 cm wide, is drawn 2 × CARRIER_R px wide.
pub const PX_PER_CM: f64 = 2.0 * CARRIER_R / 4.0;

/// Shortest time (s) a device stays lit when its antenna reads a carrier.
pub const READ_LIGHT: f64 = 1.0;

/// A gate opens (or closes) in this time (s): it lets one carrier through.
pub const GATE_TIME: f64 = 0.22;

/// Turnstile: distance from the pocket centers to the rim of the wheel.
pub const TS_RIM: f64 = 32.0;
/// Radius of a carrier as drawn on the track.
pub const CARRIER_R: f64 = 22.0;

/// Turnstile geometry. Three pockets 120° apart; positions (slots) are
/// 0 = entry, 1 = under the needle (top), 2 = exit. Each rotation moves every
/// pocket one slot forward: entry -> needle -> exit -> entry.
pub fn turnstile_slot_angle(entry_right: bool, slot: f64) -> f64 {
    use std::f64::consts::PI;
    // Screen angles (y grows downwards): entry 30° below the horizontal on its
    // side, needle straight up, exit 30° below the horizontal on the other side.
    let (a0, delta) = if entry_right { (PI / 6.0, -2.0 * PI / 3.0) } else { (5.0 * PI / 6.0, 2.0 * PI / 3.0) };
    a0 + slot * delta
}

#[derive(Clone)]
pub struct Actor {
    pub id: String,
    pub label: String,
    pub sub: String,
    pub color: String,
    /// The actor of the analyzer: `pipette` adds its "Pipetting" note here.
    pub analyzer: bool,
}

#[derive(Clone)]
pub struct Carrier {
    pub id: String,
    pub tube: bool,
    pub sample_id: String,
    pub route: Vec<String>,
    pub start: String,
    pub speed: f64,
    pub cap_color: String,
    /// Logical carrier only: where the interface believes a carrier is, while
    /// physically it is somewhere else. Drawn as a dashed outline; it is not
    /// read by antennas and does not open gates.
    pub ghost: bool,
    /// Visible from the start (otherwise it appears with a `show` step).
    pub visible: bool,
    /// Text of the tag next to the carrier (default: the SampleId).
    pub tag: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Ease {
    Linear,
    In,
    Out,
    InOut,
}

impl Ease {
    pub fn apply(self, x: f64) -> f64 {
        let x = x.clamp(0.0, 1.0);
        use std::f64::consts::PI;
        match self {
            Ease::Linear => x,
            Ease::In => 1.0 - (PI * x / 2.0).cos(),
            Ease::Out => (PI * x / 2.0).sin(),
            Ease::InOut => (1.0 - (PI * x).cos()) / 2.0,
        }
    }
    /// Duration multiplier so that the peak speed equals the nominal speed
    /// (keeps velocity continuous when an "in" move is followed by an "out" move).
    fn factor(self) -> f64 {
        match self {
            Ease::Linear => 1.0,
            _ => std::f64::consts::PI / 2.0,
        }
    }
}

#[derive(Clone)]
pub enum Action {
    Caption { title: String, text: String },
    /// `runaway`: the carrier escapes by itself (drawn in red, tag "RUN-AWAY").
    Move { carrier: String, to: String, ease: Ease, speed: Option<f64>, runaway: bool },
    /// The carrier stopped on the point of `device` sets off and rides the belt to
    /// the next stop of its route (a gated antenna, a turnstile entry) or its end.
    /// `pass` / `divert`: the carrier stopped on the antenna goes on, or turns.
    /// `carrier`: which one it must be.
    Pass { device: String, divert: bool, carrier: Option<String>, speed: Option<f64> },
    Message { kind: Option<String>, from: String, to: String, text: String, params: String, reply: bool, carrier: Option<String>, color: Option<String>, needle: Option<u32> },
    Note { actor: String, text: String },
    /// `note`: the text of the note on the analyzer, if the diagram gives it;
    /// `sample`: the SampleId the analyzer aspirates, shown in that note.
    Pipette { device: String, note: Option<String>, sample: Option<String> },
    Highlight { device: String },
    Rotate { device: String },
    /// Fades a carrier in (`show`) or out (`hide`).
    Visibility { carrier: String, show: bool },
    /// Analyzer state shown on the track: error (red), ready (green), normal.
    Status { device: String, state: AnalyzerState },
    /// The analyzer needle goes down into the tube (and stays) or back up.
    Needle { device: String, down: bool },
    /// An operator walks to the analyzer, works on it and walks away.
    Operator { device: String },
    Wait,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AnalyzerState {
    Normal,
    Error,
    Ready,
}

impl AnalyzerState {
    fn name(self) -> &'static str {
        match self {
            AnalyzerState::Normal => "normal",
            AnalyzerState::Error => "error",
            AnalyzerState::Ready => "ready",
        }
    }
}

/// When a step starts, as the directives say; `delay` is added on top.
#[derive(Clone, PartialEq)]
pub enum StartMode {
    /// When the step before ends (`after <seconds>`, or nothing).
    After,
    /// At that second of the video (`at <seconds>`).
    At(f64),
    /// When something earlier in the file happens (`on <antenna>`,
    /// `after <seconds> <what>`).
    When(Anchor),
}

/// What a step waits for.
#[derive(Clone, PartialEq)]
pub enum Anchor {
    /// "C1 at merge": the carrier reaches the point.
    Arrival { carrier: String, point: String },
    /// "query C1 arrived", "pipette sp done", "step q1 done" (the step with that
    /// id): the end of the latest matching step before this one in the file.
    Event { key: String },
}

impl Anchor {
    fn parse(w: &str) -> Option<Anchor> {
        let t: Vec<&str> = w.split_whitespace().collect();
        match t[..] {
            [carrier, "at", point] => Some(Anchor::Arrival { carrier: carrier.into(), point: point.into() }),
            [ref who @ .., "arrived"] if (1..=2).contains(&who.len()) => Some(Anchor::Event { key: who.join(" ") }),
            [kind @ ("pipette" | "rotate" | "needle" | "step"), name, "done"] => Some(Anchor::Event { key: format!("{kind} {name}") }),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct Step {
    pub action: Action,
    /// Where it is written: (file, line); the file is empty when given as text.
    pub at: (String, usize),
    /// `id`: its name, for the steps that start when it ends.
    pub id: Option<String>,
    pub start: StartMode,
    pub delay: f64,
    pub duration: Option<f64>,
    pub loc: String,
}

#[derive(Clone)]
pub struct Scene {
    pub video: Video,
    pub points: HashMap<String, Point>,
    pub lanes: Vec<Lane>,
    pub devices: Vec<Device>,
    pub actors: Vec<Actor>,
    pub carriers: Vec<Carrier>,
    pub steps: Vec<Step>,
    /// Carriers sent straight on because their pit-lane was full (set by
    /// `Timeline::build`).
    pub straight: Vec<String>,
}

fn sc_pt(points: &HashMap<String, Point>, id: &str) -> (f64, f64) {
    let p = &points[id];
    (p.x, p.y)
}

/// Direction of travel of the lane through point p (lanes list their points in
/// the direction of the carriers); right to left if p is on no lane.
fn lane_dir(points: &HashMap<String, Point>, lanes: &[Lane], p: (f64, f64)) -> (f64, f64) {
    for l in lanes {
        let pts: Vec<(f64, f64)> = l.points.iter().map(|id| sc_pt(points, id)).collect();
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
            if len < 1e-9 {
                continue;
            }
            let (ux, uy) = ((b.0 - a.0) / len, (b.1 - a.1) / len);
            let along = (p.0 - a.0) * ux + (p.1 - a.1) * uy;
            let off = ((p.0 - a.0) * uy - (p.1 - a.1) * ux).abs();
            // On the segment, not at its end (at a corner the next segment counts).
            if off < 3.0 && along >= -1e-9 && along < len - 1e-9 {
                return (ux, uy);
            }
        }
    }
    (-1.0, 0.0)
}

/// The points of each lane in the direction of the carriers, with the points
/// that lie on its segments (the vertices alone skip queue positions, sampling
/// points, ...). Generated gate points are left out.
fn lane_paths(points: &HashMap<String, Point>, lanes: &[Lane]) -> Vec<(String, Vec<String>)> {
    lanes
        .iter()
        .map(|l| {
            let mut out: Vec<String> = Vec::new();
            for w in l.points.windows(2) {
                let (a, b) = (sc_pt(points, &w[0]), sc_pt(points, &w[1]));
                let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
                let mut on: Vec<(f64, &String)> = points
                    .iter()
                    .filter(|(id, _)| !id.ends_with(".gate") && **id != w[0] && **id != w[1])
                    .filter_map(|(id, p)| {
                        let (ux, uy) = ((b.0 - a.0) / len, (b.1 - a.1) / len);
                        let along = (p.x - a.0) * ux + (p.y - a.1) * uy;
                        let off = ((p.x - a.0) * uy - (p.y - a.1) * ux).abs();
                        (off < 3.0 && along > 0.0 && along < len).then_some((along, id))
                    })
                    .collect();
                on.sort_by(|x, y| x.0.total_cmp(&y.0));
                if out.last() != Some(&w[0]) {
                    out.push(w[0].clone());
                }
                out.extend(on.into_iter().map(|x| x.1.clone()));
                out.push(w[1].clone());
            }
            (l.id.clone(), out)
        })
        .collect()
}

/// A route as written: points and lane ids, in order. Each lane id stands for its
/// points; between two consecutive places the shortest way along the lanes (in
/// their direction, and through a turnstile from "<id>.in" to "<id>.out") is
/// filled in. "main_in,pit,main_out" is main_in, nsd, pit_in, ..., pit_out, merge,
/// main_out. Two points that no lane joins are joined by a straight line.
/// The lanes as a graph: from each point, the next points along the lanes (in
/// their direction), with the length and the lane; through a turnstile from
/// "<id>.in" to "<id>.out" (lane "").
struct LaneGraph {
    edges: HashMap<String, Vec<(String, f64, String)>>,
}

impl LaneGraph {
    fn new(points: &HashMap<String, Point>, lanes: &[Lane], devices: &[Device]) -> LaneGraph {
        let mut edges: HashMap<String, Vec<(String, f64, String)>> = HashMap::new();
        for (lane, path) in lane_paths(points, lanes) {
            for w in path.windows(2) {
                let (p, q) = (sc_pt(points, &w[0]), sc_pt(points, &w[1]));
                let d = ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt();
                edges.entry(w[0].clone()).or_default().push((w[1].clone(), d, lane.clone()));
            }
        }
        for d in devices.iter().filter(|d| d.kind == DeviceKind::Turnstile) {
            edges.entry(format!("{}.in", d.id)).or_default().push((format!("{}.out", d.id), 0.0, String::new()));
        }
        LaneGraph { edges }
    }

    fn out(&self, p: &str) -> &[(String, f64, String)] {
        self.edges.get(p).map_or(&[][..], |v| v.as_slice())
    }

    /// Shortest way from a to b along the lanes (Dijkstra; the graphs are small),
    /// with the lane of its last stretch.
    fn way(&self, a: &str, b: &str) -> Option<(Vec<String>, String)> {
        // Point -> (distance, previous point and lane).
        type Best<'a> = HashMap<&'a str, (f64, Option<(&'a str, &'a str)>)>;
        let mut best: Best = HashMap::from([(a, (0.0, None))]);
        let mut open = vec![a];
        while let Some(k) = (0..open.len()).min_by(|&x, &y| best[open[x]].0.total_cmp(&best[open[y]].0)) {
            let n = open.swap_remove(k);
            if n == b {
                let mut out = vec![b.to_string()];
                let mut cur = b;
                let last = best[b].1.map_or(String::new(), |x| x.1.to_string());
                while let Some((prev, _)) = best[cur].1 {
                    out.push(prev.to_string());
                    cur = prev;
                }
                out.reverse();
                return Some((out, last));
            }
            let d = best[n].0;
            for (m, w, lane) in self.out(n) {
                if best.get(m.as_str()).is_none_or(|x| d + w < x.0 - 1e-9) {
                    best.insert(m, (d + w, Some((n, lane))));
                    open.push(m);
                }
            }
        }
        None
    }
}

fn expand_route(written: &[String], points: &HashMap<String, Point>, lanes: &[Lane], devices: &[Device]) -> Result<Vec<String>, String> {
    let paths = lane_paths(points, lanes);
    let graph = LaneGraph::new(points, lanes, devices);
    let way = |a: &str, b: &str| graph.way(a, b).map(|w| w.0);
    let mut route: Vec<String> = Vec::new();
    for tok in written {
        let places: Vec<String> = if points.contains_key(tok) {
            vec![tok.clone()]
        } else if let Some((_, p)) = paths.iter().find(|(id, _)| id == tok) {
            p.clone()
        } else {
            return Err(format!("route: '{tok}' is neither a point nor a lane"));
        };
        for p in places {
            match route.last() {
                None => route.push(p),
                Some(last) if *last == p => {}
                // No lane between them: straight from one to the other.
                Some(last) => match way(last, &p) {
                    Some(w) => route.extend(w.into_iter().skip(1)),
                    None => route.push(p),
                },
            }
        }
    }
    Ok(route)
}

fn arr<'a>(doc: &'a Doc, name: &str) -> &'a [Table] {
    doc.arrays.get(name).map(|v| v.as_slice()).unwrap_or(&[])
}

impl Scene {
    pub fn from_doc(doc: &Doc) -> Result<Scene, String> {
        for k in doc.tables.keys() {
            if !k.is_empty() && k != "video" {
                return Err(format!("unknown table [{k}]"));
            }
        }
        for k in doc.arrays.keys() {
            if !["point", "lane", "device", "actor", "carrier", "message", "step"].contains(&k.as_str()) {
                return Err(format!("unknown section @{k}"));
            }
        }
        // Included files make it easy to define the same thing twice.
        for section in ["lane", "device", "actor", "carrier", "message"] {
            let mut seen = Vec::new();
            for t in arr(doc, section) {
                if let Some(id) = t.str_opt("id")? {
                    if seen.contains(&id) {
                        return Err(format!("{}: duplicate {section} id '{id}'", t.location()));
                    }
                    seen.push(id);
                }
            }
        }
        let root = &doc.tables[""];
        if !root.entries.is_empty() {
            return Err("keys outside of any section are not allowed".into());
        }
        let v = doc.tables.get("video").ok_or("missing [video] section")?;
        v.check_keys(&["title", "subtitle", "width", "height", "fps", "tail", "show_clock", "show_times", "belt", "message"])?;
        let video = Video {
            title: v.str_opt("title")?.unwrap_or_default(),
            subtitle: v.str_opt("subtitle")?.unwrap_or_default(),
            width: v.num_opt("width")?.unwrap_or(1920.0) as u32,
            height: v.num_opt("height")?.unwrap_or(1080.0) as u32,
            fps: v.num_opt("fps")?.unwrap_or(30.0) as u32,
            tail: v.num_opt("tail")?.unwrap_or(2.0),
            show_clock: v.bool_opt("show_clock")?.unwrap_or(true),
            show_times: v.bool_opt("show_times")?.unwrap_or(true),
            belt: match v.num_opt("belt")? {
                Some(b) if b > 0.0 => b,
                Some(b) => return Err(format!("{}: belt must be a speed in cm/s above 0, not {b}", v.location())),
                None => BELT_SPEED,
            },
            message: match v.num_opt("message")? {
                Some(m) if m > 0.0 => m,
                Some(m) => return Err(format!("{}: message must be a time in s above 0, not {m}", v.location())),
                None => 1.5,
            },
        };

        let mut points = HashMap::new();
        for t in arr(doc, "point") {
            t.check_keys(&["id", "x", "y"])?;
            let id = t.str_req("id")?;
            let p = Point { x: t.num_req("x")?, y: t.num_req("y")? };
            if points.insert(id.clone(), p).is_some() {
                return Err(format!("{}: duplicate point id '{id}'", t.location()));
            }
        }
        let mut devices = Vec::new();
        let mut gated = Vec::new();
        for t in arr(doc, "device") {
            let kind = match t.str_req("kind")?.as_str() {
                "nsd" => DeviceKind::Nsd,
                "sampling" => DeviceKind::Sampling,
                "analyzer" => DeviceKind::Analyzer,
                "zone" => DeviceKind::Zone,
                "turnstile" => DeviceKind::Turnstile,
                "gate" => DeviceKind::Gate,
                "antenna" => DeviceKind::Antenna,
                "divert" => DeviceKind::Divert,
                o => return Err(format!("{}: unknown device kind '{o}' (nsd|sampling|analyzer|zone|turnstile|gate|antenna|divert)", t.location())),
            };
            let id = t.str_req("id")?;
            let mut at = None;
            let mut rect = (0.0, 0.0, 0.0, 0.0);
            let mut entry_right = true;
            match kind {
                DeviceKind::Nsd | DeviceKind::Sampling | DeviceKind::Gate | DeviceKind::Antenna | DeviceKind::Divert => {
                    if kind == DeviceKind::Antenna {
                        t.check_keys(&["id", "kind", "at", "label", "gate", "visible"])?;
                        if t.bool_opt("gate")?.unwrap_or(false) {
                            gated.push(devices.len());
                        }
                    } else if kind == DeviceKind::Gate {
                        t.check_keys(&["id", "kind", "at", "label", "label_x", "label_y"])?;
                    } else {
                        // nsd, sampling, divert: an antenna on the same point, and its gate.
                        t.check_keys(&["id", "kind", "at", "label", "label_x", "label_y", "antenna", "gate"])?;
                        if t.bool_opt("gate")?.is_some() && t.str_opt("antenna")?.is_none() {
                            return Err(format!("{}: gate= needs antenna=: the gate holds the carrier on the antenna", t.location()));
                        }
                    }
                    let a = t.str_req("at")?;
                    if !points.contains_key(&a) {
                        return Err(format!("{}: unknown point '{a}'", t.location()));
                    }
                    at = Some(a);
                }
                DeviceKind::Analyzer | DeviceKind::Zone => {
                    t.check_keys(&["id", "kind", "x", "y", "w", "h", "label"])?;
                    rect = (t.num_req("x")?, t.num_req("y")?, t.num_req("w")?, t.num_req("h")?);
                }
                DeviceKind::Turnstile => {
                    t.check_keys(&["id", "kind", "x", "y", "r", "entry", "label", "antenna"])?;
                    let (cx, cy, r) = (t.num_req("x")?, t.num_req("y")?, t.num_opt("r")?.unwrap_or(70.0));
                    entry_right = match t.str_opt("entry")?.as_deref().unwrap_or("right") {
                        "right" => true,
                        "left" => false,
                        o => return Err(format!("{}: unknown entry '{o}' (right|left)", t.location())),
                    };
                    rect = (cx, cy, r, 0.0);
                    // Generated points: <id>.in, <id>.needle, <id>.out (usable in lanes and routes).
                    for (slot, suffix) in ["in", "needle", "out"].iter().enumerate() {
                        let a = turnstile_slot_angle(entry_right, slot as f64);
                        let pid = format!("{id}.{suffix}");
                        let p = Point { x: (cx + r * a.cos()).round(), y: (cy + r * a.sin()).round() };
                        if points.insert(pid.clone(), p).is_some() {
                            return Err(format!("{}: point '{pid}' already exists", t.location()));
                        }
                    }
                    at = Some(format!("{id}.needle"));
                }
            }
            let label_pos = match (t.num_opt("label_x")?, t.num_opt("label_y")?) {
                (Some(x), Some(y)) => Some((x, y)),
                (None, None) => None,
                _ => return Err(format!("{}: give both label_x and label_y", t.location())),
            };
            let visible = t.bool_opt("visible")?.unwrap_or(true);
            let antenna = if matches!(kind, DeviceKind::Nsd | DeviceKind::Sampling | DeviceKind::Divert | DeviceKind::Turnstile) { t.str_opt("antenna")? } else { None };
            let gate = t.bool_opt("gate")?.unwrap_or(false);
            // A turnstile's antenna is under the needle.
            let point = if kind == DeviceKind::Turnstile { Some(format!("{id}.needle")) } else { at.clone() };
            devices.push(Device { id: id.clone(), kind, at, rect, label: t.str_opt("label")?.unwrap_or_default(), entry_right, label_pos, visible, part_of: None });
            if let Some(aid) = antenna {
                if gate {
                    gated.push(devices.len());
                }
                devices.push(Device { id: aid, kind: DeviceKind::Antenna, at: point, rect: (0.0, 0.0, 0.0, 0.0), label: String::new(), entry_right: true, label_pos: None, visible: true, part_of: Some(id) });
            }
        }

        let need_point = |id: &str, t: &Table| -> Result<(), String> {
            if points.contains_key(id) {
                Ok(())
            } else {
                Err(format!("{}: unknown point '{id}'", t.location()))
            }
        };

        let mut lanes = Vec::new();
        for t in arr(doc, "lane") {
            t.check_keys(&["id", "label", "points", "kind", "label_x", "label_y", "capacity"])?;
            let pts = t.str_list("points")?;
            if pts.len() < 2 {
                return Err(format!("{}: a lane needs at least 2 points", t.location()));
            }
            for p in &pts {
                need_point(p, t)?;
            }
            let kind = match t.str_opt("kind")?.as_deref().unwrap_or("main") {
                "main" => LaneKind::Main,
                "pit" => LaneKind::Pit,
                "link" => LaneKind::Link,
                o => return Err(format!("{}: unknown lane kind '{o}' (main|pit|link)", t.location())),
            };
            let label_pos = match (t.num_opt("label_x")?, t.num_opt("label_y")?) {
                (Some(x), Some(y)) => Some((x, y)),
                _ => None,
            };
            let capacity = match t.num_opt("capacity")? {
                Some(c) if c >= 1.0 && c.fract() == 0.0 => Some(c as usize),
                Some(c) => return Err(format!("{}: capacity must be a whole number from 1, not {c}", t.location())),
                None => None,
            };
            lanes.push(Lane { id: t.str_opt("id")?.unwrap_or_default(), label: t.str_opt("label")?.unwrap_or_default(), points: pts, kind, label_pos, capacity });
        }

        let mut actors = Vec::new();
        for t in arr(doc, "actor") {
            t.check_keys(&["id", "label", "sub", "color", "analyzer"])?;
            actors.push(Actor {
                id: t.str_req("id")?,
                label: t.str_req("label")?,
                sub: t.str_opt("sub")?.unwrap_or_default(),
                color: t.str_opt("color")?.unwrap_or_else(|| "#334155".into()),
                analyzer: t.bool_opt("analyzer")?.unwrap_or(false),
            });
        }

        let mut carriers = Vec::new();
        for t in arr(doc, "carrier") {
            t.check_keys(&["id", "tube", "sample_id", "route", "start", "speed", "cap_color", "ghost", "visible", "tag"])?;
            // Without `route` the way of the carrier comes from its steps (see
            // `derive_route`), from `start`.
            let route = if t.entries.contains_key("route") {
                expand_route(&t.str_list("route")?, &points, &lanes, &devices).map_err(|e| format!("{}: {e}", t.location()))?
            } else {
                Vec::new()
            };
            let start = match t.str_opt("start")? {
                Some(s) => s,
                None => route.first().cloned().ok_or_else(|| format!("{}: a carrier needs start= (or route=)", t.location()))?,
            };
            if !route.is_empty() && !route.contains(&start) {
                return Err(format!("{}: start point '{start}' is not on the route", t.location()));
            }
            if !points.contains_key(&start) {
                return Err(format!("{}: unknown point '{start}'", t.location()));
            }
            carriers.push(Carrier {
                id: t.str_req("id")?,
                tube: t.bool_opt("tube")?.unwrap_or(true),
                sample_id: t.str_opt("sample_id")?.unwrap_or_default(),
                route,
                start,
                speed: t.num_opt("speed")?.unwrap_or(video.belt_px()),
                cap_color: t.str_opt("cap_color")?.unwrap_or_else(|| "#7C3AED".into()),
                ghost: t.bool_opt("ghost")?.unwrap_or(false),
                visible: t.bool_opt("visible")?.unwrap_or(true),
                tag: t.str_opt("tag")?,
            });
        }

        // Named messages: a `message` step with `type` takes everything from here.
        let mut msg_types = HashMap::new();
        for t in arr(doc, "message") {
            t.check_keys(&["id", "from", "to", "text", "params", "reply", "color"])?;
            for k in ["from", "to", "text"] {
                t.str_req(k)?;
            }
            msg_types.insert(t.str_req("id")?, t);
        }

        let mut steps = Vec::new();
        for t in arr(doc, "step") {
            let common = ["action", "id", "at", "after", "when", "delay", "duration"];
            let action_name = t.str_req("action")?;
            let extra: &[&str] = match action_name.as_str() {
                "caption" => &["title", "text"],
                "move" => &["carrier", "to", "ease", "speed", "runaway"],
                "pass" => &["device", "divert", "carrier", "speed"],
                "message" if t.str_opt("type")?.is_some() => &["type", "carrier", "needle"],
                "message" => &["from", "to", "text", "params", "reply", "carrier", "color", "needle", "event"],
                "note" => &["actor", "text"],
                "pipette" => &["device", "note", "sample_id"],
                "highlight" => &["device"],
                "rotate" => &["device"],
                "show" | "hide" => &["carrier"],
                "status" => &["device", "state"],
                "needle" => &["device", "to"],
                "operator" => &["device"],
                "wait" => &[],
                o => return Err(format!("{}: unknown action '{o}' (caption|move|message|note|pipette|highlight|rotate|show|hide|status|needle|operator|wait)", t.location())),
            };
            let allowed: Vec<&str> = common.iter().chain(extra.iter()).copied().collect();
            t.check_keys(&allowed)?;
            let action = match action_name.as_str() {
                "caption" => Action::Caption { title: t.str_opt("title")?.unwrap_or_default(), text: t.str_req("text")? },
                "pass" => Action::Pass {
                    device: t.str_req("device")?,
                    divert: t.bool_opt("divert")?.unwrap_or(false),
                    carrier: t.str_opt("carrier")?,
                    speed: t.num_opt("speed")?,
                },
                "move" => Action::Move {
                    carrier: t.str_req("carrier")?,
                    to: t.str_req("to")?,
                    ease: match t.str_opt("ease")?.as_deref().unwrap_or("linear") {
                        "linear" => Ease::Linear,
                        "in" => Ease::In,
                        "out" => Ease::Out,
                        "inout" => Ease::InOut,
                        o => return Err(format!("{}: unknown ease '{o}' (linear|in|out|inout)", t.location())),
                    },
                    speed: t.num_opt("speed")?,
                    runaway: t.bool_opt("runaway")?.unwrap_or(false),
                },
                "message" => {
                    // Without `type` the keys are on the step itself.
                    let src = match t.str_opt("type")? {
                        Some(ty) => *msg_types.get(&ty).ok_or_else(|| format!("{}: unknown message type '{ty}'", t.location()))?,
                        None => t,
                    };
                    Action::Message {
                        // The name `when` uses: the type, or `event` for a message written out.
                        kind: t.str_opt("type")?.or(t.str_opt("event")?),
                        from: src.str_req("from")?,
                        to: src.str_req("to")?,
                        text: src.str_req("text")?,
                        params: src.str_opt("params")?.unwrap_or_default(),
                        reply: src.bool_opt("reply")?.unwrap_or(false),
                        carrier: t.str_opt("carrier")?,
                        color: src.str_opt("color")?,
                        needle: t.num_opt("needle")?.map(|n| n as u32),
                    }
                }
                "note" => Action::Note { actor: t.str_req("actor")?, text: t.str_req("text")? },
                "pipette" => Action::Pipette { device: t.str_req("device")?, note: t.str_opt("note")?, sample: t.str_opt("sample_id")? },
                "highlight" => Action::Highlight { device: t.str_req("device")? },
                "rotate" => Action::Rotate { device: t.str_req("device")? },
                "show" | "hide" => Action::Visibility { carrier: t.str_req("carrier")?, show: action_name == "show" },
                "status" => Action::Status {
                    device: t.str_req("device")?,
                    state: {
                        let v = t.str_req("state")?;
                        [AnalyzerState::Normal, AnalyzerState::Error, AnalyzerState::Ready]
                            .into_iter()
                            .find(|s| s.name() == v)
                            .ok_or_else(|| format!("{}: unknown state '{v}' (normal|error|ready)", t.location()))?
                    },
                },
                "operator" => Action::Operator { device: t.str_req("device")? },
                "needle" => Action::Needle {
                    device: t.str_req("device")?,
                    down: match t.str_req("to")?.as_str() {
                        "down" => true,
                        "up" => false,
                        o => return Err(format!("{}: unknown needle position '{o}' (down|up)", t.location())),
                    },
                },
                _ => {
                    if t.num_opt("duration")?.is_none() {
                        return Err(format!("{}: 'wait' needs a duration", t.location()));
                    }
                    Action::Wait
                }
            };
            // When the step starts: `at` (a second of the video), `after` (seconds
            // after the step before ends) or `when` (something earlier happens).
            let (at, after, when) = (t.num_opt("at")?, t.num_opt("after")?, t.str_opt("when")?);
            if [at.is_some(), after.is_some(), when.is_some()].iter().filter(|b| **b).count() > 1 {
                return Err(format!("{}: use only one of 'at', 'after', 'when'", t.location()));
            }
            if after.is_some_and(|v| v < 0.0) {
                return Err(format!("{}: 'after' must not be negative", t.location()));
            }
            let delay = t.num_opt("delay")?.unwrap_or(0.0) + after.unwrap_or(0.0);
            let start = match (when, at) {
                (Some(w), _) => StartMode::When(Anchor::parse(&w).ok_or_else(|| {
                    format!("{}: 'when' is \"<carrier> at <point>\", \"<message type> [<carrier>] arrived\", \"pipette|rotate|needle <device> done\" or \"step <id> done\", not \"{w}\"", t.location())
                })?),
                (None, Some(at)) => StartMode::At(at),
                (None, None) => StartMode::After,
            };
            steps.push(Step {
                action,
                id: t.str_opt("id")?,
                start,
                delay,
                duration: t.num_opt("duration")?,
                loc: t.location(),
                at: (t.file.clone(), t.line),
            });
        }
        if steps.is_empty() {
            return Err("the scene has no steps".into());
        }
        // The gate of an antenna: across the lane, just in front of a carrier
        // standing on the antenna ("<antenna>.gate", a point and a device).
        for di in gated {
            let a = sc_pt(&points, devices[di].at.as_deref().unwrap());
            let (dx, dy) = lane_dir(&points, &lanes, a);
            let id = format!("{}.gate", devices[di].id);
            let ahead = CARRIER_R + 6.0;
            points.insert(id.clone(), Point { x: a.0 + dx * ahead, y: a.1 + dy * ahead });
            let part_of = Some(devices[di].part_of.clone().unwrap_or_else(|| devices[di].id.clone()));
            devices.push(Device { id: id.clone(), kind: DeviceKind::Gate, at: Some(id), rect: (0.0, 0.0, 0.0, 0.0), label: String::new(), entry_right: true, label_pos: None, visible: true, part_of });
        }
        let mut sc = Scene { video, points, lanes, devices, actors, carriers, steps, straight: Vec::new() };
        let routes = sc.derive_routes()?;
        for (c, r) in sc.carriers.iter_mut().zip(routes) {
            if c.route.is_empty() {
                c.route = r;
            }
        }
        Ok(sc)
    }

    /// The ways of the carriers written without `route`, from their steps in the
    /// order of the file: from `start`; a `move` adds the shortest way along the
    /// lanes to its point; `pass` goes straight on, `divert` into the lane that
    /// starts on the antenna, then along the lanes to the next stop (an antenna
    /// with a gate, a turnstile entry) or to the end. All the carriers together, so
    /// that the first wrong step in the file is the one reported, with where the
    /// carrier is and who is on the antenna instead.
    fn derive_routes(&self) -> Result<Vec<Vec<String>>, String> {
        let graph = LaneGraph::new(&self.points, &self.lanes, &self.devices);
        let stops = self.stops();
        let n = self.carriers.len();
        let derived: Vec<bool> = self.carriers.iter().map(|c| c.route.is_empty()).collect();
        let mut routes: Vec<Vec<String>> = self.carriers.iter().map(|c| vec![c.start.clone()]).collect();
        let mut lanes = vec![String::new(); n];
        // The step that put each carrier where it is: (line, what it says).
        let mut last: Vec<Option<(String, String)>> = vec![None; n];
        // An antenna's name for a point, or the point.
        let place = |p: &str| {
            self.devices.iter().find(|d| d.kind == DeviceKind::Antenna && d.at.as_deref() == Some(p)).map_or_else(|| format!("'{p}'"), |d| d.id.clone())
        };
        let line = |loc: &str| loc.rsplit("line ").next().unwrap_or(loc).to_string();
        for st in &self.steps {
            match &st.action {
                Action::Move { carrier, to, .. } => {
                    let Some(ci) = self.carrier_idx(carrier).filter(|&ci| derived[ci]) else { continue };
                    let cur = routes[ci].last().unwrap().clone();
                    if *to == cur {
                        continue;
                    }
                    let (w, l) = graph.way(&cur, to).ok_or_else(|| {
                        format!("{}: move {carrier} to {}: '{carrier}' can't go there along the lanes from {}, where it is", st.loc, place(to), place(&cur))
                    })?;
                    routes[ci].extend(w.into_iter().skip(1));
                    lanes[ci] = l;
                    last[ci] = Some((line(&st.loc), format!("move {carrier} to {}", place(to))));
                }
                Action::Pass { device, divert, carrier: Some(c), .. } => {
                    let Some(ci) = self.carrier_idx(c).filter(|&ci| derived[ci]) else { continue };
                    let cmd = if *divert { "divert" } else { "pass" };
                    let at = self.devices.iter().find(|d| d.id == *device).and_then(|d| d.at.clone()).unwrap_or_default();
                    let cur = routes[ci].last().unwrap().clone();
                    if cur != at {
                        let was = match &last[ci] {
                            Some((l, what)) => format!("after line {l} ({what}) it goes to {}", place(&cur)),
                            None => format!("it is still where it starts, on {}", place(&cur)),
                        };
                        let here: Vec<&str> = (0..n).filter(|&j| j != ci && derived[j] && routes[j].last() == Some(&at)).map(|j| self.carriers[j].id.as_str()).collect();
                        let hint = match here.as_slice() {
                            [] => format!("send it there first (move {c} to {device}, or the pass/divert that brings it there)"),
                            [one] => format!("on {device} there is {one}: did you mean {cmd} {device} {one}?"),
                            many => format!("on {device} there are {}", many.join(", ")),
                        };
                        return Err(format!("{}: {cmd} {device} {c}: '{c}' is not on {device}: {was}; {hint}", st.loc));
                    }
                    // The lanes that turn off here.
                    let turns: Vec<&str> = self.lanes.iter().filter(|l| l.points.first() == Some(&at)).map(|l| l.id.as_str()).collect();
                    let ways: Vec<&(String, f64, String)> = graph.out(&at).iter().filter(|e| turns.contains(&e.2.as_str()) == *divert).collect();
                    let first = match ways.as_slice() {
                        [] if *divert => return Err(format!("{}: divert {device} {c}: no lane turns off there: pass {device} {c}", st.loc)),
                        [] => None,
                        [one] => Some(*one),
                        many => many.iter().find(|e| e.2 == lanes[ci]).copied().or(Some(many[0])),
                    };
                    last[ci] = Some((line(&st.loc), format!("{cmd} {device} {c}")));
                    let Some((mut p, _, l)) = first.cloned() else { continue };
                    lanes[ci] = l;
                    routes[ci].push(p.clone());
                    // Along the lanes to the next stop.
                    while !stops.contains(&p) {
                        let next = match graph.out(&p) {
                            [] => break,
                            [one] => one,
                            many => many.iter().find(|e| e.2 == lanes[ci]).ok_or_else(|| {
                                format!("{}: after {cmd} {device} {c} the lanes split at '{p}': send it on with a move", st.loc)
                            })?,
                        };
                        p = next.0.clone();
                        lanes[ci] = next.2.clone();
                        routes[ci].push(p.clone());
                    }
                }
                _ => {}
            }
        }
        Ok(routes)
    }

    pub fn actor_idx(&self, id: &str) -> Option<usize> {
        self.actors.iter().position(|a| a.id == id)
    }
    pub fn device_idx(&self, id: &str) -> Option<usize> {
        self.devices.iter().position(|d| d.id == id)
    }
    pub fn carrier_idx(&self, id: &str) -> Option<usize> {
        self.carriers.iter().position(|c| c.id == id)
    }
    fn need_actor(&self, id: &str, loc: &str) -> Result<usize, String> {
        self.actor_idx(id).ok_or_else(|| format!("{loc}: unknown actor '{id}'"))
    }
    /// The points of the Main-Lane (main lanes, with the points on their segments).
    fn main_points(&self) -> Vec<String> {
        lane_paths(&self.points, &self.lanes).into_iter().zip(&self.lanes).filter(|(_, l)| l.kind == LaneKind::Main).flat_map(|((_, p), _)| p).collect()
    }

    /// Where the route of a carrier leaves the Main-Lane (index of its last point
    /// there), and where it is back on it, if it gets back.
    fn off_main(&self, ci: usize, main: &[String]) -> Option<(usize, Option<usize>)> {
        let r = &self.carriers[ci].route;
        let i = (0..r.len().saturating_sub(1)).find(|&i| main.contains(&r[i]) && !main.contains(&r[i + 1]))?;
        Some((i, (i + 1..r.len()).find(|&j| main.contains(&r[j]))))
    }

    /// The first carrier that would leave the Main-Lane into a lane with a
    /// capacity already full.
    fn overflow(&self, tl: &Timeline) -> Option<usize> {
        let main = self.main_points();
        let paths = lane_paths(&self.points, &self.lanes);
        let antennas: Vec<String> = self.devices.iter().filter(|d| d.kind == DeviceKind::Antenna).filter_map(|d| d.at.clone()).collect();
        for (li, lane) in self.lanes.iter().enumerate() {
            let Some(cap) = lane.capacity else { continue };
            let pts = &paths[li].1;
            let mut users: Vec<(usize, f64, f64)> = Vec::new();
            for (ci, c) in self.carriers.iter().enumerate() {
                if c.ghost || self.straight.contains(&c.id) || !c.route.iter().any(|p| pts.contains(p) && !main.contains(p)) {
                    continue;
                }
                let Some((i, j)) = self.off_main(ci, &main) else { continue };
                let path = &tl.carriers[ci];
                let Some(t_in) = path.reaches(path.cum[i]) else { continue };
                // It stops counting once past the first antenna after the diversion
                // (where the gate holds it), or when back on the Main-Lane.
                let route = &self.carriers[ci].route;
                let end = (i + 1..route.len()).find(|&k| antennas.contains(&route[k])).or(j);
                let t_out = end.and_then(|k| path.reaches(path.cum[k] + 1.0)).unwrap_or(f64::INFINITY);
                users.push((ci, t_in, t_out));
            }
            users.sort_by(|a, b| a.1.total_cmp(&b.1));
            for (k, &(ci, t_in, _)) in users.iter().enumerate() {
                if users[..k].iter().filter(|u| u.2 > t_in + 1e-9).count() >= cap {
                    return Some(ci);
                }
            }
        }
        None
    }

    /// The scene with carrier ci going straight on along the Main-Lane where it
    /// would leave it. Its steps after the diversion are left out: its moves past
    /// that point (the first one goes to the end of the new route instead), its
    /// messages, the pipettes of its sample, the steps that start when it reaches
    /// the diversion or a point it no longer passes, and, in turn, the steps that
    /// start from those.
    fn with_straight(&self, ci: usize) -> Result<Scene, String> {
        let mut sc = self.clone();
        let main = sc.main_points();
        let (i, _) = sc.off_main(ci, &main).ok_or("a carrier sent straight on must leave the Main-Lane")?;
        let old = sc.carriers[ci].route.clone();
        // Back on the Main-Lane where its route rejoins it; a route that ends off
        // the Main-Lane (written from the steps) goes on to the end of it.
        let end = old[i + 1..].iter().rev().find(|p| main.contains(p)).or(main.last()).cloned().unwrap_or_else(|| old[i].clone());
        let mut written: Vec<String> = old[..=i].to_vec();
        written.push(end);
        let route = expand_route(&written, &sc.points, &sc.lanes, &sc.devices)?;
        let (cid, sid) = (sc.carriers[ci].id.clone(), sc.carriers[ci].sample_id.clone());
        sc.carriers[ci].route = route.clone();
        sc.straight.push(cid.clone());
        let after_diversion = |p: &str| old.iter().position(|q| q == p).is_some_and(|k| k >= i) || !route.iter().any(|q| q == p);
        let mut dropped = vec![false; sc.steps.len()];
        let mut gone = false;
        for (k, st) in sc.steps.iter_mut().enumerate() {
            match &mut st.action {
                Action::Move { carrier, to, .. } if *carrier == cid => {
                    if gone {
                        dropped[k] = true;
                    } else if !route.contains(to) {
                        *to = route.last().unwrap().clone();
                        gone = true;
                    }
                }
                Action::Message { carrier: Some(c), .. } | Action::Pass { carrier: Some(c), .. } if *c == cid => dropped[k] = true,
                Action::Pipette { sample: Some(s), .. } if !sid.is_empty() && *s == sid => dropped[k] = true,
                _ => {}
            }
            if let StartMode::When(Anchor::Arrival { carrier, point }) = &st.start {
                if *carrier == cid && after_diversion(point) {
                    dropped[k] = true;
                }
            }
        }
        // What each step makes happen, for the steps that start from it.
        let keys = |st: &Step| -> Vec<String> {
            let mut v: Vec<String> = st.id.iter().map(|id| format!("step {id}")).collect();
            v.extend(match &st.action {
                Action::Message { kind: Some(k), carrier, .. } => {
                    let mut v = vec![k.clone()];
                    v.extend(carrier.iter().map(|c| format!("{k} {c}")));
                    v
                }
                Action::Pipette { device, .. } => vec![format!("pipette {device}")],
                Action::Rotate { device } => vec![format!("rotate {device}")],
                Action::Needle { device, .. } => vec![format!("needle {device}")],
                _ => Vec::new(),
            });
            v
        };
        loop {
            let mut more = false;
            for k in 0..sc.steps.len() {
                if dropped[k] {
                    continue;
                }
                if let StartMode::When(Anchor::Event { key }) = &sc.steps[k].start {
                    let lost = sc.steps[..k].iter().enumerate().rev().find(|(_, s)| keys(s).contains(key)).is_some_and(|(j, _)| dropped[j]);
                    if lost {
                        dropped[k] = true;
                        more = true;
                    }
                }
            }
            if !more {
                break;
            }
        }
        let mut k = 0;
        sc.steps.retain(|_| {
            k += 1;
            !dropped[k - 1]
        });
        Ok(sc)
    }

    /// Points where the belt can't carry a carrier further: antennas with a gate,
    /// turnstile entries.
    fn stops(&self) -> Vec<String> {
        let mut stops = Vec::new();
        for d in &self.devices {
            match d.kind {
                DeviceKind::Antenna if self.devices.iter().any(|g| g.id == format!("{}.gate", d.id)) => stops.extend(d.at.clone()),
                DeviceKind::Turnstile => stops.push(format!("{}.in", d.id)),
                _ => {}
            }
        }
        stops
    }

    /// The move a `pass` (or `divert`) stands for: the carrier stopped on the point
    /// of the device at time t sets off to the next stop of its route, or to its
    /// end. Its route says where it goes; `divert` must be on a divert where the
    /// route turns into a lane, `pass` where it goes straight on.
    #[allow(clippy::too_many_arguments)]
    fn pass_move(&self, device: &str, divert: bool, who: Option<&str>, speed: Option<f64>, paths: &[CarrierPath], cur_idx: &[usize], attached: &[Option<usize>], t: f64, loc: &str) -> Result<Action, String> {
        let cmd = if divert { "divert" } else { "pass" };
        // Only antennas: the antenna of a device is named instead of the device.
        let d = self.devices.iter().find(|d| d.id == device && d.kind == DeviceKind::Antenna).ok_or_else(|| {
            match self.devices.iter().find(|a| a.kind == DeviceKind::Antenna && a.part_of.as_deref() == Some(device)) {
                Some(a) => format!("{loc}: '{cmd} {device}': write the antenna, {cmd} {}", a.id),
                None => format!("{loc}: '{cmd} {device}': not an antenna of this scene"),
            }
        })?;
        let at = d.at.as_deref().unwrap();
        let p = self.pt(at);
        let stopped_here = |ci: usize| {
            let c = &paths[ci];
            let q = c.pos(t);
            attached[ci].is_none() && !c.moving(t) && ((q.0 - p.0).powi(2) + (q.1 - p.1).powi(2)).sqrt() < 3.0
        };
        // The carrier stopped there (a ghost too, when it is the one written).
        let real = (0..self.carriers.len()).find(|&ci| stopped_here(ci) && !paths[ci].ghost && paths[ci].opacity(t) > 0.0);
        let ci = match who.and_then(|w| self.carrier_idx(w)) {
            Some(wi) if self.carriers[wi].ghost && stopped_here(wi) => wi,
            Some(wi) => match real {
                Some(ci) if ci == wi => ci,
                Some(ci) => return Err(format!("{loc}: {cmd} {device} {}: the carrier stopped there at t={t:.2}s is '{}', not '{}'", self.carriers[wi].id, self.carriers[ci].id, self.carriers[wi].id)),
                None => return Err(format!("{loc}: {cmd} {device} {}: no carrier is stopped on '{at}' at t={t:.2}s", self.carriers[wi].id)),
            },
            None => real.ok_or_else(|| format!("{loc}: {cmd} {device}: no carrier is stopped on '{at}' at t={t:.2}s"))?,
        };
        let c = &self.carriers[ci];
        let from = cur_idx[ci];
        // A divert (or its antenna) can turn the carrier into a lane that starts
        // there: the next point of its route is the first one of that lane.
        let owner = d.part_of.as_deref().and_then(|o| self.devices.iter().find(|x| x.id == o)).unwrap_or(d);
        let turn = (owner.kind == DeviceKind::Divert)
            .then(|| c.route.get(from + 1))
            .flatten()
            .and_then(|next| {
                lane_paths(&self.points, &self.lanes)
                    .into_iter()
                    .zip(&self.lanes)
                    .find(|((_, path), l)| l.points.first().map(String::as_str) == Some(at) && path.get(1) == Some(next))
                    .map(|((id, _), _)| id)
            });
        match (divert, &turn) {
            (true, _) if owner.kind != DeviceKind::Divert => {
                return Err(format!("{loc}: divert {device}: it is not on a divert, the carrier can only go on: pass {device}"));
            }
            (true, None) => {
                return Err(format!("{loc}: divert {device}: the route of '{}' goes straight on at '{at}': write pass {device}, or give it a route into the lane", c.id));
            }
            (false, Some(lane)) => {
                return Err(format!("{loc}: pass {device}: the route of '{}' turns into '{lane}' at '{at}': write divert {device}", c.id));
            }
            _ => {}
        }
        let stops = self.stops();
        let to = c.route[from + 1..]
            .iter()
            .find(|p| stops.contains(p))
            .or(c.route.last())
            .filter(|_| from + 1 < c.route.len())
            .ok_or_else(|| format!("{loc}: {cmd} {device}: carrier '{}' is at the end of its route", c.id))?;
        Ok(Action::Move { carrier: c.id.clone(), to: to.clone(), ease: Ease::Linear, speed, runaway: false })
    }

    fn need_carrier(&self, id: &str, loc: &str) -> Result<usize, String> {
        self.carrier_idx(id).ok_or_else(|| format!("{loc}: unknown carrier '{id}'"))
    }
    /// Device `id` for `action`; with `kinds` non-empty it must be one of them.
    fn need_device(&self, id: &str, loc: &str, action: &str, kinds: &[(DeviceKind, &str)]) -> Result<usize, String> {
        let di = self.device_idx(id).ok_or_else(|| format!("{loc}: unknown device '{id}'"))?;
        if !kinds.is_empty() && !kinds.iter().any(|k| k.0 == self.devices[di].kind) {
            let names: Vec<String> = kinds.iter().map(|k| format!("'{}'", k.1)).collect();
            return Err(format!("{loc}: {action} needs a device of kind {}", names.join(" or ")));
        }
        Ok(di)
    }
    /// Sampling position of the analyzer needle (pipette, needle): a sampling
    /// point or a turnstile, and the scene must have an analyzer.
    fn need_probe(&self, id: &str, loc: &str, action: &str) -> Result<usize, String> {
        let di = self.need_device(id, loc, action, &[(DeviceKind::Sampling, "sampling"), (DeviceKind::Turnstile, "turnstile")])?;
        if !self.devices.iter().any(|d| d.kind == DeviceKind::Analyzer) {
            return Err(format!("{loc}: {action} needs an 'analyzer' device"));
        }
        Ok(di)
    }
    /// Note shown while pipetting at device `di`, on the analyzer's actor. With
    /// several sampling points it names the needle (needle 1 = first in the file).
    /// The note of a `pipette` on the analyzer lifeline: the text written in the
    /// diagram, or "Pipetting <analyzer>[ · Needle n][ - SampleId=...]" with the
    /// SampleId given to the pipette.
    fn pipetting_note(&self, di: usize, written: Option<&str>, sample: Option<&str>) -> Option<(usize, String)> {
        let ai = self.actors.iter().position(|a| a.analyzer)?;
        if let Some(w) = written {
            return Some((ai, w.to_string()));
        }
        let needles: Vec<usize> = (0..self.devices.len()).filter(|&i| self.devices[i].kind == DeviceKind::Sampling).collect();
        let mut text = format!("Pipetting {}", self.actors[ai].label);
        if let Some(n) = needles.iter().position(|&i| i == di).filter(|_| needles.len() > 1) {
            text.push_str(&format!(" · Needle {}", n + 1));
        }
        if let Some(s) = sample {
            text.push_str(&format!(" - SampleId={s}"));
        }
        Some((ai, text))
    }
    /// The analyzer of the scene (one instrument, possibly with several needles).
    pub fn analyzer(&self) -> Option<usize> {
        self.devices.iter().position(|d| d.kind == DeviceKind::Analyzer)
    }
    pub fn pt(&self, id: &str) -> (f64, f64) {
        sc_pt(&self.points, id)
    }
    /// Direction of travel of the lane through point p.
    pub fn lane_dir(&self, p: (f64, f64)) -> (f64, f64) {
        lane_dir(&self.points, &self.lanes, p)
    }
}

// ---------------------------------------------------------------- timeline

#[derive(Clone)]
pub struct MoveSeg {
    pub t0: f64,
    pub t1: f64,
    pub s0: f64,
    pub s1: f64,
    pub ease: Ease,
    pub runaway: bool,
}

pub struct CarrierPath {
    pub pts: Vec<(f64, f64)>,
    pub cum: Vec<f64>,
    pub start_s: f64,
    pub moves: Vec<MoveSeg>,
    /// Intervals in which the carrier sits in a turnstile pocket (its position
    /// then follows the wheel instead of the route).
    pub attach: Vec<Attach>,
    pub ghost: bool,
    pub visible: bool,
    /// Fades from `show`/`hide` steps: (t0, t1, visible afterwards), in time order.
    pub fades: Vec<(f64, f64, bool)>,
}

pub struct Attach {
    pub wheel: usize,
    pub pocket: usize,
    /// When the move into the entry position starts (the carrier passes the gate).
    pub t_move: f64,
    pub t0: f64,
    /// When the carrier leaves the exit position (infinity if it never does).
    pub t1: f64,
}

/// Runtime state of a turnstile device.
pub struct Wheel {
    pub device: usize,
    pub cx: f64,
    pub cy: f64,
    pub r: f64,
    pub entry_right: bool,
    /// Rotations as (t0, t1); each one advances the pockets by one slot.
    pub rots: Vec<(f64, f64)>,
}

impl Wheel {
    /// Rotations done at time t (fractional while turning).
    pub fn turns(&self, t: f64) -> f64 {
        self.rots
            .iter()
            .map(|&(t0, t1)| if t >= t1 { 1.0 } else if t <= t0 { 0.0 } else { Ease::InOut.apply((t - t0) / (t1 - t0)) })
            .sum()
    }
    /// Angle of pocket p at time t (pocket p starts in slot p).
    pub fn pocket_angle(&self, p: usize, t: f64) -> f64 {
        turnstile_slot_angle(self.entry_right, p as f64 + self.turns(t))
    }
    pub fn pocket_pos(&self, p: usize, t: f64) -> (f64, f64) {
        let a = self.pocket_angle(p, t);
        (self.cx + self.r * a.cos(), self.cy + self.r * a.sin())
    }
}

/// Something that happens to a turnstile, checked in time order once all the
/// steps are laid out (steps are written in file order, which is not always
/// the time order).
enum WheelEv {
    /// A carrier reaches the entry: (carrier, index in its attach list).
    Arrive(usize, usize),
    /// A carrier leaves from the exit.
    Leave(usize, usize),
    /// A rotation starts; it ends at the given time.
    Rotate(f64),
}

struct WheelRec {
    t: f64,
    wheel: usize,
    ev: WheelEv,
    loc: String,
}

impl WheelRec {
    /// At the same instant: leave, then arrive, then rotate.
    fn order(&self) -> u8 {
        match self.ev {
            WheelEv::Leave(..) => 0,
            WheelEv::Arrive(..) => 1,
            WheelEv::Rotate(_) => 2,
        }
    }
}

/// Plays the turnstile events in time order: assigns pockets to arriving
/// carriers and checks that the wheel is used consistently.
fn simulate_wheels(sc: &Scene, wheels: &[Wheel], carriers: &mut [CarrierPath], mut recs: Vec<WheelRec>) -> Result<(), String> {
    recs.sort_by(|a, b| a.t.total_cmp(&b.t).then(a.order().cmp(&b.order())));
    for (wi, w) in wheels.iter().enumerate() {
        let id = &sc.devices[w.device].id;
        let mut k = 0usize; // rotations started so far
        let mut busy_until = f64::NEG_INFINITY;
        let mut pockets: [Option<usize>; 3] = [None; 3];
        let slot = |p: usize, k: usize| (p + k) % 3;
        for r in recs.iter().filter(|r| r.wheel == wi) {
            let t = r.t;
            match r.ev {
                WheelEv::Arrive(ci, ai) => {
                    let cid = &sc.carriers[ci].id;
                    if t < busy_until - 1e-9 {
                        return Err(format!("{}: turnstile '{id}' is rotating when carrier '{cid}' reaches the entry (t={t:.2}s)", r.loc));
                    }
                    let p = (3 - k % 3) % 3;
                    if let Some(o) = pockets[p] {
                        return Err(format!("{}: the entry of turnstile '{id}' is still occupied by carrier '{}' when carrier '{cid}' arrives (t={t:.2}s)", r.loc, sc.carriers[o].id));
                    }
                    pockets[p] = Some(ci);
                    carriers[ci].attach[ai].pocket = p;
                }
                WheelEv::Leave(ci, ai) => {
                    let cid = &sc.carriers[ci].id;
                    if t < busy_until - 1e-9 {
                        return Err(format!("{}: turnstile '{id}' is still rotating when carrier '{cid}' should leave (t={t:.2}s)", r.loc));
                    }
                    let p = carriers[ci].attach[ai].pocket;
                    if slot(p, k) != 2 {
                        return Err(format!("{}: carrier '{cid}' is in turnstile '{id}' but not in the exit position at t={t:.2}s; rotate it first", r.loc));
                    }
                    pockets[p] = None;
                }
                WheelEv::Rotate(t_end) => {
                    if t < busy_until - 1e-9 {
                        return Err(format!("{}: turnstile '{id}' is still rotating at t={t:.2}s", r.loc));
                    }
                    if let Some(ci) = (0..3).find(|&p| slot(p, k) == 2).and_then(|p| pockets[p]) {
                        return Err(format!("{}: carrier '{}' is still in the exit position of '{id}' at t={t:.2}s; move it out before rotating", r.loc, sc.carriers[ci].id));
                    }
                    k += 1;
                    busy_until = t_end;
                }
            }
        }
    }
    Ok(())
}

/// The first instant from `t` at which wheel `wi` can make a rotation of `d`
/// seconds: its previous rotation is over, no carrier is on its way into the
/// entry pocket, and every carrier outside the pockets stays clear of the wheel
/// for the whole rotation. Only the moves laid out so far are known; if the
/// wheel never gets free, `t` (the final checks then say why).
fn wheel_free_at(w: &Wheel, wi: usize, carriers: &[CarrierPath], t: f64, d: f64) -> f64 {
    const STEP: f64 = 0.02;
    let clear = w.r + TS_RIM + CARRIER_R;
    let mut s = w.rots.last().map_or(t, |r| t.max(r.1));
    let limit = s + 120.0;
    'search: while s < limit {
        for c in carriers {
            if let Some(a) = c.attach.iter().find(|a| a.wheel == wi && s >= a.t_move - 1e-9 && s < a.t0 - 1e-9) {
                s = a.t0;
                continue 'search;
            }
        }
        let n = (d / STEP).ceil().max(1.0) as usize;
        for k in 0..=n {
            let u = s + d * k as f64 / n as f64;
            for c in carriers {
                let on_wheel = c.attach.iter().any(|a| u >= a.t0 && u <= a.t1);
                if on_wheel || c.ghost || c.opacity(u) <= 0.0 {
                    continue;
                }
                let (x, y) = c.pos(u);
                if ((x - w.cx).powi(2) + (y - w.cy).powi(2)).sqrt() < clear - 1e-6 {
                    s = u + STEP;
                    continue 'search;
                }
            }
        }
        return s;
    }
    t
}

impl CarrierPath {
    pub fn s_at(&self, t: f64) -> f64 {
        let mut s = self.start_s;
        for m in &self.moves {
            if t < m.t0 {
                break;
            }
            if t >= m.t1 {
                s = m.s1;
            } else {
                let u = (t - m.t0) / (m.t1 - m.t0);
                s = m.s0 + (m.s1 - m.s0) * m.ease.apply(u);
                break;
            }
        }
        s
    }
    pub fn pos_at_s(&self, s: f64) -> (f64, f64) {
        for i in 1..self.pts.len() {
            if s <= self.cum[i] || i == self.pts.len() - 1 {
                let seg = self.cum[i] - self.cum[i - 1];
                let u = if seg > 0.0 { ((s - self.cum[i - 1]) / seg).clamp(0.0, 1.0) } else { 0.0 };
                let (a, b) = (self.pts[i - 1], self.pts[i]);
                return (a.0 + (b.0 - a.0) * u, a.1 + (b.1 - a.1) * u);
            }
        }
        self.pts[0]
    }
    pub fn pos(&self, t: f64) -> (f64, f64) {
        self.pos_at_s(self.s_at(t))
    }
    pub fn moving(&self, t: f64) -> bool {
        self.moves.iter().any(|m| t > m.t0 && t < m.t1 && m.s1 > m.s0 + 1e-9)
    }
    /// Running away: from the first `move` with `runaway = true` to the end of
    /// the last one, including a stop in between.
    pub fn running_away(&self, t: f64) -> bool {
        let first = self.moves.iter().find(|m| m.runaway);
        let last = self.moves.iter().rev().find(|m| m.runaway);
        first.zip(last).is_some_and(|(a, b)| t >= a.t0 && t <= b.t1)
    }
    /// 0 = hidden, 1 = fully visible (fading in between).
    pub fn opacity(&self, t: f64) -> f64 {
        let mut vis = if self.visible { 1.0 } else { 0.0 };
        for &(t0, t1, show) in &self.fades {
            if t < t0 {
                break;
            }
            let target = if show { 1.0 } else { 0.0 };
            if t >= t1 {
                vis = target;
            } else {
                let u = Ease::InOut.apply((t - t0) / (t1 - t0).max(1e-6));
                vis += (target - vis) * u;
                break;
            }
        }
        vis
    }
    /// Physically present (not a ghost) and visible: seen by antennas, opens gates.
    pub fn physical(&self, t: f64) -> bool {
        !self.ghost && self.opacity(t) > 0.0
    }
    /// Distance along the route of point p, if p lies on the route (within 3 px).
    fn project(&self, p: (f64, f64)) -> Option<f64> {
        for i in 1..self.pts.len() {
            let (a, b) = (self.pts[i - 1], self.pts[i]);
            let len2 = (b.0 - a.0).powi(2) + (b.1 - a.1).powi(2);
            if len2 == 0.0 {
                continue;
            }
            let u = (((p.0 - a.0) * (b.0 - a.0) + (p.1 - a.1) * (b.1 - a.1)) / len2).clamp(0.0, 1.0);
            let (qx, qy) = (a.0 + (b.0 - a.0) * u, a.1 + (b.1 - a.1) * u);
            if ((p.0 - qx).powi(2) + (p.1 - qy).powi(2)).sqrt() < 3.0 {
                return Some(self.cum[i - 1] + u * len2.sqrt());
            }
        }
        None
    }
    /// When the carrier reaches distance s along its route, among the moves
    /// laid out so far.
    pub fn reaches(&self, s: f64) -> Option<f64> {
        let m = self.moves.iter().find(|m| m.s1 >= s - 1e-9)?;
        if s <= m.s0 {
            return Some(m.t0);
        }
        // Bisection: the position grows monotonically during a move.
        let (mut lo, mut hi) = (m.t0, m.t1);
        for _ in 0..50 {
            let t = (lo + hi) / 2.0;
            if self.s_at(t) < s {
                lo = t;
            } else {
                hi = t;
            }
        }
        Some(hi)
    }
}

pub enum Ev {
    Caption { title: String, text: String },
    /// `color`: overrides the sender's color (e.g. to tell apart a repeated message).
    /// `row`: where its arrow is in the sequence diagram, in half rows (a label of
    /// one line takes a row; each more line half a row, above it).
    Message { from: usize, to: usize, text: String, params: String, reply: bool, row: usize, color: Option<String> },
    Note { actor: usize, text: String, row: usize },
    Pipette { device: usize },
    Highlight { device: usize },
    Status { device: usize, state: AnalyzerState },
    /// Needle of the analyzer at a sampling/turnstile device going down or up.
    Needle { device: usize, down: bool },
    Operator { device: usize },
    Wait,
}

/// The lines of a label of the sequence diagram: as in Mermaid, `<br/>` (or `<br>`,
/// `<br />`) starts a new line. Each line is (text, parameters): the parameters
/// follow the text, on its last line, and may go on over more lines.
pub fn label_lines(text: &str, params: &str) -> Vec<(String, String)> {
    fn split(s: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = s;
        loop {
            let lower = rest.to_ascii_lowercase();
            let Some(i) = ["<br/>", "<br />", "<br>"].iter().filter_map(|b| lower.find(b).map(|i| (i, b.len()))).min() else {
                out.push(rest.trim().to_string());
                return out;
            };
            out.push(rest[..i.0].trim().to_string());
            rest = &rest[i.0 + i.1..];
        }
    }
    let mut lines: Vec<(String, String)> = split(text).into_iter().map(|l| (l, String::new())).collect();
    if !params.is_empty() {
        let mut p = split(params).into_iter();
        lines.last_mut().unwrap().1 = p.next().unwrap_or_default();
        lines.extend(p.map(|l| (String::new(), l)));
    }
    lines
}

pub struct Timed {
    pub t0: f64,
    pub t1: f64,
    pub ev: Ev,
    /// Where the step it comes from is written (file, line), for warnings.
    pub at: Option<(String, usize)>,
}

pub struct Timeline {
    pub events: Vec<Timed>,
    pub carriers: Vec<CarrierPath>,
    pub wheels: Vec<Wheel>,
    /// Gate devices: (device index, intervals in which a carrier drives through).
    pub gates: Vec<(usize, Vec<(f64, f64)>)>,
    /// The height of the sequence diagram, in half rows (see `row` in `Ev`).
    pub rows: usize,
    pub duration: f64,
    pub log: Vec<String>,
}

impl Timeline {
    pub fn carrier_pos(&self, ci: usize, t: f64) -> (f64, f64) {
        let c = &self.carriers[ci];
        for a in &c.attach {
            if t >= a.t0 && t < a.t1 {
                return self.wheels[a.wheel].pocket_pos(a.pocket, t);
            }
        }
        c.pos(t)
    }
    pub fn wheel_of_device(&self, di: usize) -> Option<&Wheel> {
        self.wheels.iter().find(|w| w.device == di)
    }
}

fn subst(s: &str, sc: &Scene, carrier: Option<usize>, needle: Option<u32>) -> Result<String, String> {
    let mut s = s.to_string();
    if let Some(ci) = carrier.or(if sc.carriers.is_empty() { None } else { Some(0) }) {
        s = s.replace("{SampleId}", &sc.carriers[ci].sample_id).replace("{CarrierId}", &sc.carriers[ci].id);
    }
    match needle {
        Some(n) => Ok(s.replace("{Needle}", &n.to_string())),
        None if s.contains("{Needle}") => Err("the message uses {Needle}: add a 'needle' key to the step".into()),
        None => Ok(s),
    }
}

impl Timeline {
    /// Lays out the steps in time. On error, the message also lists the
    /// timeline computed up to that point (useful to fix delays in a scene).
    pub fn build(sc: &Scene) -> Result<Timeline, String> {
        // A carrier that would find its pit-lane full goes straight on: lay out
        // again without it in the pit-lane, until every lane is within its capacity.
        let mut changed: Option<Scene> = None;
        loop {
            let cur = changed.as_ref().unwrap_or(sc);
            let mut tl = Self::build_settled(cur)?;
            match cur.overflow(&tl) {
                None => {
                    for (k, c) in cur.straight.iter().enumerate() {
                        tl.log.insert(k, format!("{c} goes straight on: its pit-lane is full (its steps after the diversion are skipped)"));
                    }
                    return Ok(tl);
                }
                Some(ci) => changed = Some(cur.with_straight(ci)?),
            }
        }
    }

    fn build_settled(sc: &Scene) -> Result<Timeline, String> {
        // The moves depend on the steps (when they start) and the steps on the
        // moves (when a carrier arrives): lay out, play the moves together, and
        // again until nothing changes.
        let mut actual: HashMap<usize, physics::Done> = HashMap::new();
        for _ in 0..40 {
            let mut log = Vec::new();
            let (tl, plans) = Self::build_inner(sc, &mut log, &actual, false).map_err(|e| with_log(e, &log))?;
            // A ghost stands for the real carrier with its sample.
            let twin: Vec<Option<usize>> = sc
                .carriers
                .iter()
                .map(|c| c.ghost.then(|| sc.carriers.iter().position(|o| !o.ghost && !c.sample_id.is_empty() && o.sample_id == c.sample_id)).flatten())
                .collect();
            let next = physics::simulate(&tl.carriers, &plans, &twin);
            let settled = next.len() == actual.len() && next.iter().all(|(k, d)| actual.get(k).is_some_and(|a| a.same(d)));
            actual = next;
            if settled {
                let mut log = Vec::new();
                return Self::build_inner(sc, &mut log, &actual, true).map(|r| r.0).map_err(|e| with_log(e, &log));
            }
        }
        Err("the carriers keep pushing each other: the timing never settles (check the moves that start when another carrier arrives)".into())
    }

    /// One layout of the steps. `actual`: the moves as they really happen (from
    /// the physics), by step; a move not in it is laid out without obstacles.
    /// `check`: the final layout, with the checks on wheels, needles and gates.
    fn build_inner(sc: &Scene, log: &mut Vec<String>, actual: &HashMap<usize, physics::Done>, check: bool) -> Result<(Timeline, Vec<physics::Plan>), String> {
        let mut carriers = Vec::new();
        let mut cur_idx = Vec::new();
        // A carrier whose last move ended in a queue: (the carrier in front, line).
        let mut stuck: Vec<Option<(usize, String)>> = vec![None; sc.carriers.len()];
        for c in &sc.carriers {
            let pts: Vec<(f64, f64)> = c.route.iter().map(|p| sc.pt(p)).collect();
            let mut cum = vec![0.0];
            for i in 1..pts.len() {
                let d = ((pts[i].0 - pts[i - 1].0).powi(2) + (pts[i].1 - pts[i - 1].1).powi(2)).sqrt();
                cum.push(cum[i - 1] + d);
            }
            let si = c.route.iter().position(|p| *p == c.start).unwrap();
            cur_idx.push(si);
            carriers.push(CarrierPath { start_s: cum[si], pts, cum, moves: Vec::new(), attach: Vec::new(), ghost: c.ghost, visible: c.visible, fades: Vec::new() });
        }

        // Turnstiles: map "<id>.in" / "<id>.out" points to (wheel, slot).
        let mut wheels = Vec::new();
        let mut wheel_pts: HashMap<String, (usize, usize)> = HashMap::new();
        for (di, d) in sc.devices.iter().enumerate().filter(|(_, d)| d.kind == DeviceKind::Turnstile) {
            let wi = wheels.len();
            wheel_pts.insert(format!("{}.in", d.id), (wi, 0));
            wheel_pts.insert(format!("{}.needle", d.id), (wi, 1));
            wheel_pts.insert(format!("{}.out", d.id), (wi, 2));
            wheels.push(Wheel { device: di, cx: d.rect.0, cy: d.rect.1, r: d.rect.2, entry_right: d.entry_right, rots: Vec::new() });
        }
        // Routes cross a turnstile as "<id>.in", "<id>.out" (the wheel carries
        // the carrier in between); the needle point is not part of a route.
        for c in &sc.carriers {
            for (i, p) in c.route.iter().enumerate() {
                if let Some(&(wi, slot)) = wheel_pts.get(p) {
                    let id = &sc.devices[wheels[wi].device].id;
                    let ok = match slot {
                        0 => c.route.get(i + 1) == Some(&format!("{id}.out")),
                        2 => i > 0 && c.route[i - 1] == format!("{id}.in"),
                        _ => false,
                    };
                    if !ok {
                        return Err(format!("carrier '{}': in the route a turnstile is written as \"{id}.in\", \"{id}.out\" (consecutive)", c.id));
                    }
                }
            }
            if wheel_pts.contains_key(&c.start) {
                return Err(format!("carrier '{}': it cannot start inside a turnstile", c.id));
            }
        }
        let mut attached: Vec<Option<usize>> = vec![None; sc.carriers.len()];
        let mut wheel_recs: Vec<WheelRec> = Vec::new();

        let mut events = Vec::new();
        let (mut prev_end, mut max_end) = (0.0f64, 0.0f64);
        let mut first = true;
        // What a `when` can refer to: (key, start, end) of the steps so far.
        let mut history: Vec<(String, f64, f64)> = Vec::new();
        let mut plans: Vec<physics::Plan> = Vec::new();

        for (si, st) in sc.steps.iter().enumerate() {
            let base = match &st.start {
                StartMode::At(at) => *at,
                StartMode::When(Anchor::Event { key }) => {
                    history.iter().rev().find(|h| h.0 == *key).map(|h| h.2).ok_or_else(|| {
                        format!("{}: no '{key}' before this step: 'after <seconds> {key}' refers to one written earlier in the file", st.loc)
                    })?
                }
                StartMode::When(Anchor::Arrival { carrier, point }) => {
                    let ci = sc.need_carrier(carrier, &st.loc)?;
                    // A carrier can go round more than once (the return belt of spur2):
                    // the last arrival there among the moves written so far.
                    let route = &sc.carriers[ci].route;
                    let i = route.iter().take(cur_idx[ci] + 1).rposition(|p| p == point)
                        .or_else(|| route.iter().position(|p| p == point))
                        .ok_or_else(|| format!("{}: point '{point}' is not on the route of carrier '{carrier}'", st.loc))?;
                    let path = &carriers[ci];
                    path.reaches(path.cum[i]).ok_or_else(|| {
                        let name = |p: &str| {
                            sc.devices.iter().find(|d| d.kind == DeviceKind::Antenna && d.at.as_deref() == Some(p)).map_or_else(|| format!("'{p}'"), |d| d.id.clone())
                        };
                        match &stuck[ci] {
                            Some((b, line)) if i <= cur_idx[ci] => {
                                let who = &sc.carriers[*b].id;
                                let there = name(&sc.carriers[*b].route[cur_idx[*b]]);
                                format!(
                                    "{}: '{carrier}' never gets to {}: after {line} it stops behind '{who}', which stays on {there} because no step before this one sends it on (e.g. pass {there} {who})",
                                    st.loc,
                                    name(point)
                                )
                            }
                            _ => format!("{}: no earlier step moves carrier '{carrier}' to '{point}' (the move must come before this step in the file)", st.loc),
                        }
                    })?
                }
                _ if first => 0.0,
                StartMode::After => prev_end,
            };
            first = false;
            let mut t0 = base + st.delay;
            // `pass`: a move of the carrier stopped on the device, to its next stop.
            let passed;
            let action = match &st.action {
                Action::Pass { device, divert, carrier, speed } => match sc.pass_move(device, *divert, carrier.as_deref(), *speed, &carriers, &cur_idx, &attached, t0, &st.loc) {
                    Ok(a) => {
                        passed = a;
                        &passed
                    }
                    // The carrier it was for went straight on.
                    Err(_) if !sc.straight.is_empty() => {
                        passed = Action::Wait;
                        &passed
                    }
                    Err(e) => return Err(e),
                },
                a => a,
            };
            // When the carrier of a move actually sets off.
            let mut depart = None;
            let (dur, ev, desc) = match action {
                Action::Caption { title, text } => {
                    (st.duration.unwrap_or(0.0), Ev::Caption { title: title.clone(), text: text.clone() }, format!("caption   {title}"))
                }
                Action::Move { carrier, to, ease, speed, runaway } => {
                    let ci = sc.need_carrier(carrier, &st.loc)?;
                    let c = &sc.carriers[ci];
                    let mut from = cur_idx[ci];
                    let leaving = attached[ci];
                    if leaving.is_some() {
                        from += 1; // the route point after "<id>.in" is "<id>.out"
                    }
                    let ti = c.route.iter().enumerate().skip(from + 1).find(|(_, p)| *p == to).map(|(i, _)| i)
                        .ok_or_else(|| format!("{}: point '{to}' is not ahead of carrier '{carrier}' on its route", st.loc))?;
                    if let Some(p) = c.route[from..ti].iter().find(|p| wheel_pts.get(*p).is_some_and(|w| w.1 == 0)) {
                        return Err(format!("{}: carrier '{carrier}' would drive through the turnstile; move it to '{p}' and use 'rotate'", st.loc));
                    }
                    let path = &mut carriers[ci];
                    let (s0, s1) = (path.cum[from], path.cum[ti]);
                    let d = st.duration.unwrap_or((s1 - s0) / speed.unwrap_or(c.speed) * ease.factor());
                    let into = wheel_pts.get(to).filter(|w| w.1 == 0).map(|w| w.0);
                    plans.push(physics::Plan { step: si, carrier: ci, t_req: t0, s0, s1, dur: d, ease: *ease, runaway: *runaway, into_wheel: into.is_some() });
                    // A move asked while the carrier is still moving starts when it stops.
                    let free = path.moves.last().map_or(f64::NEG_INFINITY, |m| m.t1);
                    let done = actual.get(&si).cloned().unwrap_or_else(|| {
                        let start = t0.max(free);
                        physics::Done { start, end: Some(start + d), last_motion: start + d, segs: vec![MoveSeg { t0: start, t1: start + d, s0, s1, ease: *ease, runaway: *runaway }], waits: Vec::new() }
                    });
                    // Leaving a turnstile (from the exit position: checked by simulate_wheels).
                    if let Some(ai) = leaving {
                        let wheel = path.attach[ai].wheel;
                        wheel_recs.push(WheelRec { t: done.start, wheel, ev: WheelEv::Leave(ci, ai), loc: st.loc.clone() });
                        path.attach[ai].t1 = done.start;
                        attached[ci] = None;
                    }
                    depart = Some(done.segs.first().map_or(done.start, |s| s.t0));
                    path.moves.extend(done.segs.iter().cloned());
                    cur_idx[ci] = ti;
                    // Entering a turnstile: the carrier drops into the pocket at the entry.
                    // The pocket is assigned later, in time order (simulate_wheels).
                    if let (Some(wi), Some(t_in)) = (into, done.end) {
                        path.attach.push(Attach { wheel: wi, pocket: 0, t_move: done.start, t0: t_in, t1: f64::INFINITY });
                        let ai = path.attach.len() - 1;
                        attached[ci] = Some(ai);
                        wheel_recs.push(WheelRec { t: t_in, wheel: wi, ev: WheelEv::Arrive(ci, ai), loc: st.loc.clone() });
                    }
                    stuck[ci] = match (done.end, done.waits.first()) {
                        (None, Some(w)) => Some((w.2, format!("line {}", st.loc.rsplit("line ").next().unwrap_or(&st.loc)))),
                        _ => None,
                    };
                    let mut tag = if *runaway { "  (run-away)".to_string() } else { String::new() };
                    let waited: f64 = done.waits.iter().map(|w| w.1 - w.0).sum();
                    if let Some(w) = done.waits.first() {
                        let who = &sc.carriers[w.2].id;
                        tag += &match done.end {
                            Some(_) => format!("  (waits {waited:.1}s behind {who})"),
                            None => format!("  (stops behind {who} and never gets there)"),
                        };
                    }
                    (done.end.unwrap_or(done.last_motion) - t0, Ev::Wait, format!("move      {carrier} -> {to}{tag}"))
                }
                Action::Message { from, to, text, params, reply, carrier, color, needle, .. } => {
                    let (fi, ti) = (sc.need_actor(from, &st.loc)?, sc.need_actor(to, &st.loc)?);
                    if fi == ti {
                        return Err(format!("{}: message from an actor to itself is not supported; use a note", st.loc));
                    }
                    let ci = carrier.as_deref().map(|c| sc.need_carrier(c, &st.loc)).transpose()?;
                    let text = subst(text, sc, ci, *needle).map_err(|e| format!("{}: {e}", st.loc))?;
                    let params = subst(params, sc, ci, *needle).map_err(|e| format!("{}: {e}", st.loc))?;
                    let desc = format!("message   {from} -> {to}  {text}  {params}");
                    let ev = Ev::Message { from: fi, to: ti, text, params, reply: *reply, row: 0, color: color.clone() };
                    (st.duration.unwrap_or(sc.video.message), ev, desc)
                }
                Action::Note { actor, text } => {
                    let ai = sc.need_actor(actor, &st.loc)?;
                    let ev = Ev::Note { actor: ai, text: text.clone(), row: 0 };
                    (st.duration.unwrap_or(1.5), ev, format!("note      {actor}: {text}"))
                }
                Action::Pipette { device, .. } => {
                    let di = sc.need_probe(device, &st.loc, "pipette")?;
                    (st.duration.unwrap_or(3.0), Ev::Pipette { device: di }, format!("pipette   at {device}"))
                }
                Action::Highlight { device } => {
                    let di = sc.need_device(device, &st.loc, "highlight", &[])?;
                    (st.duration.unwrap_or(1.5), Ev::Highlight { device: di }, format!("highlight {device}"))
                }
                Action::Rotate { device } => {
                    let di = sc.need_device(device, &st.loc, "rotate", &[(DeviceKind::Turnstile, "turnstile")])?;
                    let wi = wheels.iter().position(|w| w.device == di).unwrap();
                    let d = st.duration.unwrap_or(2.0);
                    // The wheel waits until it is free: the previous rotation over, the
                    // carrier driving into the entry in its pocket, the others clear.
                    let asked = t0;
                    t0 = wheel_free_at(&wheels[wi], wi, &carriers, t0, d);
                    let tag = if t0 > asked + 1e-6 { format!("  (waits {:.1}s for the wheel to be clear)", t0 - asked) } else { String::new() };
                    wheels[wi].rots.push((t0, t0 + d));
                    wheel_recs.push(WheelRec { t: t0, wheel: wi, ev: WheelEv::Rotate(t0 + d), loc: st.loc.clone() });
                    (d, Ev::Wait, format!("rotate    {device}{tag}"))
                }
                Action::Visibility { carrier, show } => {
                    let ci = sc.need_carrier(carrier, &st.loc)?;
                    let d = st.duration.unwrap_or(0.5);
                    let fades = &mut carriers[ci].fades;
                    if let Some(last) = fades.last() {
                        if t0 < last.1 - 1e-9 {
                            return Err(format!("{}: carrier '{carrier}' is still fading at t={t0:.2}s (show/hide steps of a carrier must be in time order)", st.loc));
                        }
                    }
                    fades.push((t0, t0 + d, *show));
                    let verb = if *show { "show" } else { "hide" };
                    (d, Ev::Wait, format!("{verb:<9} {carrier}"))
                }
                Action::Status { device, state } => {
                    let di = sc.need_device(device, &st.loc, "status", &[(DeviceKind::Analyzer, "analyzer"), (DeviceKind::Zone, "zone"), (DeviceKind::Sampling, "sampling")])?;
                    (st.duration.unwrap_or(0.0), Ev::Status { device: di, state: *state }, format!("status    {device} {}", state.name()))
                }
                Action::Needle { device, down } => {
                    let di = sc.need_probe(device, &st.loc, "needle")?;
                    let dir = if *down { "down" } else { "up" };
                    (st.duration.unwrap_or(0.9), Ev::Needle { device: di, down: *down }, format!("needle    {device} {dir}"))
                }
                Action::Operator { device } => {
                    let di = sc.need_device(device, &st.loc, "operator", &[(DeviceKind::Analyzer, "analyzer")])?;
                    let d = st.duration.unwrap_or(7.0);
                    if d < 2.0 {
                        return Err(format!("{}: operator needs a duration of at least 2 s (walk in and out)", st.loc));
                    }
                    (d, Ev::Operator { device: di }, format!("operator  at {device}"))
                }
                Action::Wait => (st.duration.unwrap_or(0.0), Ev::Wait, "wait".to_string()),
                Action::Pass { .. } => unreachable!("a pass is laid out as a move"),
            };
            let t1 = t0 + dur;
            // For the steps after it, a pass (or divert) ends when the carrier sets
            // off; any other step when it ends (a move when the carrier arrives).
            let end = if matches!(st.action, Action::Pass { .. }) { depart.unwrap_or(t1) } else { t1 };
            if let Some(id) = &st.id {
                history.push((format!("step {id}"), t0, end));
            }
            match &st.action {
                Action::Message { kind: Some(k), carrier, .. } => {
                    history.push((k.clone(), t0, t1));
                    if let Some(c) = carrier {
                        history.push((format!("{k} {c}"), t0, t1));
                    }
                }
                Action::Pipette { device, .. } => history.push((format!("pipette {device}"), t0, t1)),
                Action::Rotate { device } => history.push((format!("rotate {device}"), t0, t1)),
                Action::Needle { device, .. } => history.push((format!("needle {device}"), t0, t1)),
                _ => {}
            }
            log.push(format!("{t0:7.2}s - {t1:7.2}s  {desc}"));
            // The analyzer shows what it is doing on its lifeline.
            let note = match (&ev, &st.action) {
                (Ev::Pipette { device }, Action::Pipette { note, sample, .. }) => sc.pipetting_note(*device, note.as_deref(), sample.as_deref()),
                _ => None,
            };
            events.push(Timed { t0, t1, ev, at: Some(st.at.clone()) });
            if let Some((ai, text)) = note {
                log.push(format!("{t0:7.2}s - {t1:7.2}s  note      {}: {text}", sc.actors[ai].id));
                events.push(Timed { t0, t1, ev: Ev::Note { actor: ai, text, row: 0 }, at: Some(st.at.clone()) });
            }
            prev_end = end;
            max_end = max_end.max(t1);
        }
        if !check {
            let tl = Timeline { events, carriers, wheels, gates: Vec::new(), rows: 0, duration: max_end, log: Vec::new() };
            return Ok((tl, plans));
        }
        simulate_wheels(sc, &wheels, &mut carriers, wheel_recs)?;

        // Rows of the sequence diagram in time order (same start: file order), in
        // half rows: a label takes a row, and half a row for each more line.
        let mut order: Vec<usize> = (0..events.len()).filter(|&i| matches!(events[i].ev, Ev::Message { .. } | Ev::Note { .. })).collect();
        order.sort_by(|&a, &b| events[a].t0.total_cmp(&events[b].t0));
        let mut rows = 0;
        for &i in &order {
            let n = match &events[i].ev {
                Ev::Message { text, params, .. } => label_lines(text, params).len(),
                Ev::Note { text, .. } => label_lines(text, "").len(),
                _ => 1,
            };
            rows += n + 1;
            if let Ev::Message { row, .. } | Ev::Note { row, .. } = &mut events[i].ev {
                *row = rows - 2;
            }
        }

        // The gate in front of a turnstile holds carriers while the analyzer is
        // sampling there: nobody may drive into the wheel during a pipette.
        for (ci, c) in carriers.iter().enumerate() {
            for a in &c.attach {
                let di = wheels[a.wheel].device;
                for e in &events {
                    if matches!(e.ev, Ev::Pipette { device } if device == di) && a.t_move < e.t1 - 1e-9 && a.t0 > e.t0 + 1e-9 {
                        return Err(format!(
                            "carrier '{}' drives into turnstile '{}' ({:.2}s - {:.2}s) while the analyzer is sampling there ({:.2}s - {:.2}s): it must wait at the gate",
                            sc.carriers[ci].id, sc.devices[di].id, a.t_move, a.t0, e.t0, e.t1
                        ));
                    }
                }
            }
        }

        // A turnstile can't turn while a carrier outside its pockets (one that
        // has just left, or is arriving) still overlaps the wheel.
        for w in &wheels {
            let clear = w.r + TS_RIM + CARRIER_R;
            for &(r0, r1) in &w.rots {
                let n = ((r1 - r0) / 0.05).ceil().max(1.0) as usize;
                for k in 0..=n {
                    let t = r0 + (r1 - r0) * k as f64 / n as f64;
                    for (ci, c) in carriers.iter().enumerate() {
                        // At the instant it leaves, a carrier is still in the exit pocket.
                        let on_wheel = c.attach.iter().any(|a| t >= a.t0 && t <= a.t1);
                        if on_wheel || c.ghost || c.opacity(t) <= 0.0 {
                            continue;
                        }
                        let (x, y) = c.pos(t);
                        let d = ((x - w.cx).powi(2) + (y - w.cy).powi(2)).sqrt();
                        if d < clear - 1e-6 {
                            return Err(format!(
                                "turnstile '{}' rotates ({r0:.2}s - {r1:.2}s) while carrier '{}' is not yet clear of the wheel (at t={t:.2}s it is {d:.0} px from the center, needs {clear:.0}); start the rotation later",
                                sc.devices[w.device].id, sc.carriers[ci].id
                            ));
                        }
                    }
                }
            }
        }

        // While the needle is down (from the start of a `needle down` to the end
        // of the next `needle up`) the wheel can't turn and the carrier under
        // the needle can't leave.
        let mut needle_down: Vec<(usize, f64, f64)> = Vec::new();
        let mut needles: Vec<&Timed> = events.iter().filter(|e| matches!(e.ev, Ev::Needle { .. })).collect();
        needles.sort_by(|a, b| a.t0.total_cmp(&b.t0));
        for (k, e) in needles.iter().enumerate() {
            if let Ev::Needle { device, down: true } = e.ev {
                let up = needles[k + 1..].iter().find(|u| matches!(u.ev, Ev::Needle { device: d, down: false } if d == device));
                needle_down.push((device, e.t0, up.map_or(f64::INFINITY, |u| u.t1)));
            }
        }
        for &(di, a, b) in &needle_down {
            let d = &sc.devices[di];
            if let Some(w) = wheels.iter().find(|w| w.device == di) {
                if let Some(r) = w.rots.iter().find(|r| r.0 < b - 1e-9 && r.1 > a + 1e-9) {
                    return Err(format!("turnstile '{}' rotates ({:.2}s - {:.2}s) while the needle is down ({a:.2}s - {b:.2}s)", d.id, r.0, r.1));
                }
            } else {
                let p = sc.pt(d.at.as_deref().unwrap());
                for (ci, c) in carriers.iter().enumerate() {
                    for m in c.moves.iter().filter(|m| m.s1 > m.s0 + 1e-9) {
                        let at = c.pos_at_s(m.s0);
                        if m.t0 > a - 1e-9 && m.t0 < b - 1e-9 && ((at.0 - p.0).powi(2) + (at.1 - p.1).powi(2)).sqrt() < 3.0 {
                            return Err(format!("carrier '{}' leaves '{}' at {:.2}s while the needle is down ({a:.2}s - {b:.2}s)", sc.carriers[ci].id, d.id, m.t0));
                        }
                    }
                }
            }
        }

        // A device lights up while its antenna reads a carrier: from when the
        // carrier gets on the antenna until it leaves it, at least READ_LIGHT
        // seconds (a carrier passing the NSD is read in a moment). The NSD and the
        // diverts carrying the antenna light up, or an antenna on its own. Ghosts
        // and hidden carriers are not read; sampling points don't light up.
        for (ai, d) in sc.devices.iter().enumerate().filter(|(_, d)| d.kind == DeviceKind::Antenna && d.visible) {
            let target = match &d.part_of {
                Some(p) => match sc.devices.iter().position(|x| x.id == *p) {
                    Some(pi) if matches!(sc.devices[pi].kind, DeviceKind::Nsd | DeviceKind::Divert) => pi,
                    _ => continue,
                },
                None => ai,
            };
            let at = d.at.as_deref().unwrap_or_default();
            for (ci, c) in sc.carriers.iter().enumerate().filter(|(_, c)| !c.ghost) {
                let path = &carriers[ci];
                let start = c.route.iter().position(|p| *p == c.start).unwrap_or(0);
                for (k, _) in c.route.iter().enumerate().filter(|(k, p)| p.as_str() == at && *k != start) {
                    if let Some(t0) = path.reaches(path.cum[k]).filter(|&t0| path.opacity(t0) > 0.0) {
                        let off = path.reaches(path.cum[k] + 1.0).unwrap_or(max_end);
                        let t1 = off.max(t0 + READ_LIGHT);
                        log.push(format!("{t0:7.2}s - {t1:7.2}s  highlight {}  ({} read by {})", sc.devices[target].id, c.id, d.id));
                        events.push(Timed { t0, t1, ev: Ev::Highlight { device: target }, at: None });
                    }
                }
            }
        }

        // Gates: open from shortly before a carrier reaches them until it is through.
        let mut gates = Vec::new();
        for (di, d) in sc.devices.iter().enumerate().filter(|(_, d)| d.kind == DeviceKind::Gate) {
            let g = sc.pt(d.at.as_deref().unwrap());
            let mut iv = Vec::new();
            for c in carriers.iter().filter(|c| !c.ghost) {
                if let Some(sg) = c.project(g) {
                    if let Some(k) = c.moves.iter().position(|m| m.s0 < sg && m.s1 > sg) {
                        // The start of the run that goes through the gate: a carrier
                        // held by it sets off only when it opens.
                        let mut first = k;
                        while first > 0 && (c.moves[first - 1].t1 - c.moves[first].t0).abs() < 1e-9 && c.moves[first - 1].s1 > c.moves[first - 1].s0 + 1e-9 {
                            first -= 1;
                        }
                        let open = c.reaches(sg - CARRIER_R).unwrap_or(c.moves[k].t0).max(c.moves[first].t0);
                        iv.push((open, c.reaches(sg + CARRIER_R).unwrap_or(c.moves[k].t1)));
                    }
                }
            }
            gates.push((di, iv));
        }
        Ok((Timeline { events, carriers, wheels, gates, rows, duration: max_end + sc.video.tail, log: std::mem::take(log) }, plans))
    }
}

fn with_log(e: String, log: &[String]) -> String {
    if log.is_empty() {
        e
    } else {
        format!("{e}\nTimeline up to the error:\n  {}", log.join("\n  "))
    }
}

#[cfg(test)]
#[path = "scene_tests.rs"]
pub(crate) mod tests;
