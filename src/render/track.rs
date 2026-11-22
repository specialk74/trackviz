//! The track panel: zones, lanes, devices, operators, carriers and the probe
//! of the analyzer, in this order (later ones on top).

use super::*;
use super::devices::device_state;
use super::operator::operator;
use crate::scene::{AnalyzerState, DeviceKind, Ev, LaneKind, Scene, Timeline};
use std::fmt::Write;

/// What every part of the track is drawn for: the scene, its timeline, the instant.
#[derive(Clone, Copy)]
pub(super) struct Ctx<'a> {
    pub(super) sc: &'a Scene,
    pub(super) tl: &'a Timeline,
    pub(super) t: f64,
}

/// The track at time t; without the carriers (and the probe) for the layout image.
pub(super) fn track(o: &mut String, sc: &Scene, tl: &Timeline, t: f64, carriers: bool) {
    let c = Ctx { sc, tl, t };
    zones(o, &c);
    lanes(o, sc);
    for (di, d) in sc.devices.iter().enumerate() {
        devices::device(o, &c, di, d);
    }
    operators(o, &c);
    if !carriers {
        return;
    }
    carriers::carriers(o, &c);
    probe::probes(o, &c);
    timers(o, &c);
}

/// Fast clocks (`timer`), on top of everything; they stay a moment after they stop.
fn timers(o: &mut String, c: &Ctx) {
    let Ctx { sc, tl, t } = *c;
    for e in tl.events.iter().filter(|e| t >= e.t0 && t <= e.t1 + super::timer::LINGER + 0.5) {
        if let Ev::Timer { device, value, limit, ref label, pos } = e.ev {
            super::timer::timer(o, &sc.devices[device], pos, value, limit, label, e.t0, e.t1, t);
        }
    }
}

/// Zones (e.g. the interface module) first, as background. A `status` turns
/// them red (error) or green (ready).
fn zones(o: &mut String, c: &Ctx) {
    let Ctx { sc, tl, t } = *c;
        for (di, d) in sc.devices.iter().enumerate().filter(|(_, d)| d.kind == DeviceKind::Zone) {
            let (x, y, w, h) = d.rect;
            let st = device_state(tl, di, t);
            let (fill, stroke, text, sw) = match st.map(|s| s.0) {
                Some(AnalyzerState::Error | AnalyzerState::Warning) => ("#FEF2F2", "#DC2626", "#B91C1C", 3.0),
                Some(AnalyzerState::Ready) => ("#F0FDF4", "#16A34A", "#15803D", 3.0),
                Some(AnalyzerState::Manual) => ("#FFFBEB", "#D97706", "#B45309", 3.0),
                _ => ("#F8FAFC", "#94A3B8", MUTED, 2.0),
            };
            // A status change pulses for 1.5 s; a warning keeps pulsing, slower.
            if let Some((s, t0)) = st.filter(|s| s.0 != AnalyzerState::Normal) {
                let a = if t - t0 < 1.5 {
                    Some(0.3 + 0.3 * ((t - t0) / 1.5 * std::f64::consts::PI * 4.0).sin().abs())
                } else if s == AnalyzerState::Warning {
                    Some(0.1 + 0.25 * ((t - t0) * std::f64::consts::PI / 1.2).sin().abs())
                } else {
                    None
                };
                if let Some(a) = a {
                    let _ = write!(o, r#"<rect x="{}" y="{}" width="{}" height="{}" rx="22" fill="none" stroke="{stroke}" stroke-width="10" opacity="{a:.2}"/>"#, x - 5.0, y - 5.0, w + 10.0, h + 10.0);
                }
            }
            let _ = write!(
                o,
                r##"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="18" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" stroke-dasharray="10 8"/><text x="{}" y="{}" font-size="16" font-weight="600" fill="{text}">{}</text>"##,
                x + 16.0, y + 28.0, esc(&d.label)
            );
            // A badge after the name, except for normal.
            if let Some((badge, col)) = st.and_then(|s| devices::state_badge(s.0)) {
                let bx = x + 16.0 + 9.0 * d.label.chars().count() as f64 + 14.0;
                let _ = write!(o, r##"<g transform="translate({bx} {})">"##, y + 10.0);
                devices::status_badge(o, 56.0, 0.0, badge, col);
                o.push_str("</g>");
            }
        }
}

/// Lanes: links below, then pit and main lanes, with their direction chevrons.
fn lanes(o: &mut String, sc: &Scene) {
    for kind in [LaneKind::Link, LaneKind::Pit, LaneKind::Main] {
        for l in sc.lanes.iter().filter(|l| l.kind == kind) {
            let pts: Vec<(f64, f64)> = l.points.iter().map(|p| sc.pt(p)).collect();
            let poly: String = pts.iter().map(|(x, y)| format!("{x},{y} ")).collect();
            let _ = write!(
                o,
                r##"<polyline points="{poly}" fill="none" stroke="#AEB8C5" stroke-width="36" stroke-linejoin="round"/><polyline points="{poly}" fill="none" stroke="#DDE3EA" stroke-width="30.6" stroke-linejoin="round"/>"##
            );
            // Direction chevrons
            for w in pts.windows(2) {
                let (a, b) = (w[0], w[1]);
                let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
                let ang = (b.1 - a.1).atan2(b.0 - a.0).to_degrees();
                let n = (len / 70.0).floor() as i32;
                for k in 1..n {
                    let u = k as f64 / n as f64;
                    let (x, y) = (a.0 + (b.0 - a.0) * u, a.1 + (b.1 - a.1) * u);
                    let _ = write!(
                        o,
                        r##"<path d="M-4,-7 L4,0 L-4,7" fill="none" stroke="#A3AEBB" stroke-width="2.5" stroke-linecap="round" transform="translate({x:.1},{y:.1}) rotate({ang:.1})"/>"##
                    );
                }
            }
            if !l.label.is_empty() {
                let (lx, ly) = l.label_pos.unwrap_or((pts[0].0, pts[0].1 + 46.0));
                let _ = write!(o, r##"<text x="{lx}" y="{ly}" font-size="17" font-weight="600" fill="#475569">{}</text>"##, esc(&l.label));
            }
        }
    }
}

/// Operators working on an analyzer (they walk in from the right edge of the panel).
fn operators(o: &mut String, c: &Ctx) {
    let Ctx { sc, tl, t } = *c;
    for e in tl.events.iter().filter(|e| t >= e.t0 && t <= e.t1) {
        if let Ev::Operator { device, tubes } = e.ev {
            operator(o, &sc.devices[device], tubes, t - e.t0, e.t1 - e.t0, t);
        }
    }
}
