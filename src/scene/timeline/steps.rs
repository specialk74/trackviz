//! Each kind of step laid out: its start, how long it lasts, its event and its
//! line in the log. Moves, rotations and fades change the layout.

use super::*;

impl Layout<'_> {
    /// Lays out step `si`, asked to start at `t0`.
    pub(super) fn lay(&mut self, si: usize, st: &Step, t0: f64) -> Result<Laid, String> {
        let sc = self.sc;
        // `pass`: a move of the carrier stopped on the antenna, to its next stop.
        let passed;
        let action = match &st.action {
            Action::Pass { .. } => {
                passed = match self.pass_move(st, t0) {
                    Ok(a) => a,
                    // The carrier it was for went straight on.
                    Err(_) if !sc.straight.is_empty() => Action::Wait,
                    Err(e) => return Err(e),
                };
                &passed
            }
            a => a,
        };
        let simple = |dur: f64, ev: Ev, desc: String| -> Result<Laid, String> { Ok(Laid { t0, dur, ev, desc, depart: None }) };
        let d = st.duration;
        let loc = &st.loc;
        match action {
            Action::Caption { title, text } => simple(d.unwrap_or(0.0), Ev::Caption { title: title.clone(), text: text.clone() }, format!("caption   {title}")),
            Action::Move { .. } => self.lay_move(si, st, action, t0),
            Action::Message { from, to, text, params, reply, carrier, color, needle, .. } => {
                let (fi, ti) = (sc.need_actor(from, loc)?, sc.need_actor(to, loc)?);
                if fi == ti {
                    return Err(format!("{loc}: message from an actor to itself is not supported; use a note"));
                }
                let ci = carrier.as_deref().map(|c| sc.need_carrier(c, loc)).transpose()?;
                let text = subst(text, sc, ci, *needle).map_err(|e| format!("{loc}: {e}"))?;
                let params = subst(params, sc, ci, *needle).map_err(|e| format!("{loc}: {e}"))?;
                let desc = format!("message   {from} -> {to}  {text}  {params}");
                let ev = Ev::Message { from: fi, to: ti, text, params, reply: *reply, row: 0, color: color.clone() };
                simple(d.unwrap_or(sc.video.message), ev, desc)
            }
            Action::Note { actor, text } => {
                let ai = sc.need_actor(actor, loc)?;
                simple(d.unwrap_or(1.5), Ev::Note { actor: ai, text: text.clone(), row: 0 }, format!("note      {actor}: {text}"))
            }
            Action::Pipette { device, .. } => {
                let di = sc.need_probe(device, loc, "pipette")?;
                simple(d.unwrap_or(3.0), Ev::Pipette { device: di }, format!("pipette   at {device}"))
            }
            Action::Highlight { device } => {
                let di = sc.need_device(device, loc, "highlight", &[])?;
                simple(d.unwrap_or(1.5), Ev::Highlight { device: di }, format!("highlight {device}"))
            }
            Action::Rotate { device } => self.rotate(st, device, t0),
            Action::Visibility { carrier, show } => self.fade(st, carrier, *show, t0),
            Action::Status { device, state } => {
                let di = sc.need_device(device, loc, "status", &[(DeviceKind::Analyzer, "analyzer"), (DeviceKind::Zone, "zone"), (DeviceKind::Sampling, "sampling")])?;
                simple(d.unwrap_or(0.0), Ev::Status { device: di, state: *state }, format!("status    {device} {}", state.name()))
            }
            Action::Needle { device, down } => {
                let di = sc.need_probe(device, loc, "needle")?;
                let dir = if *down { "down" } else { "up" };
                simple(d.unwrap_or(0.9), Ev::Needle { device: di, down: *down }, format!("needle    {device} {dir}"))
            }
            Action::Operator { device } => {
                let di = sc.need_device(device, loc, "operator", &[(DeviceKind::Analyzer, "analyzer")])?;
                let d = d.unwrap_or(7.0);
                if d < 2.0 {
                    return Err(format!("{loc}: operator needs a duration of at least 2 s (walk in and out)"));
                }
                simple(d, Ev::Operator { device: di }, format!("operator  at {device}"))
            }
            Action::Wait => simple(d.unwrap_or(0.0), Ev::Wait, "wait".to_string()),
            Action::Pass { .. } => unreachable!("a pass is laid out as a move"),
        }
    }

    /// A move (or a pass): the carrier sets off along its route to the point, as
    /// the physics played it if it did; into or out of a turnstile pocket.
    fn lay_move(&mut self, si: usize, st: &Step, action: &Action, t0: f64) -> Result<Laid, String> {
        let Action::Move { carrier, to, ease, speed, runaway } = action else { unreachable!("not a move") };
        let sc = self.sc;
        let ci = sc.need_carrier(carrier, &st.loc)?;
        let c = &sc.carriers[ci];
        let mut from = self.cur_idx[ci];
        let leaving = self.attached[ci];
        if leaving.is_some() {
            from += 1; // the route point after "<id>.in" is "<id>.out"
        }
        let ti = c
            .route
            .iter()
            .enumerate()
            .skip(from + 1)
            .find(|(_, p)| *p == to)
            .map(|(i, _)| i)
            .ok_or_else(|| format!("{}: point '{to}' is not ahead of carrier '{carrier}' on its route", st.loc))?;
        if let Some(p) = c.route[from..ti].iter().find(|p| self.wheel_pts.get(*p).is_some_and(|w| w.1 == 0)) {
            return Err(format!("{}: carrier '{carrier}' would drive through the turnstile; move it to '{p}' and use 'rotate'", st.loc));
        }
        let path = &mut self.carriers[ci];
        let (s0, s1) = (path.cum[from], path.cum[ti]);
        let d = st.duration.unwrap_or((s1 - s0) / speed.unwrap_or(c.speed) * ease.factor());
        let into = self.wheel_pts.get(to).filter(|w| w.1 == 0).map(|w| w.0);
        self.plans.push(physics::Plan { step: si, carrier: ci, t_req: t0, s0, s1, dur: d, ease: *ease, runaway: *runaway, into_wheel: into.is_some() });
        // A move asked while the carrier is still moving starts when it stops.
        let free = path.moves.last().map_or(f64::NEG_INFINITY, |m| m.t1);
        let done = self.actual.get(&si).cloned().unwrap_or_else(|| {
            let start = t0.max(free);
            physics::Done { start, end: Some(start + d), last_motion: start + d, segs: vec![MoveSeg { t0: start, t1: start + d, s0, s1, ease: *ease, runaway: *runaway }], waits: Vec::new() }
        });
        // Leaving a turnstile (from the exit position: checked by simulate_wheels).
        if let Some(ai) = leaving {
            let wheel = path.attach[ai].wheel;
            self.wheel_recs.push(WheelRec { t: done.start, wheel, ev: WheelEv::Leave(ci, ai), loc: st.loc.clone() });
            path.attach[ai].t1 = done.start;
            self.attached[ci] = None;
        }
        let depart = Some(done.segs.first().map_or(done.start, |s| s.t0));
        path.moves.extend(done.segs.iter().cloned());
        self.cur_idx[ci] = ti;
        // Entering a turnstile: the carrier drops into the pocket at the entry.
        // The pocket is assigned later, in time order (simulate_wheels).
        if let (Some(wi), Some(t_in)) = (into, done.end) {
            path.attach.push(Attach { wheel: wi, pocket: 0, t_move: done.start, t0: t_in, t1: f64::INFINITY });
            let ai = path.attach.len() - 1;
            self.attached[ci] = Some(ai);
            self.wheel_recs.push(WheelRec { t: t_in, wheel: wi, ev: WheelEv::Arrive(ci, ai), loc: st.loc.clone() });
        }
        self.stuck[ci] = match (done.end, done.waits.first()) {
            (None, Some(w)) => Some(Stuck { front: w.2, line: format!("line {}", st.loc.rsplit("line ").next().unwrap_or(&st.loc)), s: done.segs.last().map_or(0.0, |g| g.s1) }),
            _ => None,
        };
        let mut tag = if *runaway { "  (run-away)".to_string() } else { String::new() };
        let waited: f64 = done.waits.iter().map(|w| w.1 - w.0).sum();
        if let Some(w) = done.waits.first() {
            let who = &sc.carriers[w.2].id;
            tag += &match done.end {
                Some(_) => format!("  (waits {waited:.1}s behind {who})"),
                None => format!("  (stops behind {who} and never gets there)"),
            };
        }
        Ok(Laid { t0, dur: done.end.unwrap_or(done.last_motion) - t0, ev: Ev::Wait, desc: format!("move      {carrier} -> {to}{tag}"), depart })
    }

    /// A rotation of a turnstile: it waits until the wheel is free (the previous
    /// rotation over, the carrier driving into the entry in its pocket, the
    /// others clear).
    fn rotate(&mut self, st: &Step, device: &str, asked: f64) -> Result<Laid, String> {
        let di = self.sc.need_device(device, &st.loc, "rotate", &[(DeviceKind::Turnstile, "turnstile")])?;
        let wi = self.wheels.iter().position(|w| w.device == di).unwrap();
        let d = st.duration.unwrap_or(2.0);
        let t0 = wheel_free_at(&self.wheels[wi], wi, &self.carriers, asked, d);
        let tag = if t0 > asked + 1e-6 { format!("  (waits {:.1}s for the wheel to be clear)", t0 - asked) } else { String::new() };
        self.wheels[wi].rots.push((t0, t0 + d));
        self.wheel_recs.push(WheelRec { t: t0, wheel: wi, ev: WheelEv::Rotate(t0 + d), loc: st.loc.clone() });
        Ok(Laid { t0, dur: d, ev: Ev::Wait, desc: format!("rotate    {device}{tag}"), depart: None })
    }

    /// `show` / `hide`: the carrier fades in or out.
    fn fade(&mut self, st: &Step, carrier: &str, show: bool, t0: f64) -> Result<Laid, String> {
        let ci = self.sc.need_carrier(carrier, &st.loc)?;
        let d = st.duration.unwrap_or(0.5);
        let fades = &mut self.carriers[ci].fades;
        if let Some(last) = fades.last() {
            if t0 < last.1 - 1e-9 {
                return Err(format!("{}: carrier '{carrier}' is still fading at t={t0:.2}s (show/hide steps of a carrier must be in time order)", st.loc));
            }
        }
        fades.push((t0, t0 + d, show));
        let verb = if show { "show" } else { "hide" };
        Ok(Laid { t0, dur: d, ev: Ev::Wait, desc: format!("{verb:<9} {carrier}"), depart: None })
    }
}
