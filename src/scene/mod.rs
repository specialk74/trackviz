//! Scene model (parsed from the scenario file) and timeline computation.
//!
//! - `load`: the scene from its document (`Scene::from_doc`);
//! - `lanes`: the lanes as paths and as a graph;
//! - `routes`: the way of each carrier, and carriers sent straight on;
//! - `path`, `wheels`: where carriers are in time, turnstiles;
//! - `timeline`: the steps laid out in time, with the physics.

mod lanes;
mod load;
mod path;
mod routes;
mod timeline;
mod wheels;

pub use path::{Attach, CarrierPath, MoveSeg};
pub use timeline::{label_lines, Ev, Timed, Timeline};
pub use wheels::Wheel;
use lanes::{lane_dir, sc_pt};
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
    /// Show, small under the clock, the line of the scene that ran last (`show_line=`).
    pub show_line: bool,
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

impl Scene {
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

    /// The note of a `pipette` at device `di` on the analyzer lifeline: the text
    /// written in the diagram, or "Pipetting <analyzer>[ · Needle n][ - SampleId=...]"
    /// with the SampleId given to the pipette (with several sampling points it
    /// names the needle, needle 1 = the first in the file).
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

#[cfg(test)]
pub(crate) mod tests;
