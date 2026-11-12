//! `pass` and `divert`: the move of the carrier stopped on an antenna.

use super::*;

impl Layout<'_> {
    /// The move a `pass` (or `divert`) stands for: the carrier stopped on the
    /// antenna at time t sets off to the next stop of its route, or to its end.
    /// Its route says where it goes; `divert` must be on a divert where the route
    /// turns into a lane, `pass` where it goes straight on. While the layout is
    /// not settled yet (the moves may still change) the carrier written is taken
    /// even if another one is there.
    pub(super) fn pass_move(&self, st: &Step, t: f64) -> Result<Action, String> {
        let Action::Pass { device, divert, carrier: who, speed } = &st.action else { unreachable!("not a pass") };
        let (device, divert, speed, loc) = (device.as_str(), *divert, *speed, &st.loc);
        let (sc, paths, cur_idx, attached, strict) = (self.sc, &self.carriers, &self.cur_idx, &self.attached, self.check);
        let cmd = if divert { "divert" } else { "pass" };
        // Only antennas: the antenna of a device is named instead of the device.
        let d = sc.devices.iter().find(|d| d.id == device && d.kind == DeviceKind::Antenna).ok_or_else(|| {
            match sc.devices.iter().find(|a| a.kind == DeviceKind::Antenna && a.part_of.as_deref() == Some(device)) {
                Some(a) => format!("{loc}: '{cmd} {device}': write the antenna, {cmd} {}", a.id),
                None => format!("{loc}: '{cmd} {device}': not an antenna of this scene"),
            }
        })?;
        let at = d.at.as_deref().unwrap();
        let p = sc.pt(at);
        let stopped_here = |ci: usize| {
            let c = &paths[ci];
            let q = c.pos(t);
            attached[ci].is_none() && !c.moving(t) && ((q.0 - p.0).powi(2) + (q.1 - p.1).powi(2)).sqrt() < 3.0
        };
        // The carrier stopped there (a ghost too, when it is the one written).
        let real = (0..sc.carriers.len()).find(|&ci| stopped_here(ci) && !paths[ci].ghost && paths[ci].opacity(t) > 0.0);
        let ci = match who.as_deref().and_then(|w| sc.carrier_idx(w)) {
            Some(wi) if sc.carriers[wi].ghost && stopped_here(wi) => wi,
            Some(wi) if !strict => wi,
            // The carrier written is stopped there. Another one may look stopped there
            // too, when the step that sends it on is written later in the file.
            Some(wi) if stopped_here(wi) && paths[wi].opacity(t) > 0.0 => wi,
            Some(wi) => match real {
                Some(ci) if ci == wi => ci,
                Some(ci) => return Err(format!("{loc}: {cmd} {device} {}: the carrier stopped there at t={t:.2}s is '{}', not '{}'", sc.carriers[wi].id, sc.carriers[ci].id, sc.carriers[wi].id)),
                None => return Err(format!("{loc}: {cmd} {device} {}: no carrier is stopped on '{at}' at t={t:.2}s", sc.carriers[wi].id)),
            },
            None => real.ok_or_else(|| format!("{loc}: {cmd} {device}: no carrier is stopped on '{at}' at t={t:.2}s"))?,
        };
        let c = &sc.carriers[ci];
        let from = cur_idx[ci];
        // A divert (or its antenna) can turn the carrier into a lane that starts
        // there: the next point of its route is the first one of that lane.
        let owner = d.part_of.as_deref().and_then(|o| sc.devices.iter().find(|x| x.id == o)).unwrap_or(d);
        let turn = (owner.kind == DeviceKind::Divert)
            .then(|| c.route.get(from + 1))
            .flatten()
            .and_then(|next| {
                lane_paths(&sc.points, &sc.lanes)
                    .into_iter()
                    .zip(&sc.lanes)
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
        let stops = sc.stops();
        let to = c.route[from + 1..]
            .iter()
            .find(|p| stops.contains(p))
            .or(c.route.last())
            .filter(|_| from + 1 < c.route.len())
            .ok_or_else(|| format!("{loc}: {cmd} {device}: carrier '{}' is at the end of its route", c.id))?;
        Ok(Action::Move { carrier: c.id.clone(), to: to.clone(), ease: Ease::Linear, speed, runaway: false })
    }
}
