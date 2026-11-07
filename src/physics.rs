//! Carrier physics: two carriers never take the same space. A carrier that
//! reaches another one stops in contact with it and moves on as soon as the
//! other one makes room; carriers only go forward along their routes.
//!
//! The steps of a scene ask for moves ("C2 starts towards the sampling point at
//! t"); `simulate` plays all of them together and returns where each carrier
//! really is, and when it really arrives.

use crate::scene::{CarrierPath, Ease, MoveSeg, CARRIER_R};
use std::collections::{HashMap, VecDeque};

/// The part of the panel where carriers are on the track: outside it (spawn and
/// exit points) they may share a place.
pub const TRACK_X: std::ops::Range<f64> = 30.0..950.0;

/// Simulation step (s).
const DT: f64 = 1.0 / 120.0;
/// How far ahead (px) a moving carrier claims its way, and the spacing of the
/// points that describe it.
const LOOKAHEAD: f64 = 150.0;
const SWEEP_STEP: f64 = 4.0;

/// A move asked for by a step.
pub struct Plan {
    /// Index of the step.
    pub step: usize,
    pub carrier: usize,
    /// When the step asks the carrier to start.
    pub t_req: f64,
    pub s0: f64,
    pub s1: f64,
    /// Duration without obstacles.
    pub dur: f64,
    pub ease: Ease,
    pub runaway: bool,
    /// The move ends in a turnstile pocket: from then on the wheel carries it.
    pub into_wheel: bool,
}

/// A move as it really happens.
#[derive(Clone)]
pub struct Done {
    pub start: f64,
    /// None: the carrier never gets there (still queued at the end).
    pub end: Option<f64>,
    /// The last instant the carrier moved during this move.
    pub last_motion: f64,
    pub segs: Vec<MoveSeg>,
    /// Stops behind another carrier: (from, to, carrier in front).
    pub waits: Vec<(f64, f64, usize)>,
}

impl Done {
    /// Same timing (to tell when the layout has settled).
    pub fn same(&self, o: &Done) -> bool {
        let eq = |a: f64, b: f64| (a - b).abs() < 1e-6;
        eq(self.start, o.start)
            && match (self.end, o.end) {
                (Some(a), Some(b)) => eq(a, b),
                (None, None) => eq(self.last_motion, o.last_motion),
                _ => false,
            }
    }
}

struct Run {
    plan: usize,
    ts: f64,
    tau: f64,
    samples: Vec<(f64, f64)>,
    blocked: bool,
    waits: Vec<(f64, f64, usize)>,
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// Linear pieces through the samples, merging pieces at the same speed.
fn pieces(samples: &[(f64, f64)], runaway: bool) -> Vec<MoveSeg> {
    let mut segs: Vec<MoveSeg> = Vec::new();
    for w in samples.windows(2) {
        let ((t0, s0), (t1, s1)) = (w[0], w[1]);
        if t1 - t0 < 1e-9 {
            continue;
        }
        if let Some(last) = segs.last_mut() {
            let v_last = (last.s1 - last.s0) / (last.t1 - last.t0);
            let v = (s1 - s0) / (t1 - t0);
            if (v - v_last).abs() < 1e-6 {
                last.t1 = t1;
                last.s1 = s1;
                continue;
            }
        }
        segs.push(MoveSeg { t0, t1, s0, s1, ease: Ease::Linear, runaway });
    }
    segs
}

/// Plays all the moves together. `paths` gives routes, visibility and ghosts
/// (their moves are not used); the result is keyed by step index.
pub fn simulate(paths: &[CarrierPath], plans: &[Plan]) -> HashMap<usize, Done> {
    let n = paths.len();
    let mut s: Vec<f64> = paths.iter().map(|p| p.start_s).collect();
    let mut queue: Vec<VecDeque<usize>> = vec![VecDeque::new(); n];
    for (pi, p) in plans.iter().enumerate() {
        queue[p.carrier].push_back(pi);
    }
    let mut run: Vec<Option<Run>> = (0..n).map(|_| None).collect();
    let mut free_at = vec![f64::NEG_INFINITY; n];
    let mut attached = vec![false; n];
    let mut out = HashMap::new();
    let limit = plans.iter().map(|p| p.t_req).fold(0.0, f64::max) + plans.iter().map(|p| p.dur).sum::<f64>() + 60.0;
    // Who goes first, for each pair of carriers whose ways cross: decided when
    // the crossing starts, kept until it is over.
    let mut decided: HashMap<(usize, usize), bool> = HashMap::new();
    let mut t = 0.0;
    while t < limit && (queue.iter().any(|q| !q.is_empty()) || run.iter().any(Option::is_some)) {
        let t_next = t + DT;
        // Who takes space at time t (also off the panel: carriers waiting at the
        // spawn point go in in turn).
        let obstacles: Vec<Option<(f64, f64)>> = (0..n)
            .map(|ci| {
                let p = &paths[ci];
                let q = p.pos_at_s(s[ci]);
                (!p.ghost && !attached[ci] && p.opacity(t) > 0.0).then_some(q)
            })
            .collect();
        // The way ahead of each moving carrier, up to where its move ends.
        let way: Vec<Vec<(f64, f64)>> = (0..n)
            .map(|ci| match (&run[ci], obstacles[ci]) {
                (Some(r), Some(_)) => {
                    let end = plans[r.plan].s1.min(s[ci] + LOOKAHEAD);
                    let k = ((end - s[ci]) / SWEEP_STEP).ceil().max(0.0) as usize;
                    (0..=k).map(|i| paths[ci].pos_at_s((s[ci] + i as f64 * SWEEP_STEP).min(end))).collect()
                }
                _ => Vec::new(),
            })
            .collect();
        // Two moving carriers whose ways cross: the one already on the way of the
        // other goes first (the other one follows it); otherwise the one nearer
        // to the crossing, and on a tie the one declared first. The other one keeps
        // clear of its way. first[i][j]: i goes before j.
        let mut first = vec![vec![false; n]; n];
        for i in 0..n {
            for j in i + 1..n {
                let hit = |a: &[(f64, f64)], b: &[(f64, f64)]| a.iter().position(|p| b.iter().any(|q| dist(*p, *q) < 2.0 * CARRIER_R));
                let (ai, aj) = (hit(&way[i], &way[j]), hit(&way[j], &way[i]));
                let (Some(ai), Some(aj)) = (ai, aj) else {
                    decided.remove(&(i, j));
                    continue;
                };
                let i_first = *decided.entry((i, j)).or_insert_with(|| {
                    let on_way = |w: &[(f64, f64)], q: Option<(f64, f64)>| q.is_some_and(|q| w.iter().any(|p| dist(*p, q) < 2.0 * CARRIER_R));
                    match (on_way(&way[j], obstacles[i]), on_way(&way[i], obstacles[j])) {
                        (true, false) => true,
                        (false, true) => false,
                        _ => ai <= aj,
                    }
                });
                if i_first {
                    first[i][j] = true;
                } else {
                    first[j][i] = true;
                }
            }
        }
        for ci in 0..n {
            let path = &paths[ci];
            let solid = !path.ghost && path.opacity(t) > 0.0;
            // The carrier in front, if moving from s_from to s_to would go into it.
            let blocker = |s_from: f64, s_to: f64| -> Option<usize> {
                let (p0, p1) = (path.pos_at_s(s_from), path.pos_at_s(s_to));
                if !solid || !TRACK_X.contains(&p1.0) {
                    return None;
                }
                let closer = |q: (f64, f64)| {
                    let d = dist(p1, q);
                    d < 2.0 * CARRIER_R - 1e-6 && d < dist(p0, q) - 1e-9
                };
                // Two carriers on the same spot (they start there): the one declared
                // first goes, the other one waits for room.
                let same_spot = |j: usize, q: (f64, f64)| j < ci && dist(p0, q) < 1.0 && dist(p1, q) < 2.0 * CARRIER_R - 1e-6;
                (0..n).filter(|&j| j != ci && !first[ci][j]).find(|&j| {
                    obstacles[j].is_some_and(|q| closer(q) || same_spot(j, q) || (first[j][ci] && way[j].iter().any(|w| closer(*w))))
                })
            };
            let mut u = t;
            loop {
                if run[ci].is_none() {
                    let Some(&pi) = queue[ci].front() else { break };
                    let ts = plans[pi].t_req.max(free_at[ci]);
                    if ts >= t_next {
                        break;
                    }
                    queue[ci].pop_front();
                    attached[ci] = false;
                    s[ci] = plans[pi].s0;
                    run[ci] = Some(Run { plan: pi, ts, tau: 0.0, samples: vec![(ts, plans[pi].s0)], blocked: false, waits: Vec::new() });
                }
                let r = run[ci].as_mut().unwrap();
                let p = &plans[r.plan];
                let dur = p.dur.max(1e-6);
                let profile = |tau: f64| p.s0 + (p.s1 - p.s0) * p.ease.apply(tau / dur);
                let from = u.max(r.ts);
                let tau_before = r.tau;
                let tau_c = (r.tau + (t_next - from)).min(dur);
                let s_c = profile(tau_c);
                match blocker(s[ci], s_c) {
                    None => {
                        r.tau = tau_c;
                        s[ci] = s_c;
                    }
                    Some(b) => {
                        // As far as possible: in contact with the carrier in front.
                        let (mut lo, mut hi) = (r.tau, tau_c);
                        for _ in 0..40 {
                            let mid = (lo + hi) / 2.0;
                            if blocker(s[ci], profile(mid)).is_some() {
                                hi = mid;
                            } else {
                                lo = mid;
                            }
                        }
                        // Already touching: stay (no creeping by rounding).
                        if profile(lo) > s[ci] + 1e-6 {
                            r.tau = lo;
                            s[ci] = profile(lo);
                        }
                        r.blocked = true;
                        match r.waits.last_mut() {
                            Some(w) if w.2 == b && (w.1 - t).abs() < 1e-9 => w.1 = t_next,
                            _ => r.waits.push((t, t_next, b)),
                        }
                    }
                }
                if r.tau >= dur - 1e-12 {
                    let t_end = from + (dur - tau_before);
                    r.samples.push((t_end, p.s1));
                    let segs = if r.blocked {
                        pieces(&r.samples, p.runaway)
                    } else {
                        vec![MoveSeg { t0: r.ts, t1: r.ts + dur, s0: p.s0, s1: p.s1, ease: p.ease, runaway: p.runaway }]
                    };
                    let waits = std::mem::take(&mut r.waits);
                    out.insert(p.step, Done { start: r.ts, end: Some(t_end), last_motion: t_end, segs, waits });
                    free_at[ci] = t_end;
                    attached[ci] = p.into_wheel;
                    run[ci] = None;
                    u = t_end;
                    continue;
                }
                r.samples.push((t_next, s[ci]));
                break;
            }
        }
        t = t_next;
    }
    // Moves that never end: the carrier stays queued.
    for r in run.into_iter().flatten() {
        let p = &plans[r.plan];
        let last_motion = r.samples.windows(2).filter(|w| w[1].1 > w[0].1 + 1e-9).map(|w| w[1].0).next_back().unwrap_or(r.ts);
        out.insert(p.step, Done { start: r.ts, end: None, last_motion, segs: pieces(&r.samples, p.runaway), waits: r.waits });
    }
    for q in &queue {
        for &pi in q {
            let p = &plans[pi];
            out.insert(p.step, Done { start: p.t_req, end: None, last_motion: p.t_req, segs: Vec::new(), waits: Vec::new() });
        }
    }
    out
}
