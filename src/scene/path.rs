//! Where a carrier is in time: its moves along its route.

use super::*;

#[derive(Clone)]
pub struct MoveSeg {
    pub t0: f64,
    pub t1: f64,
    pub s0: f64,
    pub s1: f64,
    pub ease: Ease,
    pub runaway: bool,
}

pub struct CarrierPath {
    pub pts: Vec<(f64, f64)>,
    pub cum: Vec<f64>,
    pub start_s: f64,
    pub moves: Vec<MoveSeg>,
    /// Intervals in which the carrier sits in a turnstile pocket (its position
    /// then follows the wheel instead of the route).
    pub attach: Vec<Attach>,
    pub ghost: bool,
    pub visible: bool,
    /// Fades from `show`/`hide` steps: (t0, t1, visible afterwards), in time order.
    pub fades: Vec<(f64, f64, bool)>,
}

pub struct Attach {
    pub wheel: usize,
    pub pocket: usize,
    /// When the move into the entry position starts (the carrier passes the gate).
    pub t_move: f64,
    pub t0: f64,
    /// When the carrier leaves the exit position (infinity if it never does).
    pub t1: f64,
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
    pub fn moving(&self, t: f64) -> bool {
        self.moves.iter().any(|m| t > m.t0 && t < m.t1 && m.s1 > m.s0 + 1e-9)
    }
    /// Running away: from the first `move` with `runaway = true` to the end of
    /// the last one, including a stop in between.
    pub fn running_away(&self, t: f64) -> bool {
        let first = self.moves.iter().find(|m| m.runaway);
        let last = self.moves.iter().rev().find(|m| m.runaway);
        first.zip(last).is_some_and(|(a, b)| t >= a.t0 && t <= b.t1)
    }
    /// 0 = hidden, 1 = fully visible (fading in between).
    pub fn opacity(&self, t: f64) -> f64 {
        let mut vis = if self.visible { 1.0 } else { 0.0 };
        for &(t0, t1, show) in &self.fades {
            if t < t0 {
                break;
            }
            let target = if show { 1.0 } else { 0.0 };
            if t >= t1 {
                vis = target;
            } else {
                let u = Ease::InOut.apply((t - t0) / (t1 - t0).max(1e-6));
                vis += (target - vis) * u;
                break;
            }
        }
        vis
    }
    /// Physically present (not a ghost) and visible: seen by antennas, opens gates.
    pub fn physical(&self, t: f64) -> bool {
        !self.ghost && self.opacity(t) > 0.0
    }
    /// Distance along the route of point p, if p lies on the route (within 3 px).
    pub(super) fn project(&self, p: (f64, f64)) -> Option<f64> {
        for i in 1..self.pts.len() {
            let (a, b) = (self.pts[i - 1], self.pts[i]);
            let len2 = (b.0 - a.0).powi(2) + (b.1 - a.1).powi(2);
            if len2 == 0.0 {
                continue;
            }
            let u = (((p.0 - a.0) * (b.0 - a.0) + (p.1 - a.1) * (b.1 - a.1)) / len2).clamp(0.0, 1.0);
            let (qx, qy) = (a.0 + (b.0 - a.0) * u, a.1 + (b.1 - a.1) * u);
            if ((p.0 - qx).powi(2) + (p.1 - qy).powi(2)).sqrt() < 3.0 {
                return Some(self.cum[i - 1] + u * len2.sqrt());
            }
        }
        None
    }
    /// When the carrier reaches distance s along its route, among the moves
    /// laid out so far.
    pub fn reaches(&self, s: f64) -> Option<f64> {
        let m = self.moves.iter().find(|m| m.s1 >= s - 1e-9)?;
        if s <= m.s0 {
            return Some(m.t0);
        }
        // Bisection: the position grows monotonically during a move.
        let (mut lo, mut hi) = (m.t0, m.t1);
        for _ in 0..50 {
            let t = (lo + hi) / 2.0;
            if self.s_at(t) < s {
                lo = t;
            } else {
                hi = t;
            }
        }
        Some(hi)
    }
}
