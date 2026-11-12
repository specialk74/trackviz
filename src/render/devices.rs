//! The devices on the track, one function for each kind; a highlighted device
//! glows (`glow`: its opacity).

use super::*;
use super::track::Ctx;
use crate::scene::{AnalyzerState, Device, DeviceKind, Ease, Ev, Scene, Timeline, GATE_TIME, TS_RIM};
use std::fmt::Write;

/// Device `di`, with its glow while a `highlight` is on.
pub(super) fn device(o: &mut String, c: &Ctx, di: usize, d: &Device) {
    let Ctx { tl, t, .. } = *c;
    let hl = active(tl, t, |e| matches!(e, Ev::Highlight { device } if *device == di));
    let glow = hl.map(|e| {
        let u = (t - e.t0) / (e.t1 - e.t0).max(1e-6);
        0.35 + 0.35 * (u * std::f64::consts::PI * 4.0).sin().abs()
    });
    match d.kind {
        DeviceKind::Analyzer => analyzer(o, c, di, d),
        DeviceKind::Nsd | DeviceKind::Divert => nsd_or_divert(o, c, d, glow),
        DeviceKind::Sampling => sampling(o, c.sc, d),
        DeviceKind::Turnstile => turnstile(o, c, di, d, glow),
        DeviceKind::Gate => gate(o, c, di, d, glow),
        DeviceKind::Antenna if d.visible => antenna(o, c, d, glow),
        DeviceKind::Antenna => {}
        DeviceKind::Zone => {}
    }
}

/// The analyzer: its status, and with several needles a band for each one.
fn analyzer(o: &mut String, c: &Ctx, di: usize, d: &Device) {
    let Ctx { sc, tl, t } = *c;
    let (x, y, w, h) = d.rect;
    let st = device_state(tl, di, t);
    let (fill, stroke, badge) = match st.map(|s| s.0) {
        Some(AnalyzerState::Error) => ("#FEE2E2", "#DC2626", Some(("ERROR", "#DC2626"))),
        Some(AnalyzerState::Ready) => ("#DCFCE7", "#16A34A", Some(("READY", "#16A34A"))),
        _ => ("#E8EDF3", "#64748B", None),
    };
    // A status change pulses for 1.5 s.
    if let Some((_, t0)) = st.filter(|s| s.0 != AnalyzerState::Normal && t - s.1 < 1.5) {
        let a = 0.35 + 0.35 * ((t - t0) / 1.5 * std::f64::consts::PI * 4.0).sin().abs();
        let _ = write!(o, r#"<rect x="{}" y="{}" width="{}" height="{}" rx="16" fill="none" stroke="{stroke}" stroke-width="10" opacity="{a:.2}"/>"#, x - 6.0, y - 6.0, w + 12.0, h + 12.0);
    }
    let sw = if badge.is_some() { 3.5 } else { 2.0 };
    let ly = if badge.is_some() { y + h / 2.0 - 4.0 } else { y + h / 2.0 + 9.0 };
    let _ = write!(o, r##"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="12" fill="{fill}" stroke="{stroke}" stroke-width="{sw}"/>"##);
    // Several needles: a band of the instrument for each one, which
    // shows the status of that needle.
    let bands = needle_bands(sc, di);
    if bands.len() > 1 {
        for (n, &(sd, y0, y1)) in bands.iter().enumerate() {
            let band = device_state(tl, sd, t);
            let (fill, stroke, badge) = match band.map(|s| s.0) {
                Some(AnalyzerState::Error) => ("#FEE2E2", "#DC2626", Some(("ERROR", "#DC2626"))),
                Some(AnalyzerState::Ready) => ("#DCFCE7", "#16A34A", Some(("READY", "#16A34A"))),
                _ => ("none", "none", None),
            };
            if badge.is_some() {
                let _ = write!(o, r##"<rect x="{x}" y="{y0}" width="{w}" height="{}" rx="12" fill="{fill}" stroke="{stroke}" stroke-width="3.5"/>"##, y1 - y0);
            }
            let (_, sy) = sc.pt(sc.devices[sd].at.as_deref().unwrap());
            let _ = write!(o, r##"<text x="{}" y="{}" font-size="15" font-weight="600" text-anchor="middle" fill="{MUTED}">Needle {}</text>"##, x + w / 2.0, sy - 34.0, n + 1);
            if let Some((text, col)) = badge {
                status_badge(o, x + w / 2.0, sy + 20.0, text, col);
            }
        }
    }
    if bands.len() > 1 {
        // The title sits on the edge between two bands.
        let _ = write!(o, r##"<rect x="{}" y="{}" width="170" height="38" rx="10" fill="#E8EDF3"/>"##, x + w / 2.0 - 85.0, ly - 28.0);
    }
    let _ = write!(o, r##"<text x="{}" y="{ly}" font-size="26" font-weight="700" text-anchor="middle" fill="#334155">{}</text>"##, x + w / 2.0, esc(&d.label));
    if let Some((text, col)) = badge {
        status_badge(o, x + w / 2.0, ly + 14.0, text, col);
    }
}

/// The NSD or a divert, with its label; a divert turns blue while it reads the
/// carrier stopped on it.
fn nsd_or_divert(o: &mut String, c: &Ctx, d: &Device, glow: Option<f64>) {
    let Ctx { sc, tl, t } = *c;
    let (x, y) = sc.pt(d.at.as_deref().unwrap());
    if let Some(a) = glow {
        let _ = write!(o, r#"<rect x="{}" y="{}" width="84" height="76" rx="12" fill="none" stroke="{ACCENT}" stroke-width="10" opacity="{a:.2}"/>"#, x - 42.0, y - 38.0);
    }
    // A diversion turns blue while it reads the carrier stopped on it.
    let read = d.kind == DeviceKind::Divert && carrier_stopped_at(sc, tl, (x, y), t);
    let col = if glow.is_some() { ACCENT } else if read { "#2563EB" } else { "#475569" };
    let (lx, ly) = d.label_pos.unwrap_or((x, y + 58.0));
    let _ = write!(
        o,
        r##"<rect x="{}" y="{}" width="72" height="64" rx="9" fill="#FFFFFF" fill-opacity="0.35" stroke="{col}" stroke-width="3"/><text x="{lx}" y="{ly}" font-size="17" font-weight="700" text-anchor="middle" fill="{col}">{}</text>"##,
        x - 36.0, y - 32.0, esc(&d.label)
    );
}

/// A sampling point: a dashed circle under the needle, with its label.
fn sampling(o: &mut String, sc: &Scene, d: &Device) {
    let (x, y) = sc.pt(d.at.as_deref().unwrap());
    let (lx, ly) = d.label_pos.unwrap_or((x, y + 56.0));
    let _ = write!(
        o,
        r##"<circle cx="{x}" cy="{y}" r="30" fill="none" stroke="#DC2626" stroke-width="2.5" stroke-dasharray="6 5"/><text x="{lx}" y="{ly}" font-size="16" font-weight="600" text-anchor="middle" fill="#B91C1C">{}</text>"##,
        esc(&d.label)
    );
}

/// A turnstile: the wheel with its pockets at time t, the fixed sampling position.
fn turnstile(o: &mut String, c: &Ctx, di: usize, d: &Device, glow: Option<f64>) {
    let Ctx { sc, tl, t } = *c;
    let w = tl.wheel_of_device(di).unwrap();
    let (cx, cy, r) = (w.cx, w.cy, w.r);
    let disk = r + TS_RIM;
    if let Some(a) = glow {
        let _ = write!(o, r#"<circle cx="{cx}" cy="{cy}" r="{}" fill="none" stroke="{ACCENT}" stroke-width="10" opacity="{a:.2}"/>"#, disk + 6.0);
    }
    let _ = write!(o, r##"<circle cx="{cx}" cy="{cy}" r="{disk}" fill="#E2E8F0" stroke="#64748B" stroke-width="2.5"/>"##);
    for p in 0..3 {
        let (px, py) = w.pocket_pos(p, t);
        let _ = write!(o, r##"<line x1="{cx}" y1="{cy}" x2="{px:.1}" y2="{py:.1}" stroke="#94A3B8" stroke-width="6" stroke-linecap="round"/>"##);
    }
    for p in 0..3 {
        let (px, py) = w.pocket_pos(p, t);
        let _ = write!(o, r##"<circle cx="{px:.1}" cy="{py:.1}" r="24.3" fill="#F8FAFC" stroke="#94A3B8" stroke-width="2"/>"##);
    }
    let _ = write!(o, r##"<circle cx="{cx}" cy="{cy}" r="12" fill="#64748B"/>"##);
    // The sampling position (under the needle) is fixed.
    let (nx, ny) = sc.pt(d.at.as_deref().unwrap());
    let _ = write!(o, r##"<circle cx="{nx}" cy="{ny}" r="31" fill="none" stroke="#DC2626" stroke-width="2.5" stroke-dasharray="6 5"/>"##);
    let col = if glow.is_some() { ACCENT } else { "#475569" };
    let _ = write!(o, r##"<text x="{cx}" y="{}" font-size="17" font-weight="700" text-anchor="middle" fill="{col}">{}</text>"##, cy + disk + 26.0, esc(&d.label));
}

/// A gate: post beside the lane, arm across it. Seen from above, the lifting
/// arm gets shorter as it opens.
fn gate(o: &mut String, c: &Ctx, di: usize, d: &Device, glow: Option<f64>) {
    let Ctx { sc, tl, t } = *c;
    let (gx, gy) = sc.pt(d.at.as_deref().unwrap());
    let (dx, dy) = sc.lane_dir((gx, gy));
    // The post stands on the right of the direction of travel (above a
    // lane that goes right to left).
    let (nx, ny) = (-dy, dx);
    let iv = tl.gates.iter().find(|g| g.0 == di).map(|g| g.1.as_slice()).unwrap_or(&[]);
    let open = iv
        .iter()
        .map(|&(a, b)| Ease::InOut.apply(((t - (a - GATE_TIME)) / GATE_TIME).min((b + GATE_TIME - t) / GATE_TIME)))
        .fold(0.0, f64::max);
    let (px, py) = (gx + nx * 27.0, gy + ny * 27.0);
    let len = 54.0 * (1.0 - open);
    if let Some(a) = glow {
        let _ = write!(o, r#"<circle cx="{gx}" cy="{gy}" r="38" fill="none" stroke="{ACCENT}" stroke-width="8" opacity="{a:.2}"/>"#);
    }
    if len > 0.5 {
        let (ex, ey) = (px - nx * len, py - ny * len);
        let _ = write!(
            o,
            r##"<line x1="{px:.1}" y1="{py:.1}" x2="{ex:.1}" y2="{ey:.1}" stroke="#DC2626" stroke-width="7" stroke-linecap="round"/><line x1="{px:.1}" y1="{py:.1}" x2="{ex:.1}" y2="{ey:.1}" stroke="#FFFFFF" stroke-width="3" stroke-dasharray="6 6"/>"##
        );
    }
    let _ = write!(o, r##"<rect x="{:.1}" y="{:.1}" width="14" height="14" rx="3" fill="#334155"/>"##, px + nx * 5.0 - 7.0, py + ny * 5.0 - 7.0);
    if !d.label.is_empty() {
        let col = if glow.is_some() { ACCENT } else { "#475569" };
        let _ = write!(o, r##"<text x="{:.1}" y="{:.1}" font-size="14" font-weight="700" text-anchor="middle" fill="{col}">{}</text>"##, gx - nx * 44.0, gy - ny * 44.0 + 5.0, esc(&d.label));
    }
}

/// An antenna: a flat reader on the lane, under the carriers. While it reads a
/// carrier, waves spread around it.
fn antenna(o: &mut String, c: &Ctx, d: &Device, glow: Option<f64>) {
    let Ctx { sc, tl, t } = *c;
    let (ax, ay) = sc.pt(d.at.as_deref().unwrap());
    let (dx, dy) = sc.lane_dir((ax, ay));
    let deg = dy.atan2(dx).to_degrees();
    let reading = carrier_over(sc, tl, (ax, ay), t);
    if let Some(a) = glow {
        let _ = write!(o, r#"<circle cx="{ax}" cy="{ay}" r="30" fill="none" stroke="{ACCENT}" stroke-width="8" opacity="{a:.2}"/>"#);
    }
    if reading {
        for k in 0..2 {
            let phase = (t * 1.5 + k as f64 * 0.5).rem_euclid(1.0);
            let _ = write!(
                o,
                r##"<circle cx="{ax}" cy="{ay}" r="{:.1}" fill="none" stroke="#2563EB" stroke-width="2.5" opacity="{:.2}"/>"##,
                DRAW_R + 4.0 + 18.0 * phase,
                0.7 * (1.0 - phase)
            );
        }
    }
    let coil = if reading { "#2563EB" } else { "#64748B" };
    let _ = write!(
        o,
        r##"<g transform="translate({ax} {ay}) rotate({deg:.1})"><rect x="-8" y="-13.5" width="16" height="27" rx="4" fill="#CBD5E1" stroke="#334155" stroke-width="2"/><rect x="-4.5" y="-9" width="9" height="18" rx="2" fill="none" stroke="{coil}" stroke-width="1.8"/></g>"##
    );
}

/// ERROR / READY pill centered on x, with its top at y.
fn status_badge(o: &mut String, x: f64, y: f64, text: &str, col: &str) {
    let _ = write!(
        o,
        r##"<rect x="{}" y="{y}" width="96" height="28" rx="14" fill="{col}"/><text x="{x}" y="{}" font-size="16" font-weight="700" letter-spacing="1.5" text-anchor="middle" fill="#FFFFFF">{text}</text>"##,
        x - 48.0, y + 19.5
    );
}

/// Sampling devices (the needles, in file order) with the vertical band
/// (y0, y1) of the analyzer that belongs to each one.
pub(super) fn needle_bands(sc: &Scene, an: usize) -> Vec<(usize, f64, f64)> {
    let (_, ay, _, ah) = sc.devices[an].rect;
    let ids: Vec<usize> = sc.devices.iter().enumerate().filter(|(_, d)| d.kind == DeviceKind::Sampling).map(|(i, _)| i).collect();
    let y_of = |i: usize| sc.pt(sc.devices[i].at.as_deref().unwrap()).1;
    let edge = |i: usize, sign: f64| {
        // Half way to the nearest needle in that direction, or the analyzer edge.
        let y = y_of(i);
        ids.iter()
            .map(|&o| y_of(o))
            .filter(|&v| (v - y) * sign > 0.0)
            .min_by(|a, b| (a - y).abs().total_cmp(&(b - y).abs()))
            .map_or(if sign < 0.0 { ay } else { ay + ah }, |v| (v + y) / 2.0)
    };
    ids.iter().map(|&i| (i, edge(i, -1.0), edge(i, 1.0))).collect()
}

/// A physical carrier is standing still on point p.
fn carrier_stopped_at(sc: &Scene, tl: &Timeline, p: (f64, f64), t: f64) -> bool {
    (0..sc.carriers.len()).any(|ci| {
        let (x, y) = tl.carrier_pos(ci, t);
        tl.carriers[ci].physical(t) && !tl.carriers[ci].moving(t) && ((x - p.0).powi(2) + (y - p.1).powi(2)).sqrt() < 3.0
    })
}

/// A carrier, stopped or passing, is over point p.
fn carrier_over(sc: &Scene, tl: &Timeline, p: (f64, f64), t: f64) -> bool {
    (0..sc.carriers.len()).any(|ci| {
        let (x, y) = tl.carrier_pos(ci, t);
        tl.carriers[ci].physical(t) && ((x - p.0).powi(2) + (y - p.1).powi(2)).sqrt() < 10.0
    })
}

/// Last `status` of a device (analyzer or zone) at time t, with the time it was set.
pub(super) fn device_state(tl: &Timeline, di: usize, t: f64) -> Option<(AnalyzerState, f64)> {
    tl.events
        .iter()
        .filter(|e| e.t0 <= t)
        .filter_map(|e| match e.ev {
            Ev::Status { device, state } if device == di => Some((state, e.t0)),
            _ => None,
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
}
