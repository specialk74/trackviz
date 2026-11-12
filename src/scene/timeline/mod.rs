//! The steps laid out in time: events, and the loop that lays them out and
//! plays the moves together until the timing settles.

mod finish;
mod layout;
mod pass;
mod steps;

use layout::{Laid, Layout, Stuck};

use super::*;
use crate::physics;
use std::collections::HashMap;

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
    /// The lines of the scene, by the time their step starts: (t0, "file:line  text").
    pub lines: Vec<(f64, String)>,
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
}

/// The line of the scene of each step, by the time it starts (stable: in the
/// order of the file at the same time). The text is read from the file; a
/// scene not read from a file only has the line number.
fn source_lines(events: &[Timed]) -> Vec<(f64, String)> {
    let mut files: HashMap<String, Vec<String>> = HashMap::new();
    let mut seen = std::collections::HashSet::new();
    let mut v = Vec::new();
    for e in events {
        let Some((f, n)) = &e.at else { continue };
        // A step not written in a file (the caption of the video up to an error).
        if *n == 0 {
            continue;
        }
        if !seen.insert((f.clone(), *n)) {
            continue;
        }
        let lines = files.entry(f.clone()).or_insert_with(|| std::fs::read_to_string(f).map(|s| s.lines().map(str::to_string).collect()).unwrap_or_default());
        let text = lines.get(n.wrapping_sub(1)).map_or("", |l| l.trim());
        let name = std::path::Path::new(f).file_name().map_or(f.as_str(), |s| s.to_str().unwrap_or(f));
        v.push((e.t0, format!("{name}:{n} · {text}").trim_end_matches([' ', '·']).to_string()));
    }
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v
}

fn with_log(e: String, log: &[String]) -> String {
    if log.is_empty() {
        e
    } else {
        format!("{e}\nTimeline up to the error:\n  {}", log.join("\n  "))
    }
}
