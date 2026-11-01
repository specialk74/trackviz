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
}

pub struct Device {
    pub id: String,
    pub kind: DeviceKind,
    pub at: Option<String>,
    pub rect: (f64, f64, f64, f64),
    pub label: String,
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
    Message { from: String, to: String, text: String, params: String, reply: bool, carrier: Option<String> },
    Note { actor: String, text: String },
    Pipette { device: String, carrier: Option<String> },
    Highlight { device: String },
    Wait,
}

#[derive(Clone, Copy, PartialEq)]
pub enum StartMode {
    After,
    With,
    AfterAll,
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

        let mut devices = Vec::new();
        for t in arr(doc, "device") {
            t.check_keys(&["id", "kind", "at", "x", "y", "w", "h", "label"])?;
            let kind = match t.str_req("kind")?.as_str() {
                "nsd" => DeviceKind::Nsd,
                "sampling" => DeviceKind::Sampling,
                "analyzer" => DeviceKind::Analyzer,
                "zone" => DeviceKind::Zone,
                o => return Err(format!("{}: unknown device kind '{o}' (nsd|sampling|analyzer|zone)", t.location())),
            };
            let at = t.str_opt("at")?;
            let mut rect = (0.0, 0.0, 0.0, 0.0);
            match kind {
                DeviceKind::Nsd | DeviceKind::Sampling => {
                    let a = at.as_deref().ok_or_else(|| format!("{}: '{}' needs 'at = <point id>'", t.location(), t.str_req("kind").unwrap()))?;
                    need_point(a, t)?;
                }
                _ => {
                    rect = (t.num_req("x")?, t.num_req("y")?, t.num_req("w")?, t.num_req("h")?);
                }
            }
            devices.push(Device {
                id: t.str_req("id")?,
                kind,
                at,
                rect,
                label: t.str_opt("label")?.unwrap_or_default(),
            });
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
            t.check_keys(&["id", "tube", "sample_id", "route", "start", "speed", "cap_color"])?;
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
            });
        }

        let mut steps = Vec::new();
        for t in arr(doc, "step") {
            let common = ["action", "start", "delay", "duration"];
            let action_name = t.str_req("action")?;
            let extra: &[&str] = match action_name.as_str() {
                "caption" => &["title", "text"],
                "move" => &["carrier", "to", "ease", "speed"],
                "message" => &["from", "to", "text", "params", "reply", "carrier"],
                "note" => &["actor", "text"],
                "pipette" => &["device", "carrier"],
                "highlight" => &["device"],
                "wait" => &[],
                o => return Err(format!("{}: unknown action '{o}' (caption|move|message|note|pipette|highlight|wait)", t.location())),
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
                },
                "note" => Action::Note { actor: t.str_req("actor")?, text: t.str_req("text")? },
                "pipette" => Action::Pipette { device: t.str_req("device")?, carrier: t.str_opt("carrier")? },
                "highlight" => Action::Highlight { device: t.str_req("device")? },
                _ => {
                    if t.num_opt("duration")?.is_none() {
                        return Err(format!("{}: 'wait' needs a duration", t.location()));
                    }
                    Action::Wait
                }
            };
            let start = match t.str_opt("start")?.as_deref().unwrap_or("after") {
                "after" => StartMode::After,
                "with" => StartMode::With,
                "after_all" => StartMode::AfterAll,
                o => return Err(format!("{}: unknown start '{o}' (after|with|after_all)", t.location())),
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
}

pub enum Ev {
    Caption { title: String, text: String },
    Message { from: usize, to: usize, text: String, params: String, reply: bool, row: usize },
    Note { actor: usize, text: String, row: usize },
    Pipette { device: usize },
    Highlight { device: usize },
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
    pub rows: usize,
    pub duration: f64,
    pub log: Vec<String>,
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
    pub fn build(sc: &Scene) -> Result<Timeline, String> {
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
            carriers.push(CarrierPath { start_s: cum[si], pts, cum, moves: Vec::new() });
        }

        let mut events = Vec::new();
        let mut log = Vec::new();
        let (mut prev_start, mut prev_end, mut max_end) = (0.0f64, 0.0f64, 0.0f64);
        let mut row = 0usize;
        let mut first = true;

        for st in &sc.steps {
            let base = if first {
                0.0
            } else {
                match st.start {
                    StartMode::After => prev_end,
                    StartMode::With => prev_start,
                    StartMode::AfterAll => max_end,
                }
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
                    let from = cur_idx[ci];
                    let ti = c.route.iter().enumerate().skip(from + 1).find(|(_, p)| *p == to).map(|(i, _)| i)
                        .ok_or_else(|| format!("{}: point '{to}' is not ahead of carrier '{carrier}' on its route", st.loc))?;
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
                    (d, Ev::Wait, format!("move      {carrier} -> {to}"))
                }
                Action::Message { from, to, text, params, reply, carrier } => {
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
                    let ev = Ev::Message { from: fi, to: ti, text, params, reply: *reply, row };
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
                    if sc.devices[di].kind != DeviceKind::Sampling {
                        return Err(format!("{}: pipette needs a device of kind 'sampling'", st.loc));
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
                Action::Wait => (st.duration.unwrap(), Ev::Wait, "wait".to_string()),
            };
            let t1 = t0 + dur;
            log.push(format!("{t0:7.2}s - {t1:7.2}s  {desc}"));
            events.push(Timed { t0, t1, ev });
            prev_start = t0;
            prev_end = t1;
            max_end = max_end.max(t1);
        }
        Ok(Timeline { events, carriers, rows: row, duration: max_end + sc.video.tail, log })
    }
}
