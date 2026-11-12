//! After the steps: the turnstiles played in time order, the rows of the
//! sequence diagram, the checks on wheels and needles, the devices lit by their
//! antennas and the gates opening.

use super::*;
use super::layout::Layout;
use crate::physics;
use crate::scene::wheels::simulate_wheels;
use crate::scene::{DeviceKind, CARRIER_R, READ_LIGHT, TS_RIM};

impl Layout<'_> {
    /// The timeline of the steps laid out (they end by `max_end`). Before the
    /// layout is final only what the physics needs.
    pub(super) fn finish(mut self, max_end: f64) -> Result<(Timeline, Vec<physics::Plan>), String> {
        if !self.check {
            let tl = Timeline { events: self.events, carriers: self.carriers, wheels: self.wheels, gates: Vec::new(), rows: 0, duration: max_end, log: Vec::new(), lines: Vec::new() };
            return Ok((tl, self.plans));
        }
        let sc = self.sc;
        simulate_wheels(sc, &self.wheels, &mut self.carriers, std::mem::take(&mut self.wheel_recs))?;
        let rows = diagram_rows(&mut self.events);
        self.no_entry_while_sampling()?;
        self.wheels_turn_clear()?;
        self.needle_down_holds()?;
        self.antenna_reads(max_end);
        let gates = self.gates();
        let lines = source_lines(&self.events);
        let tl = Timeline { events: self.events, carriers: self.carriers, wheels: self.wheels, gates, rows, duration: max_end + sc.video.tail, log: std::mem::take(self.log), lines };
        Ok((tl, self.plans))
    }

    /// The gate in front of a turnstile holds carriers while the analyzer is
    /// sampling there: nobody may drive into the wheel during a pipette.
    fn no_entry_while_sampling(&self) -> Result<(), String> {
        let sc = self.sc;
        for (ci, c) in self.carriers.iter().enumerate() {
            for a in &c.attach {
                let di = self.wheels[a.wheel].device;
                for e in &self.events {
                    if matches!(e.ev, Ev::Pipette { device } if device == di) && a.t_move < e.t1 - 1e-9 && a.t0 > e.t0 + 1e-9 {
                        return Err(format!(
                            "carrier '{}' drives into turnstile '{}' ({:.2}s - {:.2}s) while the analyzer is sampling there ({:.2}s - {:.2}s): it must wait at the gate",
                            sc.carriers[ci].id, sc.devices[di].id, a.t_move, a.t0, e.t0, e.t1
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// A turnstile can't turn while a carrier outside its pockets (one that has
    /// just left, or is arriving) still overlaps the wheel.
    fn wheels_turn_clear(&self) -> Result<(), String> {
        let sc = self.sc;
        for w in &self.wheels {
            let clear = w.r + TS_RIM + CARRIER_R;
            for &(r0, r1) in &w.rots {
                let n = ((r1 - r0) / 0.05).ceil().max(1.0) as usize;
                for k in 0..=n {
                    let t = r0 + (r1 - r0) * k as f64 / n as f64;
                    for (ci, c) in self.carriers.iter().enumerate() {
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
        Ok(())
    }

    /// While the needle is down (from the start of a `needle down` to the end of
    /// the next `needle up`) the wheel can't turn and the carrier under the
    /// needle can't leave.
    fn needle_down_holds(&self) -> Result<(), String> {
        let sc = self.sc;
        let mut needle_down: Vec<(usize, f64, f64)> = Vec::new();
        let mut needles: Vec<&Timed> = self.events.iter().filter(|e| matches!(e.ev, Ev::Needle { .. })).collect();
        needles.sort_by(|a, b| a.t0.total_cmp(&b.t0));
        for (k, e) in needles.iter().enumerate() {
            if let Ev::Needle { device, down: true } = e.ev {
                let up = needles[k + 1..].iter().find(|u| matches!(u.ev, Ev::Needle { device: d, down: false } if d == device));
                needle_down.push((device, e.t0, up.map_or(f64::INFINITY, |u| u.t1)));
            }
        }
        for &(di, a, b) in &needle_down {
            let d = &sc.devices[di];
            if let Some(w) = self.wheels.iter().find(|w| w.device == di) {
                if let Some(r) = w.rots.iter().find(|r| r.0 < b - 1e-9 && r.1 > a + 1e-9) {
                    return Err(format!("turnstile '{}' rotates ({:.2}s - {:.2}s) while the needle is down ({a:.2}s - {b:.2}s)", d.id, r.0, r.1));
                }
            } else {
                let p = sc.pt(d.at.as_deref().unwrap());
                for (ci, c) in self.carriers.iter().enumerate() {
                    for m in c.moves.iter().filter(|m| m.s1 > m.s0 + 1e-9) {
                        let at = c.pos_at_s(m.s0);
                        if m.t0 > a - 1e-9 && m.t0 < b - 1e-9 && ((at.0 - p.0).powi(2) + (at.1 - p.1).powi(2)).sqrt() < 3.0 {
                            return Err(format!("carrier '{}' leaves '{}' at {:.2}s while the needle is down ({a:.2}s - {b:.2}s)", sc.carriers[ci].id, d.id, m.t0));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// A device lights up while its antenna reads a carrier: from when the
    /// carrier gets on the antenna until it leaves it, at least READ_LIGHT
    /// seconds (a carrier passing the NSD is read in a moment). The NSD and the
    /// diverts carrying the antenna light up, or an antenna on its own. Ghosts
    /// and hidden carriers are not read; sampling points don't light up.
    fn antenna_reads(&mut self, max_end: f64) {
        let sc = self.sc;
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
                let path = &self.carriers[ci];
                let start = c.route.iter().position(|p| *p == c.start).unwrap_or(0);
                for (k, _) in c.route.iter().enumerate().filter(|(k, p)| p.as_str() == at && *k != start) {
                    if let Some(t0) = path.reaches(path.cum[k]).filter(|&t0| path.opacity(t0) > 0.0) {
                        let off = path.reaches(path.cum[k] + 1.0).unwrap_or(max_end);
                        let t1 = off.max(t0 + READ_LIGHT);
                        self.log.push(format!("{t0:7.2}s - {t1:7.2}s  highlight {}  ({} read by {})", sc.devices[target].id, c.id, d.id));
                        self.events.push(Timed { t0, t1, ev: Ev::Highlight { device: target }, at: None });
                    }
                }
            }
        }
    }

    /// Gates: open from shortly before a carrier reaches them until it is through.
    fn gates(&self) -> Vec<(usize, Vec<(f64, f64)>)> {
        let sc = self.sc;
        let mut gates = Vec::new();
        for (di, d) in sc.devices.iter().enumerate().filter(|(_, d)| d.kind == DeviceKind::Gate) {
            let g = sc.pt(d.at.as_deref().unwrap());
            let mut iv = Vec::new();
            for c in self.carriers.iter().filter(|c| !c.ghost) {
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
        gates
    }
}

/// Rows of the sequence diagram in time order (same start: file order), in half
/// rows: a label takes a row, and half a row for each more line. Returns the
/// height of the diagram.
fn diagram_rows(events: &mut [Timed]) -> usize {
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
    rows
}
