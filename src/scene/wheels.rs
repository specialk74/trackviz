//! Turnstiles: the wheel, its pockets, and the checks on its use in time order.

use super::*;

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
pub(super) enum WheelEv {
    /// A carrier reaches the entry: (carrier, index in its attach list).
    Arrive(usize, usize),
    /// A carrier leaves from the exit.
    Leave(usize, usize),
    /// A rotation starts; it ends at the given time.
    Rotate(f64),
}

pub(super) struct WheelRec {
    pub(super) t: f64,
    pub(super) wheel: usize,
    pub(super) ev: WheelEv,
    pub(super) loc: String,
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
pub(super) fn simulate_wheels(sc: &Scene, wheels: &[Wheel], carriers: &mut [CarrierPath], mut recs: Vec<WheelRec>) -> Result<(), String> {
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

/// The first instant from `t` at which wheel `wi` can make a rotation of `d`
/// seconds: its previous rotation is over, no carrier is on its way into the
/// entry pocket, and every carrier outside the pockets stays clear of the wheel
/// for the whole rotation. Only the moves laid out so far are known; if the
/// wheel never gets free, `t` (the final checks then say why).
pub(super) fn wheel_free_at(w: &Wheel, wi: usize, carriers: &[CarrierPath], t: f64, d: f64) -> f64 {
    const STEP: f64 = 0.02;
    let clear = w.r + TS_RIM + CARRIER_R;
    let mut s = w.rots.last().map_or(t, |r| t.max(r.1));
    let limit = s + 120.0;
    'search: while s < limit {
        for c in carriers {
            if let Some(a) = c.attach.iter().find(|a| a.wheel == wi && s >= a.t_move - 1e-9 && s < a.t0 - 1e-9) {
                s = a.t0;
                continue 'search;
            }
        }
        let n = (d / STEP).ceil().max(1.0) as usize;
        for k in 0..=n {
            let u = s + d * k as f64 / n as f64;
            for c in carriers {
                let on_wheel = c.attach.iter().any(|a| u >= a.t0 && u <= a.t1);
                if on_wheel || c.ghost || c.opacity(u) <= 0.0 {
                    continue;
                }
                let (x, y) = c.pos(u);
                if ((x - w.cx).powi(2) + (y - w.cy).powi(2)).sqrt() < clear - 1e-6 {
                    s = u + STEP;
                    continue 'search;
                }
            }
        }
        return s;
    }
    t
}
