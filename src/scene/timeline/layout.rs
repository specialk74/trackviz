//! One layout of the steps in time (`Timeline::build_inner`): `Layout` keeps
//! what the steps laid out so far have done (where the carriers are, the
//! turnstiles, what an `after` can refer to); the steps are laid out in the
//! order of the file (`steps.rs`), then the final checks (`finish.rs`).

use super::*;

impl Timeline {
    /// One layout of the steps. `actual`: the moves as they really happen (from
    /// the physics), by step; a move not in it is laid out without obstacles.
    /// `check`: the final layout, with the checks on wheels, needles and gates.
    pub(super) fn build_inner(sc: &Scene, log: &mut Vec<String>, actual: &HashMap<usize, physics::Done>, check: bool) -> Result<(Timeline, Vec<physics::Plan>), String> {
        let mut lay = Layout::new(sc, log, actual, check)?;
        let max_end = lay.lay_steps()?;
        lay.finish(max_end)
    }
}

/// A carrier whose last move ended in a queue, behind another one.
pub(super) struct Stuck {
    /// The carrier in front.
    pub(super) front: usize,
    /// "line N" of the move.
    pub(super) line: String,
    /// Where it stopped along its route.
    pub(super) s: f64,
}

/// A step laid out: when it starts (a rotation may wait for the wheel), how
/// long it lasts, its event and its line in the log; for a move, when the
/// carrier really sets off.
pub(super) struct Laid {
    pub(super) t0: f64,
    pub(super) dur: f64,
    pub(super) ev: Ev,
    pub(super) desc: String,
    pub(super) depart: Option<f64>,
}

/// The layout in progress.
pub(super) struct Layout<'a> {
    pub(super) sc: &'a Scene,
    /// The moves as the physics played them (by step).
    pub(super) actual: &'a HashMap<usize, physics::Done>,
    /// The final layout: all the checks apply.
    pub(super) check: bool,
    pub(super) log: &'a mut Vec<String>,
    pub(super) carriers: Vec<CarrierPath>,
    /// Where each carrier is on its route: the index of the point its last move goes to.
    pub(super) cur_idx: Vec<usize>,
    pub(super) stuck: Vec<Option<Stuck>>,
    pub(super) wheels: Vec<Wheel>,
    /// "<id>.in", "<id>.needle", "<id>.out" of each turnstile: (wheel, slot).
    pub(super) wheel_pts: HashMap<String, (usize, usize)>,
    /// The pocket (index in its attach list) a carrier is in, while a wheel holds it.
    pub(super) attached: Vec<Option<usize>>,
    /// What happens to the turnstiles, checked in time order at the end.
    pub(super) wheel_recs: Vec<WheelRec>,
    pub(super) events: Vec<Timed>,
    /// What an `after <s> <what>` can refer to: (key, start, end) of the steps so far.
    pub(super) history: Vec<(String, f64, f64)>,
    /// The moves asked for, for the physics.
    pub(super) plans: Vec<physics::Plan>,
}

impl<'a> Layout<'a> {
    fn new(sc: &'a Scene, log: &'a mut Vec<String>, actual: &'a HashMap<usize, physics::Done>, check: bool) -> Result<Layout<'a>, String> {
        let (carriers, cur_idx) = carrier_paths(sc);
        let (wheels, wheel_pts) = turnstiles(sc)?;
        let n = sc.carriers.len();
        Ok(Layout {
            sc,
            actual,
            check,
            log,
            carriers,
            cur_idx,
            stuck: (0..n).map(|_| None).collect(),
            wheels,
            wheel_pts,
            attached: vec![None; n],
            wheel_recs: Vec::new(),
            events: Vec::new(),
            history: Vec::new(),
            plans: Vec::new(),
        })
    }

    /// Lays out the steps in the order of the file; returns when the last one ends.
    fn lay_steps(&mut self) -> Result<f64, String> {
        let sc = self.sc;
        let (mut prev_end, mut max_end) = (0.0f64, 0.0f64);
        for (si, st) in sc.steps.iter().enumerate() {
            let base = self.start(st, prev_end, si == 0)?;
            let Laid { t0, dur, ev, desc, depart } = self.lay(si, st, base + st.delay)?;
            let t1 = t0 + dur;
            // For the steps after it, a pass (or divert) ends when the carrier sets
            // off; any other step when it ends (a move when the carrier arrives).
            let end = if matches!(st.action, Action::Pass { .. }) { depart.unwrap_or(t1) } else { t1 };
            self.record(st, t0, t1, end, ev, desc);
            prev_end = end;
            max_end = max_end.max(t1);
        }
        Ok(max_end)
    }

    /// When a step starts, before its `delay`.
    fn start(&self, st: &Step, prev_end: f64, first: bool) -> Result<f64, String> {
        match &st.start {
            StartMode::At(at) => Ok(*at),
            StartMode::When(Anchor::Event { key }) => self.history.iter().rev().find(|h| h.0 == *key).map(|h| h.2).ok_or_else(|| {
                format!("{}: no '{key}' before this step: 'after <seconds> {key}' refers to one written earlier in the file", st.loc)
            }),
            StartMode::When(Anchor::Arrival { carrier, point }) => self.arrival(st, carrier, point),
            _ if first => Ok(0.0),
            StartMode::After => Ok(prev_end),
        }
    }

    /// When `carrier` gets to `point` (`on <antenna> <carrier>`).
    fn arrival(&self, st: &Step, carrier: &str, point: &str) -> Result<f64, String> {
        let sc = self.sc;
        let ci = sc.need_carrier(carrier, &st.loc)?;
        // A carrier can go round more than once (the return belt of spur2):
        // the last arrival there among the moves written so far.
        let route = &sc.carriers[ci].route;
        let i = route
            .iter()
            .take(self.cur_idx[ci] + 1)
            .rposition(|p| p == point)
            .or_else(|| route.iter().position(|p| p == point))
            .ok_or_else(|| format!("{}: point '{point}' is not on the route of carrier '{carrier}'", st.loc))?;
        let path = &self.carriers[ci];
        path.reaches(path.cum[i]).ok_or_else(|| match &self.stuck[ci] {
            Some(stuck) if i <= self.cur_idx[ci] => self.never_gets_there(st, ci, point, stuck),
            _ => format!("{}: no earlier step moves carrier '{carrier}' to '{point}' (the move must come before this step in the file)", st.loc),
        })
    }

    /// The error of a carrier that never gets to `point`: it is queued behind
    /// another one that nothing sends on.
    fn never_gets_there(&self, st: &Step, ci: usize, point: &str, stuck: &Stuck) -> String {
        let sc = self.sc;
        let name = |p: &str| sc.devices.iter().find(|d| d.kind == DeviceKind::Antenna && d.at.as_deref() == Some(p)).map_or_else(|| format!("'{p}'"), |d| d.id.clone());
        let who = &sc.carriers[stuck.front].id;
        // The one in front waits at the next stop after where this one stopped.
        let stops = sc.stops();
        let route = &sc.carriers[ci].route;
        let path = &self.carriers[ci];
        let there = (0..route.len())
            .find(|&j| path.cum[j] > stuck.s + 1.0 && stops.contains(&route[j]))
            .map_or_else(|| name(&sc.carriers[stuck.front].route[self.cur_idx[stuck.front]]), |j| name(&route[j]));
        format!(
            "{}: '{}' never gets to {}: after {} it stops behind '{who}', which stays on {there} because no step in the file sends it on (e.g. pass {there} {who})",
            st.loc,
            sc.carriers[ci].id,
            name(point),
            stuck.line
        )
    }

    /// Keeps a step laid out: what later steps can refer to, its line in the log,
    /// its event (and the note of a pipette on the analyzer lifeline).
    fn record(&mut self, st: &Step, t0: f64, t1: f64, end: f64, ev: Ev, desc: String) {
        if let Some(id) = &st.id {
            self.history.push((format!("step {id}"), t0, end));
        }
        match &st.action {
            Action::Message { kind: Some(k), carrier, .. } => {
                self.history.push((k.clone(), t0, t1));
                if let Some(c) = carrier {
                    self.history.push((format!("{k} {c}"), t0, t1));
                }
            }
            Action::Pipette { device, .. } => self.history.push((format!("pipette {device}"), t0, t1)),
            Action::Rotate { device } => self.history.push((format!("rotate {device}"), t0, t1)),
            Action::Needle { device, .. } => self.history.push((format!("needle {device}"), t0, t1)),
            _ => {}
        }
        self.log.push(format!("{t0:7.2}s - {t1:7.2}s  {desc}"));
        // The analyzer shows what it is doing on its lifeline.
        let note = match (&ev, &st.action) {
            (Ev::Pipette { device }, Action::Pipette { note, sample, .. }) => self.sc.pipetting_note(*device, note.as_deref(), sample.as_deref()),
            _ => None,
        };
        self.events.push(Timed { t0, t1, ev, at: Some(st.at.clone()) });
        if let Some((ai, text)) = note {
            self.log.push(format!("{t0:7.2}s - {t1:7.2}s  note      {}: {text}", self.sc.actors[ai].id));
            self.events.push(Timed { t0, t1, ev: Ev::Note { actor: ai, text, row: 0 }, at: Some(st.at.clone()) });
        }
    }
}

/// Each carrier as a path along its route (with the lengths), and where on its
/// route it starts.
fn carrier_paths(sc: &Scene) -> (Vec<CarrierPath>, Vec<usize>) {
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
    (carriers, cur_idx)
}

/// The turnstiles, and their "<id>.in" / "<id>.needle" / "<id>.out" points as
/// (wheel, slot). Routes cross a turnstile as "<id>.in", "<id>.out" (the wheel
/// carries the carrier in between); the needle point is not part of a route.
#[allow(clippy::type_complexity)]
fn turnstiles(sc: &Scene) -> Result<(Vec<Wheel>, HashMap<String, (usize, usize)>), String> {
    let mut wheels = Vec::new();
    let mut wheel_pts: HashMap<String, (usize, usize)> = HashMap::new();
    for (di, d) in sc.devices.iter().enumerate().filter(|(_, d)| d.kind == DeviceKind::Turnstile) {
        let wi = wheels.len();
        wheel_pts.insert(format!("{}.in", d.id), (wi, 0));
        wheel_pts.insert(format!("{}.needle", d.id), (wi, 1));
        wheel_pts.insert(format!("{}.out", d.id), (wi, 2));
        wheels.push(Wheel { device: di, cx: d.rect.0, cy: d.rect.1, r: d.rect.2, entry_right: d.entry_right, rots: Vec::new() });
    }
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
    Ok((wheels, wheel_pts))
}
