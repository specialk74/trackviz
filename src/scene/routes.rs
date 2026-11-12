//! The way of each carrier (from its steps), the stops along the lanes, and
//! carriers sent straight on when their pit-lane is full.

use super::*;

impl Scene {
    /// The ways of the carriers written without `route`, from their steps in the
    /// order of the file: from `start`; a `move` adds the shortest way along the
    /// lanes to its point; `pass` goes straight on, `divert` into the lane that
    /// starts on the antenna, then along the lanes to the next stop (an antenna
    /// with a gate, a turnstile entry) or to the end. All the carriers together, so
    /// that the first wrong step in the file is the one reported, with where the
    /// carrier is and who is on the antenna instead.
    pub(super) fn derive_routes(&self) -> Result<Vec<Vec<String>>, String> {
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
    pub(super) fn overflow(&self, tl: &Timeline) -> Option<usize> {
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
    pub(super) fn with_straight(&self, ci: usize) -> Result<Scene, String> {
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
    pub(super) fn stops(&self) -> Vec<String> {
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
}
