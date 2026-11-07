//! Scene model (parsed from the scenario file) and timeline computation.

use crate::doc::{Doc, Table};
use crate::physics;
use std::collections::HashMap;

pub struct Video {
    pub title: String,
    pub subtitle: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub tail: f64,
    pub show_clock: bool,
    pub show_times: bool,
}

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

pub struct Lane {
    pub label: String,
    pub points: Vec<String>,
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

/// Speed of the belt under the carriers: 17.5 cm/s, with a carrier 4 cm wide
/// drawn 44 px wide (11 px/cm).
pub const BELT_SPEED: f64 = 17.5 * 2.0 * CARRIER_R / 4.0;

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

pub struct Actor {
    pub id: String,
    pub label: String,
    pub sub: String,
    pub color: String,
    /// The actor of the analyzer: `pipette` adds its "Pipetting" note here.
    pub analyzer: bool,
}

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

pub enum Action {
    Caption { title: String, text: String },
    /// `runaway`: the carrier escapes by itself (drawn in red, tag "RUN-AWAY").
    Move { carrier: String, to: String, ease: Ease, speed: Option<f64>, runaway: bool },
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

#[derive(Clone, PartialEq)]
pub enum StartMode {
    After,
    With,
    AfterAll,
    /// Absolute start time in seconds (`at = ...`); `delay` is added on top.
    At(f64),
    /// `when = ...`: when something earlier in the file happens; with several
    /// causes joined by " and ", when the last of them happens.
    When(Vec<Anchor>),
}

/// What a `when` refers to.
#[derive(Clone, PartialEq)]
pub enum Anchor {
    /// "C1 at merge": the carrier reaches the point.
    Arrival { carrier: String, point: String },
    /// "query C1 sent", "query C1 arrived", "pipette sp done": the latest matching
    /// step before this one in the file, at its start or at its end.
    Event { key: String, end: bool },
}

impl Anchor {
    fn parse(w: &str) -> Option<Anchor> {
        let t: Vec<&str> = w.split_whitespace().collect();
        match t[..] {
            [carrier, "at", point] => Some(Anchor::Arrival { carrier: carrier.into(), point: point.into() }),
            [ref who @ .., "sent"] if (1..=2).contains(&who.len()) => Some(Anchor::Event { key: who.join(" "), end: false }),
            [ref who @ .., "arrived"] if (1..=2).contains(&who.len()) => Some(Anchor::Event { key: who.join(" "), end: true }),
            [kind @ ("pipette" | "rotate" | "needle"), device, "done"] => Some(Anchor::Event { key: format!("{kind} {device}"), end: true }),
            _ => None,
        }
    }
}

pub struct Step {
    pub action: Action,
    pub start: StartMode,
    pub delay: f64,
    pub duration: Option<f64>,
    pub loc: String,
}

pub struct Scene {
    pub video: Video,
    pub points: HashMap<String, Point>,
    pub lanes: Vec<Lane>,
    pub devices: Vec<Device>,
    pub actors: Vec<Actor>,
    pub carriers: Vec<Carrier>,
    pub steps: Vec<Step>,
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
        v.check_keys(&["title", "subtitle", "width", "height", "fps", "tail", "show_clock", "show_times"])?;
        let video = Video {
            title: v.str_opt("title")?.unwrap_or_default(),
            subtitle: v.str_opt("subtitle")?.unwrap_or_default(),
            width: v.num_opt("width")?.unwrap_or(1920.0) as u32,
            height: v.num_opt("height")?.unwrap_or(1080.0) as u32,
            fps: v.num_opt("fps")?.unwrap_or(30.0) as u32,
            tail: v.num_opt("tail")?.unwrap_or(2.0),
            show_clock: v.bool_opt("show_clock")?.unwrap_or(true),
            show_times: v.bool_opt("show_times")?.unwrap_or(true),
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
                    t.check_keys(&["id", "kind", "x", "y", "r", "entry", "label"])?;
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
            let antenna = if matches!(kind, DeviceKind::Nsd | DeviceKind::Sampling | DeviceKind::Divert) { t.str_opt("antenna")? } else { None };
            let gate = t.bool_opt("gate")?.unwrap_or(false);
            let point = at.clone();
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
            t.check_keys(&["id", "label", "points", "kind", "label_x", "label_y"])?;
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
            lanes.push(Lane { label: t.str_opt("label")?.unwrap_or_default(), points: pts, kind, label_pos });
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
            let route = t.str_list("route")?;
            for p in &route {
                need_point(p, t)?;
            }
            let start = t.str_opt("start")?.unwrap_or_else(|| route[0].clone());
            if !route.contains(&start) {
                return Err(format!("{}: start point '{start}' is not on the route", t.location()));
            }
            carriers.push(Carrier {
                id: t.str_req("id")?,
                tube: t.bool_opt("tube")?.unwrap_or(true),
                sample_id: t.str_opt("sample_id")?.unwrap_or_default(),
                route,
                start,
                speed: t.num_opt("speed")?.unwrap_or(BELT_SPEED),
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
            let common = ["action", "start", "at", "after_start", "after_end", "when", "delay", "duration"];
            let action_name = t.str_req("action")?;
            let extra: &[&str] = match action_name.as_str() {
                "caption" => &["title", "text"],
                "move" => &["carrier", "to", "ease", "speed", "runaway"],
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
            // When the step starts: `at` (absolute), `after_start` / `after_end`
            // (seconds after the start / end of the previous step) or `start`.
            let (at, after_start, after_end) = (t.num_opt("at")?, t.num_opt("after_start")?, t.num_opt("after_end")?);
            let start_key = t.str_opt("start")?;
            let when = t.str_opt("when")?;
            let given = [at.is_some(), after_start.is_some(), after_end.is_some(), start_key.is_some(), when.is_some()].iter().filter(|b| **b).count();
            if given > 1 {
                return Err(format!("{}: use only one of 'at', 'after_start', 'after_end', 'when', 'start'", t.location()));
            }
            for (k, v) in [("after_start", after_start), ("after_end", after_end)] {
                if v.is_some_and(|v| v < 0.0) {
                    return Err(format!("{}: '{k}' must not be negative", t.location()));
                }
            }
            let mut delay = t.num_opt("delay")?.unwrap_or(0.0);
            let start = if let Some(w) = when {
                let anchors = w.split(" and ").map(|p| Anchor::parse(p.trim())).collect::<Option<Vec<_>>>().ok_or_else(|| {
                    format!("{}: 'when' is \"<carrier> at <point>\", \"<message type> [<carrier>] sent|arrived\" or \"pipette|rotate|needle <device> done\", joined by \" and \", not \"{w}\"", t.location())
                })?;
                StartMode::When(anchors)
            } else if let Some(at) = at {
                StartMode::At(at)
            } else if let Some(d) = after_start {
                delay += d;
                StartMode::With
            } else if let Some(d) = after_end {
                delay += d;
                StartMode::After
            } else {
                match start_key.as_deref().unwrap_or("after") {
                    "after" => StartMode::After,
                    "with" => StartMode::With,
                    "after_all" => StartMode::AfterAll,
                    o => return Err(format!("{}: unknown start '{o}' (after|with|after_all)", t.location())),
                }
            };
            steps.push(Step {
                action,
                start,
                delay,
                duration: t.num_opt("duration")?,
                loc: t.location(),
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
        Ok(Scene { video, points, lanes, devices, actors, carriers, steps })
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

pub struct Timed {
    pub t0: f64,
    pub t1: f64,
    pub ev: Ev,
}

pub struct Timeline {
    pub events: Vec<Timed>,
    pub carriers: Vec<CarrierPath>,
    pub wheels: Vec<Wheel>,
    /// Gate devices: (device index, intervals in which a carrier drives through).
    pub gates: Vec<(usize, Vec<(f64, f64)>)>,
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
        // The moves depend on the steps (when they start) and the steps on the
        // moves (when a carrier arrives): lay out, play the moves together, and
        // again until nothing changes.
        let mut actual: HashMap<usize, physics::Done> = HashMap::new();
        for _ in 0..40 {
            let mut log = Vec::new();
            let (tl, plans) = Self::build_inner(sc, &mut log, &actual, false).map_err(|e| with_log(e, &log))?;
            let next = physics::simulate(&tl.carriers, &plans);
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
        let (mut prev_start, mut prev_end, mut max_end) = (0.0f64, 0.0f64, 0.0f64);
        let mut first = true;
        // What a `when` can refer to: (key, start, end) of the steps so far.
        let mut history: Vec<(String, f64, f64)> = Vec::new();
        let mut plans: Vec<physics::Plan> = Vec::new();

        for (si, st) in sc.steps.iter().enumerate() {
            let base = match &st.start {
                StartMode::At(at) => *at,
                StartMode::When(anchors) => {
                    let mut base = f64::NEG_INFINITY;
                    for anchor in anchors {
                        base = base.max(match anchor {
                            Anchor::Event { key, end } => {
                                let &(_, a, b) = history.iter().rev().find(|h| h.0 == *key)
                                    .ok_or_else(|| format!("{}: no earlier step matches '{key}' (it must come before this step in the file)", st.loc))?;
                                if *end { b } else { a }
                            }
                            Anchor::Arrival { carrier, point } => {
                                let ci = sc.need_carrier(carrier, &st.loc)?;
                                let i = sc.carriers[ci].route.iter().position(|p| p == point)
                                    .ok_or_else(|| format!("{}: point '{point}' is not on the route of carrier '{carrier}'", st.loc))?;
                                let path = &carriers[ci];
                                path.reaches(path.cum[i]).ok_or_else(|| {
                                    format!("{}: no earlier step moves carrier '{carrier}' to '{point}' (the move must come before this step in the file)", st.loc)
                                })?
                            }
                        });
                    }
                    base
                }
                _ if first => 0.0,
                StartMode::After => prev_end,
                StartMode::With => prev_start,
                StartMode::AfterAll => max_end,
            };
            first = false;
            let t0 = base + st.delay;
            let (dur, ev, desc) = match &st.action {
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
                    (st.duration.unwrap_or(1.5), ev, desc)
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
                    wheels[wi].rots.push((t0, t0 + d));
                    wheel_recs.push(WheelRec { t: t0, wheel: wi, ev: WheelEv::Rotate(t0 + d), loc: st.loc.clone() });
                    (d, Ev::Wait, format!("rotate    {device}"))
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
                Action::Wait => (st.duration.unwrap(), Ev::Wait, "wait".to_string()),
            };
            let t1 = t0 + dur;
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
            events.push(Timed { t0, t1, ev });
            if let Some((ai, text)) = note {
                log.push(format!("{t0:7.2}s - {t1:7.2}s  note      {}: {text}", sc.actors[ai].id));
                events.push(Timed { t0, t1, ev: Ev::Note { actor: ai, text, row: 0 } });
            }
            prev_start = t0;
            prev_end = t1;
            max_end = max_end.max(t1);
        }
        if !check {
            let tl = Timeline { events, carriers, wheels, gates: Vec::new(), rows: 0, duration: max_end, log: Vec::new() };
            return Ok((tl, plans));
        }
        simulate_wheels(sc, &wheels, &mut carriers, wheel_recs)?;

        // Rows of the sequence diagram in time order (same start: file order).
        let mut order: Vec<usize> = (0..events.len()).filter(|&i| matches!(events[i].ev, Ev::Message { .. } | Ev::Note { .. })).collect();
        order.sort_by(|&a, &b| events[a].t0.total_cmp(&events[b].t0));
        for (r, &i) in order.iter().enumerate() {
            if let Ev::Message { row, .. } | Ev::Note { row, .. } = &mut events[i].ev {
                *row = r;
            }
        }
        let rows = order.len();

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
