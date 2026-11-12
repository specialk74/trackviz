//! The scene from its document: video, points, devices, lanes, actors, carriers,
//! steps; then the gates of the antennas and the way of each carrier.

use super::*;
use super::lanes::{expand_route, lane_dir, sc_pt};
use crate::doc::{Doc, Table};
use std::collections::HashMap;

fn arr<'a>(doc: &'a Doc, name: &str) -> &'a [Table] {
    doc.arrays.get(name).map(|v| v.as_slice()).unwrap_or(&[])
}

impl Scene {
    pub fn from_doc(doc: &Doc) -> Result<Scene, String> {
        check_sections(doc)?;
        let video = video(doc)?;
        let mut points = points(doc)?;
        let (mut devices, gated) = devices(doc, &mut points)?;
        let lanes = lanes(doc, &points)?;
        let actors = actors(doc)?;
        let carriers = carriers(doc, &video, &points, &lanes, &devices)?;
        let steps = steps(doc)?;
        add_gates(&gated, &mut points, &mut devices, &lanes);
        let mut sc = Scene { video, points, lanes, devices, actors, carriers, steps, straight: Vec::new() };
        let routes = sc.derive_routes()?;
        for (c, r) in sc.carriers.iter_mut().zip(routes) {
            if c.route.is_empty() {
                c.route = r;
            }
        }
        Ok(sc)
    }
}

/// Only known sections, no id defined twice (included files make it easy).
fn check_sections(doc: &Doc) -> Result<(), String> {
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
    if !doc.tables[""].entries.is_empty() {
        return Err("keys outside of any section are not allowed".into());
    }
    Ok(())
}

fn video(doc: &Doc) -> Result<Video, String> {
    let v = doc.tables.get("video").ok_or("missing [video] section")?;
    v.check_keys(&["title", "subtitle", "width", "height", "fps", "tail", "show_clock", "show_times", "show_line", "belt", "message"])?;
    Ok(Video {
        title: v.str_opt("title")?.unwrap_or_default(),
        subtitle: v.str_opt("subtitle")?.unwrap_or_default(),
        width: v.num_opt("width")?.unwrap_or(1920.0) as u32,
        height: v.num_opt("height")?.unwrap_or(1080.0) as u32,
        fps: v.num_opt("fps")?.unwrap_or(30.0) as u32,
        tail: v.num_opt("tail")?.unwrap_or(2.0),
        show_clock: v.bool_opt("show_clock")?.unwrap_or(true),
        show_times: v.bool_opt("show_times")?.unwrap_or(true),
        show_line: v.bool_opt("show_line")?.unwrap_or(true),
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
    })
}

fn points(doc: &Doc) -> Result<HashMap<String, Point>, String> {
    let mut points = HashMap::new();
    for t in arr(doc, "point") {
        t.check_keys(&["id", "x", "y"])?;
        let id = t.str_req("id")?;
        let p = Point { x: t.num_req("x")?, y: t.num_req("y")? };
        if points.insert(id.clone(), p).is_some() {
            return Err(format!("{}: duplicate point id '{id}'", t.location()));
        }
    }
    Ok(points)
}

/// The devices, with the antenna an nsd, sampling point, divert or turnstile
/// carries (`antenna=`) right after it; a turnstile adds its points. Also the
/// devices whose antenna has a gate (indices), for `add_gates`.
fn devices(doc: &Doc, points: &mut HashMap<String, Point>) -> Result<(Vec<Device>, Vec<usize>), String> {
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
    Ok((devices, gated))
}

fn lanes(doc: &Doc, points: &HashMap<String, Point>) -> Result<Vec<Lane>, String> {
    let mut lanes = Vec::new();
    for t in arr(doc, "lane") {
        t.check_keys(&["id", "label", "points", "kind", "label_x", "label_y", "capacity"])?;
        let pts = t.str_list("points")?;
        if pts.len() < 2 {
            return Err(format!("{}: a lane needs at least 2 points", t.location()));
        }
        if let Some(p) = pts.iter().find(|p| !points.contains_key(*p)) {
            return Err(format!("{}: unknown point '{p}'", t.location()));
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
    Ok(lanes)
}

fn actors(doc: &Doc) -> Result<Vec<Actor>, String> {
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
    Ok(actors)
}

fn carriers(doc: &Doc, video: &Video, points: &HashMap<String, Point>, lanes: &[Lane], devices: &[Device]) -> Result<Vec<Carrier>, String> {
    let mut carriers = Vec::new();
    for t in arr(doc, "carrier") {
        t.check_keys(&["id", "tube", "sample_id", "route", "start", "speed", "cap_color", "ghost", "visible", "tag"])?;
        // Without `route` the way of the carrier comes from its steps (see
        // `derive_routes`), from `start`.
        let route = if t.entries.contains_key("route") {
            expand_route(&t.str_list("route")?, points, lanes, devices).map_err(|e| format!("{}: {e}", t.location()))?
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
    Ok(carriers)
}

fn steps(doc: &Doc) -> Result<Vec<Step>, String> {
    // Named messages: a `message` step with `type` takes everything from here.
    let mut msg_types = HashMap::new();
    for t in arr(doc, "message") {
        t.check_keys(&["id", "from", "to", "text", "params", "reply", "color"])?;
        for k in ["from", "to", "text"] {
            t.str_req(k)?;
        }
        msg_types.insert(t.str_req("id")?, t);
    }
    let steps = arr(doc, "step").iter().map(|t| step(t, &msg_types)).collect::<Result<Vec<Step>, String>>()?;
    if steps.is_empty() {
        return Err("the scene has no steps".into());
    }
    Ok(steps)
}

fn step(t: &Table, msg_types: &HashMap<String, &Table>) -> Result<Step, String> {
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
    let action = action(t, &action_name, msg_types)?;
    let (start, delay) = start(t)?;
    Ok(Step { action, id: t.str_opt("id")?, start, delay, duration: t.num_opt("duration")?, loc: t.location(), at: (t.file.clone(), t.line) })
}

/// What a step does (its keys are already checked).
fn action(t: &Table, name: &str, msg_types: &HashMap<String, &Table>) -> Result<Action, String> {
    Ok(match name {
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
        "show" | "hide" => Action::Visibility { carrier: t.str_req("carrier")?, show: name == "show" },
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
    })
}

/// When a step starts: `at` (a second of the video), `after` (seconds after the
/// step before ends) or `when` (something earlier happens); and its delay.
fn start(t: &Table) -> Result<(StartMode, f64), String> {
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
    Ok((start, delay))
}

/// The gate of an antenna: across the lane, just in front of a carrier standing
/// on the antenna ("<antenna>.gate", a point and a device).
fn add_gates(gated: &[usize], points: &mut HashMap<String, Point>, devices: &mut Vec<Device>, lanes: &[Lane]) {
    for &di in gated {
        let a = sc_pt(points, devices[di].at.as_deref().unwrap());
        let (dx, dy) = lane_dir(points, lanes, a);
        let id = format!("{}.gate", devices[di].id);
        let ahead = CARRIER_R + 6.0;
        points.insert(id.clone(), Point { x: a.0 + dx * ahead, y: a.1 + dy * ahead });
        let part_of = Some(devices[di].part_of.clone().unwrap_or_else(|| devices[di].id.clone()));
        devices.push(Device { id: id.clone(), kind: DeviceKind::Gate, at: Some(id), rect: (0.0, 0.0, 0.0, 0.0), label: String::new(), entry_right: true, label_pos: None, visible: true, part_of });
    }
}
