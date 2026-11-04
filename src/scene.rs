//! Scene model (parsed from the scenario file) and timeline computation.

use crate::toml::{Doc, Table};
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
    /// Reader next to a lane; shows a reading animation while a carrier stands on its point.
    Antenna,
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
}

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

#[allow(dead_code)]
pub enum Action {
    Caption { title: String, text: String },
    Move { carrier: String, to: String, ease: Ease, speed: Option<f64> },
    Message { from: String, to: String, text: String, params: String, reply: bool, carrier: Option<String>, color: Option<String> },
    Note { actor: String, text: String },
    Pipette { device: String, carrier: Option<String> },
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

#[derive(Clone, Copy, PartialEq)]
pub enum AnalyzerState {
    Normal,
    Error,
    Ready,
}

#[derive(Clone, Copy, PartialEq)]
pub enum StartMode {
    After,
    With,
    AfterAll,
    /// Absolute start time in seconds (`at = ...`); `delay` is added on top.
    At(f64),
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
            if !["point", "lane", "device", "actor", "carrier", "step"].contains(&k.as_str()) {
                return Err(format!("unknown section [[{k}]]"));
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
        for t in arr(doc, "device") {
            let kind = match t.str_req("kind")?.as_str() {
                "nsd" => DeviceKind::Nsd,
                "sampling" => DeviceKind::Sampling,
                "analyzer" => DeviceKind::Analyzer,
                "zone" => DeviceKind::Zone,
                "turnstile" => DeviceKind::Turnstile,
                "gate" => DeviceKind::Gate,
                "antenna" => DeviceKind::Antenna,
                o => return Err(format!("{}: unknown device kind '{o}' (nsd|sampling|analyzer|zone|turnstile|gate|antenna)", t.location())),
            };
            let id = t.str_req("id")?;
            let mut at = None;
            let mut rect = (0.0, 0.0, 0.0, 0.0);
            let mut entry_right = true;
            match kind {
                DeviceKind::Nsd | DeviceKind::Sampling | DeviceKind::Gate | DeviceKind::Antenna => {
                    t.check_keys(&["id", "kind", "at", "label"])?;
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
            devices.push(Device { id, kind, at, rect, label: t.str_opt("label")?.unwrap_or_default(), entry_right });
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
            t.check_keys(&["id", "label", "sub", "color"])?;
            actors.push(Actor {
                id: t.str_req("id")?,
                label: t.str_req("label")?,
                sub: t.str_opt("sub")?.unwrap_or_default(),
                color: t.str_opt("color")?.unwrap_or_else(|| "#334155".into()),
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
                speed: t.num_opt("speed")?.unwrap_or(100.0),
                cap_color: t.str_opt("cap_color")?.unwrap_or_else(|| "#7C3AED".into()),
                ghost: t.bool_opt("ghost")?.unwrap_or(false),
                visible: t.bool_opt("visible")?.unwrap_or(true),
                tag: t.str_opt("tag")?,
            });
        }

        let mut steps = Vec::new();
        for t in arr(doc, "step") {
            let common = ["action", "start", "at", "delay", "duration"];
            let action_name = t.str_req("action")?;
            let extra: &[&str] = match action_name.as_str() {
                "caption" => &["title", "text"],
                "move" => &["carrier", "to", "ease", "speed"],
                "message" => &["from", "to", "text", "params", "reply", "carrier", "color"],
                "note" => &["actor", "text"],
                "pipette" => &["device", "carrier"],
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
                    ease: match t.str_opt("ease")?.as_deref().unwrap_or("inout") {
                        "linear" => Ease::Linear,
                        "in" => Ease::In,
                        "out" => Ease::Out,
                        "inout" => Ease::InOut,
                        o => return Err(format!("{}: unknown ease '{o}' (linear|in|out|inout)", t.location())),
                    },
                    speed: t.num_opt("speed")?,
                },
                "message" => Action::Message {
                    from: t.str_req("from")?,
                    to: t.str_req("to")?,
                    text: t.str_req("text")?,
                    params: t.str_opt("params")?.unwrap_or_default(),
                    reply: t.bool_opt("reply")?.unwrap_or(false),
                    carrier: t.str_opt("carrier")?,
                    color: t.str_opt("color")?,
                },
                "note" => Action::Note { actor: t.str_req("actor")?, text: t.str_req("text")? },
                "pipette" => Action::Pipette { device: t.str_req("device")?, carrier: t.str_opt("carrier")? },
                "highlight" => Action::Highlight { device: t.str_req("device")? },
                "rotate" => Action::Rotate { device: t.str_req("device")? },
                "show" | "hide" => Action::Visibility { carrier: t.str_req("carrier")?, show: action_name == "show" },
                "status" => Action::Status {
                    device: t.str_req("device")?,
                    state: match t.str_req("state")?.as_str() {
                        "normal" => AnalyzerState::Normal,
                        "error" => AnalyzerState::Error,
                        "ready" => AnalyzerState::Ready,
                        o => return Err(format!("{}: unknown state '{o}' (normal|error|ready)", t.location())),
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
            let start = match (t.num_opt("at")?, t.str_opt("start")?) {
                (Some(_), Some(_)) => return Err(format!("{}: use either 'at' or 'start', not both", t.location())),
                (Some(at), None) => StartMode::At(at),
                (None, s) => match s.as_deref().unwrap_or("after") {
                    "after" => StartMode::After,
                    "with" => StartMode::With,
                    "after_all" => StartMode::AfterAll,
                    o => return Err(format!("{}: unknown start '{o}' (after|with|after_all)", t.location())),
                },
            };
            steps.push(Step {
                action,
                start,
                delay: t.num_opt("delay")?.unwrap_or(0.0),
                duration: t.num_opt("duration")?,
                loc: t.location(),
            });
        }
        if steps.is_empty() {
            return Err("the scene has no [[step]] entries".into());
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
    pub fn pt(&self, id: &str) -> (f64, f64) {
        let p = &self.points[id];
        (p.x, p.y)
    }
}

// ---------------------------------------------------------------- timeline

pub struct MoveSeg {
    pub t0: f64,
    pub t1: f64,
    pub s0: f64,
    pub s1: f64,
    pub ease: Ease,
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
        self.moves.iter().any(|m| t > m.t0 && t < m.t1)
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
    /// First time within move m at which the carrier reaches distance s.
    fn time_at_s(&self, m: &MoveSeg, s: f64) -> f64 {
        if s <= m.s0 {
            return m.t0;
        }
        let n = 400;
        for k in 0..=n {
            let t = m.t0 + (m.t1 - m.t0) * k as f64 / n as f64;
            if self.s_at(t) >= s {
                return t;
            }
        }
        m.t1
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

fn subst(s: &str, sc: &Scene, carrier: Option<usize>) -> String {
    let c = carrier.or(if sc.carriers.is_empty() { None } else { Some(0) });
    match c {
        Some(ci) => s
            .replace("{SampleId}", &sc.carriers[ci].sample_id)
            .replace("{CarrierId}", &sc.carriers[ci].id),
        None => s.to_string(),
    }
}

impl Timeline {
    /// Lays out the steps in time. On error, the message also lists the
    /// timeline computed up to that point (useful to fix delays in a scene).
    pub fn build(sc: &Scene) -> Result<Timeline, String> {
        let mut log = Vec::new();
        Self::build_inner(sc, &mut log).map_err(|e| {
            if log.is_empty() {
                e
            } else {
                format!("{e}\nTimeline up to the error:\n  {}", log.join("\n  "))
            }
        })
    }

    fn build_inner(sc: &Scene, log: &mut Vec<String>) -> Result<Timeline, String> {
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
        let mut row = 0usize;
        let mut first = true;

        for st in &sc.steps {
            let base = match st.start {
                StartMode::At(at) => at,
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
                Action::Move { carrier, to, ease, speed } => {
                    let ci = sc.carrier_idx(carrier).ok_or_else(|| format!("{}: unknown carrier '{carrier}'", st.loc))?;
                    let c = &sc.carriers[ci];
                    let mut from = cur_idx[ci];
                    // Leaving a turnstile (from the exit position: checked by simulate_wheels).
                    if let Some(ai) = attached[ci] {
                        let wheel = carriers[ci].attach[ai].wheel;
                        wheel_recs.push(WheelRec { t: t0, wheel, ev: WheelEv::Leave(ci, ai), loc: st.loc.clone() });
                        carriers[ci].attach[ai].t1 = t0;
                        attached[ci] = None;
                        from += 1; // the route point after "<id>.in" is "<id>.out"
                        cur_idx[ci] = from;
                    }
                    let ti = c.route.iter().enumerate().skip(from + 1).find(|(_, p)| *p == to).map(|(i, _)| i)
                        .ok_or_else(|| format!("{}: point '{to}' is not ahead of carrier '{carrier}' on its route", st.loc))?;
                    if let Some(p) = c.route[from..ti].iter().find(|p| wheel_pts.get(*p).is_some_and(|w| w.1 == 0)) {
                        return Err(format!("{}: carrier '{carrier}' would drive through the turnstile; move it to '{p}' and use 'rotate'", st.loc));
                    }
                    let path = &mut carriers[ci];
                    let (s0, s1) = (path.cum[from], path.cum[ti]);
                    if let Some(last) = path.moves.last() {
                        if t0 < last.t1 - 1e-9 {
                            return Err(format!("{}: carrier '{carrier}' is still moving at t={t0:.2}s", st.loc));
                        }
                    }
                    let d = st.duration.unwrap_or((s1 - s0) / speed.unwrap_or(c.speed) * ease.factor());
                    path.moves.push(MoveSeg { t0, t1: t0 + d, s0, s1, ease: *ease });
                    cur_idx[ci] = ti;
                    // Entering a turnstile: the carrier drops into the pocket at the entry.
                    // The pocket is assigned later, in time order (simulate_wheels).
                    if let Some(&(wi, 0)) = wheel_pts.get(to) {
                        let t_in = t0 + d;
                        carriers[ci].attach.push(Attach { wheel: wi, pocket: 0, t_move: t0, t0: t_in, t1: f64::INFINITY });
                        let ai = carriers[ci].attach.len() - 1;
                        attached[ci] = Some(ai);
                        wheel_recs.push(WheelRec { t: t_in, wheel: wi, ev: WheelEv::Arrive(ci, ai), loc: st.loc.clone() });
                    }
                    (d, Ev::Wait, format!("move      {carrier} -> {to}"))
                }
                Action::Message { from, to, text, params, reply, carrier, color } => {
                    let fi = sc.actor_idx(from).ok_or_else(|| format!("{}: unknown actor '{from}'", st.loc))?;
                    let ti = sc.actor_idx(to).ok_or_else(|| format!("{}: unknown actor '{to}'", st.loc))?;
                    if fi == ti {
                        return Err(format!("{}: message from an actor to itself is not supported; use a note", st.loc));
                    }
                    let ci = match carrier {
                        Some(c) => Some(sc.carrier_idx(c).ok_or_else(|| format!("{}: unknown carrier '{c}'", st.loc))?),
                        None => None,
                    };
                    let text = subst(text, sc, ci);
                    let params = subst(params, sc, ci);
                    let desc = format!("message   {from} -> {to}  {text}  {params}");
                    let ev = Ev::Message { from: fi, to: ti, text, params, reply: *reply, row, color: color.clone() };
                    row += 1;
                    (st.duration.unwrap_or(1.5), ev, desc)
                }
                Action::Note { actor, text } => {
                    let ai = sc.actor_idx(actor).ok_or_else(|| format!("{}: unknown actor '{actor}'", st.loc))?;
                    let ev = Ev::Note { actor: ai, text: text.clone(), row };
                    row += 1;
                    (st.duration.unwrap_or(1.5), ev, format!("note      {actor}: {text}"))
                }
                Action::Pipette { device, .. } => {
                    let di = sc.device_idx(device).ok_or_else(|| format!("{}: unknown device '{device}'", st.loc))?;
                    if !matches!(sc.devices[di].kind, DeviceKind::Sampling | DeviceKind::Turnstile) {
                        return Err(format!("{}: pipette needs a device of kind 'sampling' or 'turnstile'", st.loc));
                    }
                    if !sc.devices.iter().any(|d| d.kind == DeviceKind::Analyzer) {
                        return Err(format!("{}: pipette needs an 'analyzer' device", st.loc));
                    }
                    (st.duration.unwrap_or(3.0), Ev::Pipette { device: di }, format!("pipette   at {device}"))
                }
                Action::Highlight { device } => {
                    let di = sc.device_idx(device).ok_or_else(|| format!("{}: unknown device '{device}'", st.loc))?;
                    (st.duration.unwrap_or(1.5), Ev::Highlight { device: di }, format!("highlight {device}"))
                }
                Action::Rotate { device } => {
                    let di = sc.device_idx(device).ok_or_else(|| format!("{}: unknown device '{device}'", st.loc))?;
                    let wi = wheels.iter().position(|w| w.device == di)
                        .ok_or_else(|| format!("{}: rotate needs a device of kind 'turnstile'", st.loc))?;
                    let d = st.duration.unwrap_or(2.0);
                    wheels[wi].rots.push((t0, t0 + d));
                    wheel_recs.push(WheelRec { t: t0, wheel: wi, ev: WheelEv::Rotate(t0 + d), loc: st.loc.clone() });
                    (d, Ev::Wait, format!("rotate    {device}"))
                }
                Action::Visibility { carrier, show } => {
                    let ci = sc.carrier_idx(carrier).ok_or_else(|| format!("{}: unknown carrier '{carrier}'", st.loc))?;
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
                    let di = sc.device_idx(device).ok_or_else(|| format!("{}: unknown device '{device}'", st.loc))?;
                    if sc.devices[di].kind != DeviceKind::Analyzer {
                        return Err(format!("{}: status needs a device of kind 'analyzer'", st.loc));
                    }
                    let name = match state {
                        AnalyzerState::Normal => "normal",
                        AnalyzerState::Error => "error",
                        AnalyzerState::Ready => "ready",
                    };
                    (st.duration.unwrap_or(0.0), Ev::Status { device: di, state: *state }, format!("status    {device} {name}"))
                }
                Action::Needle { device, down } => {
                    let di = sc.device_idx(device).ok_or_else(|| format!("{}: unknown device '{device}'", st.loc))?;
                    if !matches!(sc.devices[di].kind, DeviceKind::Sampling | DeviceKind::Turnstile) {
                        return Err(format!("{}: needle needs a device of kind 'sampling' or 'turnstile'", st.loc));
                    }
                    if !sc.devices.iter().any(|d| d.kind == DeviceKind::Analyzer) {
                        return Err(format!("{}: needle needs an 'analyzer' device", st.loc));
                    }
                    let dir = if *down { "down" } else { "up" };
                    (st.duration.unwrap_or(0.9), Ev::Needle { device: di, down: *down }, format!("needle    {device} {dir}"))
                }
                Action::Operator { device } => {
                    let di = sc.device_idx(device).ok_or_else(|| format!("{}: unknown device '{device}'", st.loc))?;
                    if sc.devices[di].kind != DeviceKind::Analyzer {
                        return Err(format!("{}: operator needs a device of kind 'analyzer'", st.loc));
                    }
                    let d = st.duration.unwrap_or(7.0);
                    if d < 2.0 {
                        return Err(format!("{}: operator needs a duration of at least 2 s (walk in and out)", st.loc));
                    }
                    (d, Ev::Operator { device: di }, format!("operator  at {device}"))
                }
                Action::Wait => (st.duration.unwrap(), Ev::Wait, "wait".to_string()),
            };
            let t1 = t0 + dur;
            log.push(format!("{t0:7.2}s - {t1:7.2}s  {desc}"));
            events.push(Timed { t0, t1, ev });
            prev_start = t0;
            prev_end = t1;
            max_end = max_end.max(t1);
        }
        simulate_wheels(sc, &wheels, &mut carriers, wheel_recs)?;

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
                    for m in &c.moves {
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
                    for m in c.moves.iter().filter(|m| m.s0 < sg && m.s1 > sg) {
                        iv.push((c.time_at_s(m, sg - 60.0), c.time_at_s(m, sg + 45.0)));
                    }
                }
            }
            gates.push((di, iv));
        }
        Ok(Timeline { events, carriers, wheels, gates, rows: row, duration: max_end + sc.video.tail, log: std::mem::take(log) })
    }
}
