//! The lanes: their points in the direction of the carriers, as a graph, and
//! the direction of travel through a point.

use super::*;
use std::collections::HashMap;

pub(super) fn sc_pt(points: &HashMap<String, Point>, id: &str) -> (f64, f64) {
    let p = &points[id];
    (p.x, p.y)
}

/// Direction of travel of the lane through point p (lanes list their points in
/// the direction of the carriers); right to left if p is on no lane.
pub(super) fn lane_dir(points: &HashMap<String, Point>, lanes: &[Lane], p: (f64, f64)) -> (f64, f64) {
    for l in lanes {
        let pts: Vec<(f64, f64)> = l.points.iter().map(|id| sc_pt(points, id)).collect();
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
            if len < 1e-9 {
                continue;
            }
            let (ux, uy) = ((b.0 - a.0) / len, (b.1 - a.1) / len);
            let along = (p.0 - a.0) * ux + (p.1 - a.1) * uy;
            let off = ((p.0 - a.0) * uy - (p.1 - a.1) * ux).abs();
            // On the segment, not at its end (at a corner the next segment counts).
            if off < 3.0 && along >= -1e-9 && along < len - 1e-9 {
                return (ux, uy);
            }
        }
    }
    (-1.0, 0.0)
}

/// The points of each lane in the direction of the carriers, with the points
/// that lie on its segments (the vertices alone skip queue positions, sampling
/// points, ...). Generated gate points are left out.
pub(super) fn lane_paths(points: &HashMap<String, Point>, lanes: &[Lane]) -> Vec<(String, Vec<String>)> {
    lanes
        .iter()
        .map(|l| {
            let mut out: Vec<String> = Vec::new();
            for w in l.points.windows(2) {
                let (a, b) = (sc_pt(points, &w[0]), sc_pt(points, &w[1]));
                let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
                let mut on: Vec<(f64, &String)> = points
                    .iter()
                    .filter(|(id, _)| !id.ends_with(".gate") && **id != w[0] && **id != w[1])
                    .filter_map(|(id, p)| {
                        let (ux, uy) = ((b.0 - a.0) / len, (b.1 - a.1) / len);
                        let along = (p.x - a.0) * ux + (p.y - a.1) * uy;
                        let off = ((p.x - a.0) * uy - (p.y - a.1) * ux).abs();
                        (off < 3.0 && along > 0.0 && along < len).then_some((along, id))
                    })
                    .collect();
                on.sort_by(|x, y| x.0.total_cmp(&y.0));
                if out.last() != Some(&w[0]) {
                    out.push(w[0].clone());
                }
                out.extend(on.into_iter().map(|x| x.1.clone()));
                out.push(w[1].clone());
            }
            (l.id.clone(), out)
        })
        .collect()
}

/// The lanes as a graph: from each point, the next points along the lanes (in
/// their direction), with the length and the lane; through a turnstile from
/// "<id>.in" to "<id>.out" (lane "").
pub(super) struct LaneGraph {
    edges: HashMap<String, Vec<(String, f64, String)>>,
}

impl LaneGraph {
    pub(super) fn new(points: &HashMap<String, Point>, lanes: &[Lane], devices: &[Device]) -> LaneGraph {
        let mut edges: HashMap<String, Vec<(String, f64, String)>> = HashMap::new();
        for (lane, path) in lane_paths(points, lanes) {
            for w in path.windows(2) {
                let (p, q) = (sc_pt(points, &w[0]), sc_pt(points, &w[1]));
                let d = ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt();
                edges.entry(w[0].clone()).or_default().push((w[1].clone(), d, lane.clone()));
            }
        }
        for d in devices.iter().filter(|d| d.kind == DeviceKind::Turnstile) {
            edges.entry(format!("{}.in", d.id)).or_default().push((format!("{}.out", d.id), 0.0, String::new()));
        }
        LaneGraph { edges }
    }

    pub(super) fn out(&self, p: &str) -> &[(String, f64, String)] {
        self.edges.get(p).map_or(&[][..], |v| v.as_slice())
    }

    /// Shortest way from a to b along the lanes (Dijkstra; the graphs are small),
    /// with the lane of its last stretch.
    pub(super) fn way(&self, a: &str, b: &str) -> Option<(Vec<String>, String)> {
        // Point -> (distance, previous point and lane).
        type Best<'a> = HashMap<&'a str, (f64, Option<(&'a str, &'a str)>)>;
        let mut best: Best = HashMap::from([(a, (0.0, None))]);
        let mut open = vec![a];
        while let Some(k) = (0..open.len()).min_by(|&x, &y| best[open[x]].0.total_cmp(&best[open[y]].0)) {
            let n = open.swap_remove(k);
            if n == b {
                let mut out = vec![b.to_string()];
                let mut cur = b;
                let last = best[b].1.map_or(String::new(), |x| x.1.to_string());
                while let Some((prev, _)) = best[cur].1 {
                    out.push(prev.to_string());
                    cur = prev;
                }
                out.reverse();
                return Some((out, last));
            }
            let d = best[n].0;
            for (m, w, lane) in self.out(n) {
                if best.get(m.as_str()).is_none_or(|x| d + w < x.0 - 1e-9) {
                    best.insert(m, (d + w, Some((n, lane))));
                    open.push(m);
                }
            }
        }
        None
    }
}

/// A route as written: points and lane ids, in order. Each lane id stands for its
/// points; between two consecutive places the shortest way along the lanes (in
/// their direction, and through a turnstile from "<id>.in" to "<id>.out") is
/// filled in. "main_in,pit,main_out" is main_in, nsd, pit_in, ..., pit_out, merge,
/// main_out. Two points that no lane joins are joined by a straight line.
pub(super) fn expand_route(written: &[String], points: &HashMap<String, Point>, lanes: &[Lane], devices: &[Device]) -> Result<Vec<String>, String> {
    let paths = lane_paths(points, lanes);
    let graph = LaneGraph::new(points, lanes, devices);
    let way = |a: &str, b: &str| graph.way(a, b).map(|w| w.0);
    let mut route: Vec<String> = Vec::new();
    for tok in written {
        let places: Vec<String> = if points.contains_key(tok) {
            vec![tok.clone()]
        } else if let Some((_, p)) = paths.iter().find(|(id, _)| id == tok) {
            p.clone()
        } else {
            return Err(format!("route: '{tok}' is neither a point nor a lane"));
        };
        for p in places {
            match route.last() {
                None => route.push(p),
                Some(last) if *last == p => {}
                // No lane between them: straight from one to the other.
                Some(last) => match way(last, &p) {
                    Some(w) => route.extend(w.into_iter().skip(1)),
                    None => route.push(p),
                },
            }
        }
    }
    Ok(route)
}
