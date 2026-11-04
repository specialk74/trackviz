//! Frame renderer: builds one SVG per instant `t`.
//! All drawing happens in a fixed 1920x1080 coordinate space; the output size
//! from [video] only scales it.

use crate::scene::*;
use std::fmt::Write;

const FONT: &str = "Inter, 'DejaVu Sans', sans-serif";
const MONO: &str = "'DejaVu Sans Mono', monospace";
const INK: &str = "#0F172A";
const MUTED: &str = "#64748B";
const ACCENT: &str = "#F59E0B";

// Layout of the two panels and the caption band (in the 1920x1080 space).
const TRACK: (f64, f64, f64, f64) = (30.0, 120.0, 920.0, 765.0);
const SEQ: (f64, f64, f64, f64) = (970.0, 120.0, 920.0, 765.0);
const CAPTION_Y: f64 = 905.0;
/// Turnstile: distance from the pocket centers to the rim of the wheel.
const TS_RIM: f64 = 32.0;
/// Sequence diagram: below this row height the diagram scrolls instead of
/// shrinking, with rows of SCROLL_ROW_H.
const MIN_ROW_H: f64 = 38.0;
const SCROLL_ROW_H: f64 = 46.0;

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn wrap(text: &str, max: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for para in text.split('\n') {
        let mut cur = String::new();
        for w in para.split_whitespace() {
            if !cur.is_empty() && cur.chars().count() + 1 + w.chars().count() > max {
                lines.push(std::mem::take(&mut cur));
            }
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(w);
        }
        lines.push(cur);
    }
    lines
}

fn clamp01(x: f64) -> f64 {
    x.clamp(0.0, 1.0)
}

pub fn frame(sc: &Scene, tl: &Timeline, t: f64) -> String {
    let mut o = String::with_capacity(64 * 1024);
    let v = &sc.video;
    let _ = write!(
        o,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 1920 1080" font-family="{FONT}">"#,
        v.width, v.height
    );
    o.push_str(r##"<rect width="1920" height="1080" fill="#F1F4F8"/>"##);

    // Header
    let _ = write!(o, r#"<text x="40" y="62" font-size="32" font-weight="600" fill="{INK}">{}</text>"#, esc(&v.title));
    let _ = write!(o, r#"<text x="40" y="96" font-size="19" fill="{MUTED}">{}</text>"#, esc(&v.subtitle));
    if v.show_clock {
        let _ = write!(
            o,
            r#"<text x="1880" y="70" font-size="24" text-anchor="end" font-family="{MONO}" fill="{MUTED}">t = {:5.1} s</text>"#,
            t
        );
    }

    for (r, title) in [(TRACK, "TRACK · TOP VIEW"), (SEQ, "MESSAGE SEQUENCE")] {
        let _ = write!(
            o,
            r##"<rect x="{}" y="{}" width="{}" height="{}" rx="16" fill="#FFFFFF" stroke="#DCE2EA"/><text x="{}" y="{}" font-size="14" font-weight="600" letter-spacing="1.5" fill="#94A3B8">{title}</text>"##,
            r.0, r.1, r.2, r.3, r.0 + 22.0, r.1 + 30.0
        );
    }

    let _ = write!(o, r#"<clipPath id="trk"><rect x="{}" y="{}" width="{}" height="{}" rx="16"/></clipPath><g clip-path="url(#trk)">"#, TRACK.0, TRACK.1, TRACK.2, TRACK.3);
    track(&mut o, sc, tl, t);
    o.push_str("</g>");
    sequence(&mut o, sc, tl, t);
    caption(&mut o, tl, t);
    o.push_str("</svg>");
    o
}

fn active(tl: &Timeline, t: f64, pred: impl Fn(&Ev) -> bool) -> Option<&Timed> {
    tl.events.iter().find(|e| t >= e.t0 && t <= e.t1 && pred(&e.ev))
}

fn track(o: &mut String, sc: &Scene, tl: &Timeline, t: f64) {
    // Zones (e.g. the interface module) first, as background.
    for d in sc.devices.iter().filter(|d| d.kind == DeviceKind::Zone) {
        let (x, y, w, h) = d.rect;
        let _ = write!(
            o,
            r##"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="18" fill="#F8FAFC" stroke="#94A3B8" stroke-width="2" stroke-dasharray="10 8"/><text x="{}" y="{}" font-size="16" font-weight="600" fill="{MUTED}">{}</text>"##,
            x + 16.0, y + 28.0, esc(&d.label)
        );
    }

    // Lanes: links below, then pit and main lanes.
    for kind in [LaneKind::Link, LaneKind::Pit, LaneKind::Main] {
        for l in sc.lanes.iter().filter(|l| l.kind == kind) {
            let pts: Vec<(f64, f64)> = l.points.iter().map(|p| sc.pt(p)).collect();
            let poly: String = pts.iter().map(|(x, y)| format!("{x},{y} ")).collect();
            let _ = write!(
                o,
                r##"<polyline points="{poly}" fill="none" stroke="#AEB8C5" stroke-width="40" stroke-linejoin="round"/><polyline points="{poly}" fill="none" stroke="#DDE3EA" stroke-width="34" stroke-linejoin="round"/>"##
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

    // Devices
    for (di, d) in sc.devices.iter().enumerate() {
        let hl = active(tl, t, |e| matches!(e, Ev::Highlight { device } if *device == di));
        let glow = hl.map(|e| {
            let u = (t - e.t0) / (e.t1 - e.t0).max(1e-6);
            0.35 + 0.35 * (u * std::f64::consts::PI * 4.0).sin().abs()
        });
        match d.kind {
            DeviceKind::Analyzer => {
                let (x, y, w, h) = d.rect;
                let st = analyzer_state(tl, di, t);
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
                let _ = write!(
                    o,
                    r##"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="12" fill="{fill}" stroke="{stroke}" stroke-width="{sw}"/><text x="{}" y="{ly}" font-size="26" font-weight="700" text-anchor="middle" fill="#334155">{}</text>"##,
                    x + w / 2.0, esc(&d.label)
                );
                if let Some((text, col)) = badge {
                    let _ = write!(
                        o,
                        r##"<rect x="{}" y="{}" width="96" height="28" rx="14" fill="{col}"/><text x="{}" y="{}" font-size="16" font-weight="700" letter-spacing="1.5" text-anchor="middle" fill="#FFFFFF">{text}</text>"##,
                        x + w / 2.0 - 48.0, ly + 14.0, x + w / 2.0, ly + 33.5
                    );
                }
            }
            DeviceKind::Nsd => {
                let (x, y) = sc.pt(d.at.as_deref().unwrap());
                if let Some(a) = glow {
                    let _ = write!(o, r#"<rect x="{}" y="{}" width="84" height="76" rx="12" fill="none" stroke="{ACCENT}" stroke-width="10" opacity="{a:.2}"/>"#, x - 42.0, y - 38.0);
                }
                let col = if glow.is_some() { ACCENT } else { "#475569" };
                let _ = write!(
                    o,
                    r##"<rect x="{}" y="{}" width="72" height="64" rx="9" fill="#FFFFFF" fill-opacity="0.35" stroke="{col}" stroke-width="3"/><text x="{x}" y="{}" font-size="17" font-weight="700" text-anchor="middle" fill="{col}">{}</text>"##,
                    x - 36.0, y - 32.0, y + 58.0, esc(&d.label)
                );
            }
            DeviceKind::Sampling => {
                let (x, y) = sc.pt(d.at.as_deref().unwrap());
                let _ = write!(
                    o,
                    r##"<circle cx="{x}" cy="{y}" r="30" fill="none" stroke="#DC2626" stroke-width="2.5" stroke-dasharray="6 5"/><text x="{x}" y="{}" font-size="16" font-weight="600" text-anchor="middle" fill="#B91C1C">{}</text>"##,
                    y + 56.0, esc(&d.label)
                );
            }
            DeviceKind::Turnstile => {
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
                    let _ = write!(o, r##"<circle cx="{px:.1}" cy="{py:.1}" r="27" fill="#F8FAFC" stroke="#94A3B8" stroke-width="2"/>"##);
                }
                let _ = write!(o, r##"<circle cx="{cx}" cy="{cy}" r="12" fill="#64748B"/>"##);
                // The sampling position (under the needle) is fixed.
                let (nx, ny) = sc.pt(d.at.as_deref().unwrap());
                let _ = write!(o, r##"<circle cx="{nx}" cy="{ny}" r="31" fill="none" stroke="#DC2626" stroke-width="2.5" stroke-dasharray="6 5"/>"##);
                let col = if glow.is_some() { ACCENT } else { "#475569" };
                let _ = write!(o, r##"<text x="{cx}" y="{}" font-size="17" font-weight="700" text-anchor="middle" fill="{col}">{}</text>"##, cy + disk + 26.0, esc(&d.label));
            }
            DeviceKind::Gate => {
                // Drawn for a horizontal lane: post above the lane, arm across it.
                // Seen from above, the lifting arm gets shorter as it opens.
                let (gx, gy) = sc.pt(d.at.as_deref().unwrap());
                let iv = tl.gates.iter().find(|g| g.0 == di).map(|g| g.1.as_slice()).unwrap_or(&[]);
                let open = iv
                    .iter()
                    .map(|&(a, b)| Ease::InOut.apply(((t - (a - 0.4)) / 0.4).min((b + 0.4 - t) / 0.4)))
                    .fold(0.0, f64::max);
                let (py, len) = (gy - 30.0, 60.0 * (1.0 - open));
                if let Some(a) = glow {
                    let _ = write!(o, r#"<rect x="{}" y="{}" width="28" height="72" rx="8" fill="none" stroke="{ACCENT}" stroke-width="8" opacity="{a:.2}"/>"#, gx - 14.0, gy - 40.0);
                }
                if len > 0.5 {
                    let _ = write!(
                        o,
                        r##"<line x1="{gx}" y1="{py}" x2="{gx}" y2="{:.1}" stroke="#DC2626" stroke-width="7" stroke-linecap="round"/><line x1="{gx}" y1="{py}" x2="{gx}" y2="{:.1}" stroke="#FFFFFF" stroke-width="3" stroke-dasharray="6 6"/>"##,
                        py + len, py + len
                    );
                }
                let col = if glow.is_some() { ACCENT } else { "#475569" };
                let _ = write!(
                    o,
                    r##"<rect x="{}" y="{}" width="14" height="14" rx="3" fill="#334155"/><text x="{gx}" y="{}" font-size="14" font-weight="700" text-anchor="middle" fill="{col}">{}</text>"##,
                    gx - 7.0, py - 12.0, gy + 44.0, esc(&d.label)
                );
            }
            DeviceKind::Antenna => {
                // Small reader above the lane; waves while a carrier stands on its point.
                let (ax, ay) = sc.pt(d.at.as_deref().unwrap());
                let by = ay - 36.0;
                let reading = (0..sc.carriers.len()).any(|ci| {
                    if !tl.carriers[ci].physical(t) {
                        return false;
                    }
                    let (x, y) = tl.carrier_pos(ci, t);
                    ((x - ax).powi(2) + (y - ay).powi(2)).sqrt() < 3.0 && !tl.carriers[ci].moving(t)
                });
                if let Some(a) = glow {
                    let _ = write!(o, r#"<circle cx="{ax}" cy="{by}" r="22" fill="none" stroke="{ACCENT}" stroke-width="8" opacity="{a:.2}"/>"#);
                }
                let wave = if reading { "#2563EB" } else { "#94A3B8" };
                for k in 1..=2 {
                    let r = 7.0 * k as f64;
                    let op = if reading { 0.35 + 0.65 * ((t * 3.0 - k as f64 * 0.3).sin() * 0.5 + 0.5) } else { 1.0 };
                    // Arcs on both sides of the reader, opening sideways.
                    let _ = write!(
                        o,
                        r##"<path d="M{:.1},{:.1} A{r},{r} 0 0 0 {:.1},{:.1} M{:.1},{:.1} A{r},{r} 0 0 1 {:.1},{:.1}" fill="none" stroke="{wave}" stroke-width="2.5" stroke-linecap="round" opacity="{op:.2}"/>"##,
                        ax - r * 0.7, by - r * 0.7, ax - r * 0.7, by + r * 0.7,
                        ax + r * 0.7, by - r * 0.7, ax + r * 0.7, by + r * 0.7
                    );
                }
                if reading {
                    let _ = write!(o, r##"<line x1="{ax}" y1="{}" x2="{ax}" y2="{}" stroke="#2563EB" stroke-width="2" stroke-dasharray="3 4"/>"##, by + 6.0, ay - 22.0);
                }
                let col = if glow.is_some() { ACCENT } else { "#475569" };
                let _ = write!(
                    o,
                    r##"<circle cx="{ax}" cy="{by}" r="5" fill="#334155"/><text x="{ax}" y="{}" font-size="14" font-weight="700" text-anchor="middle" fill="{col}">{}</text>"##,
                    by - 28.0, esc(&d.label)
                );
            }
            DeviceKind::Zone => {}
        }
    }

    // Carriers. SampleId tags go right of the carrier if there is room,
    // otherwise left, below or above: whichever doesn't cover other carriers or tags.
    // Ghosts (logical positions) are drawn first, so real carriers cover them.
    let pos: Vec<(f64, f64)> = (0..sc.carriers.len()).map(|ci| tl.carrier_pos(ci, t)).collect();
    let op: Vec<f64> = tl.carriers.iter().map(|c| c.opacity(t)).collect();
    let mut order: Vec<usize> = (0..sc.carriers.len()).filter(|&ci| op[ci] > 0.0).collect();
    order.sort_by_key(|&ci| !sc.carriers[ci].ghost);
    let mut taken: Vec<(usize, (f64, f64, f64, f64))> = order.iter().map(|&ci| (ci, (pos[ci].0 - 24.0, pos[ci].1 - 24.0, pos[ci].0 + 24.0, pos[ci].1 + 24.0))).collect();
    for &ci in &order {
        let c = &sc.carriers[ci];
        let (x, y) = pos[ci];
        let _ = write!(o, r#"<g opacity="{:.2}">"#, op[ci]);
        if c.ghost {
            let _ = write!(
                o,
                r##"<circle cx="{x:.1}" cy="{y:.1}" r="26" fill="#FFFFFF" fill-opacity="0.6" stroke="#475569" stroke-width="2.5" stroke-dasharray="6 4"/>"##
            );
            if c.tube {
                let _ = write!(
                    o,
                    r##"<circle cx="{x:.1}" cy="{y:.1}" r="11" fill="none" stroke="{}" stroke-width="2.5" stroke-dasharray="4 3"/>"##,
                    c.cap_color
                );
            }
        } else {
            let _ = write!(
                o,
                r##"<circle cx="{:.1}" cy="{:.1}" r="23" fill="#0F172A" opacity="0.18"/><circle cx="{x:.1}" cy="{y:.1}" r="22" fill="#334155" stroke="#1E293B" stroke-width="2"/>"##,
                x + 2.0, y + 3.0
            );
            if c.tube {
                let _ = write!(
                    o,
                    r##"<circle cx="{x:.1}" cy="{y:.1}" r="13" fill="#FFFFFF" stroke="#CBD5E1" stroke-width="1.5"/><circle cx="{x:.1}" cy="{y:.1}" r="9.5" fill="{}"/>"##,
                    c.cap_color
                );
            } else {
                let _ = write!(o, r##"<circle cx="{x:.1}" cy="{y:.1}" r="12" fill="#1E293B" stroke="#475569" stroke-width="1.5"/>"##);
            }
        }
        o.push_str("</g>");
    }
    // Tags after all carriers, so that no carrier covers a tag.
    for &ci in &order {
        let c = &sc.carriers[ci];
        let (x, y) = pos[ci];
        // No tag for a carrier outside the panel (not yet arrived / already gone).
        if x < TRACK.0 || x > TRACK.0 + TRACK.2 {
            continue;
        }
        let tag = match &c.tag {
            Some(tag) => tag.clone(),
            None if c.tube && !c.sample_id.is_empty() => c.sample_id.clone(),
            None => format!("{} (empty)", c.id),
        };
        let w = 16.0 + tag.chars().count() as f64 * 9.0;
        let cands = [(x + 30.0, y - 13.0), (x - 30.0 - w, y - 13.0), (x - w / 2.0, y + 30.0), (x - w / 2.0, y - 56.0)];
        let inside = |(tx, ty): (f64, f64)| tx > TRACK.0 + 8.0 && tx + w < TRACK.0 + TRACK.2 - 8.0 && ty > TRACK.1 && ty + 26.0 < TRACK.1 + TRACK.3;
        let free = |(tx, ty): (f64, f64)| {
            taken.iter().all(|&(k, b)| k == ci || tx >= b.2 || tx + w <= b.0 || ty >= b.3 || ty + 26.0 <= b.1)
        };
        let (tx, ty) = cands
            .iter()
            .copied()
            .find(|&p| inside(p) && free(p))
            .or_else(|| cands.iter().copied().find(|&p| inside(p)))
            .unwrap_or(cands[0]);
        taken.push((usize::MAX, (tx, ty, tx + w, ty + 26.0)));
        let (fill, stroke, dash, style) = if c.ghost {
            ("#F8FAFC", "#64748B", r#" stroke-dasharray="5 4""#, r#" font-style="italic""#)
        } else {
            ("#FFFFFF", "#94A3B8", "", "")
        };
        let _ = write!(
            o,
            r##"<g opacity="{:.2}"><rect x="{:.1}" y="{:.1}" width="{w:.1}" height="26" rx="6" fill="{fill}" stroke="{stroke}"{dash}/><text x="{:.1}" y="{:.1}" font-size="15" font-family="{MONO}" fill="{INK}"{style}>{}</text></g>"##,
            op[ci], tx, ty, tx + 8.0, ty + 18.5, esc(&tag)
        );
    }

    // Analyzer probe (drawn above the carrier): a `pipette` (down, aspirate,
    // up) or the needle moved by `needle` steps (it stays down in between).
    let error = sc
        .devices
        .iter()
        .enumerate()
        .any(|(di, d)| d.kind == DeviceKind::Analyzer && analyzer_state(tl, di, t).is_some_and(|s| s.0 == AnalyzerState::Error));
    for (device, sd) in sc.devices.iter().enumerate() {
        if !matches!(sd.kind, DeviceKind::Sampling | DeviceKind::Turnstile) {
            continue;
        }
        let (mut ext, mut aspirating) = (0.0f64, false);
        if let Some(e) = active(tl, t, |e| matches!(e, Ev::Pipette { device: d } if *d == device)) {
            let u = (t - e.t0) / (e.t1 - e.t0).max(1e-6);
            ext = if u < 0.2 {
                Ease::InOut.apply(u / 0.2)
            } else if u < 0.8 {
                1.0
            } else {
                1.0 - Ease::InOut.apply((u - 0.8) / 0.2)
            };
            aspirating = (0.2..0.8).contains(&u);
        }
        let mut needle = 0.0;
        for e in tl.events.iter().filter(|e| e.t0 <= t) {
            if let Ev::Needle { device: d, down } = e.ev {
                if d == device {
                    let u = Ease::InOut.apply((t - e.t0) / (e.t1 - e.t0).max(1e-6));
                    needle = if down { needle + (1.0 - needle) * u } else { needle * (1.0 - u) };
                }
            }
        }
        let stuck = needle >= 1.0 - 1e-9 && error;
        if needle >= 1.0 - 1e-9 && !error {
            aspirating = true;
        }
        ext = ext.max(needle);
        if ext <= 0.0 {
            continue;
        }
        let (sx, sy) = sc.pt(sd.at.as_deref().unwrap());
        let an = sc.devices.iter().find(|d| d.kind == DeviceKind::Analyzer).unwrap();
        let top = an.rect.1 + an.rect.3;
        let tip = top + (sy - top) * ext;
        let col = if error { "#B91C1C" } else { "#475569" };
        let _ = write!(
            o,
            r##"<rect x="{}" y="{}" width="28" height="18" rx="4" fill="{col}"/><line x1="{sx}" y1="{top}" x2="{sx}" y2="{tip:.1}" stroke="{col}" stroke-width="6" stroke-linecap="round"/><circle cx="{sx}" cy="{tip:.1}" r="6" fill="#1E293B"/>"##,
            sx - 14.0, top - 4.0
        );
        let label = if aspirating {
            for k in 0..2 {
                let ph = ((t * 1.4) + k as f64 * 0.5).fract();
                let _ = write!(
                    o,
                    r##"<circle cx="{sx}" cy="{sy}" r="{:.1}" fill="none" stroke="#2563EB" stroke-width="3" opacity="{:.2}"/>"##,
                    16.0 + ph * 26.0, 0.8 * (1.0 - ph)
                );
            }
            Some(("Aspirating…", "#1D4ED8"))
        } else if stuck {
            Some(("Needle down", "#B91C1C"))
        } else {
            None
        };
        if let Some((text, fill)) = label {
            if let Some(w) = tl.wheel_of_device(device) {
                // Turnstile: left of the probe, between the analyzer and the wheel.
                let rim_top = w.cy - w.r - TS_RIM;
                let _ = write!(o, r##"<text x="{}" y="{:.1}" font-size="17" font-weight="600" text-anchor="end" fill="{fill}">{text}</text>"##, sx - 18.0, (top + rim_top) / 2.0 + 6.0);
            } else {
                let _ = write!(o, r##"<text x="{}" y="{}" font-size="17" font-weight="600" fill="{fill}">{text}</text>"##, sx + 22.0, top + 105.0);
            }
        }
    }
}

/// Last `status` of an analyzer at time t, with the time it was set.
fn analyzer_state(tl: &Timeline, di: usize, t: f64) -> Option<(AnalyzerState, f64)> {
    tl.events
        .iter()
        .filter(|e| e.t0 <= t)
        .filter_map(|e| match e.ev {
            Ev::Status { device, state } if device == di => Some((state, e.t0)),
            _ => None,
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

fn sequence(o: &mut String, sc: &Scene, tl: &Timeline, t: f64) {
    let n = sc.actors.len().max(1) as f64;
    let ax = |i: usize| SEQ.0 + 40.0 + (SEQ.2 - 40.0) * (i as f64 + 0.5) / n;
    let head_y = SEQ.1 + 48.0;
    let life_top = head_y + 62.0;
    let life_bot = SEQ.1 + SEQ.3 - 18.0;
    for (i, a) in sc.actors.iter().enumerate() {
        let x = ax(i);
        let _ = write!(
            o,
            r##"<line x1="{x}" y1="{life_top}" x2="{x}" y2="{life_bot}" stroke="#CBD5E1" stroke-width="2" stroke-dasharray="7 6"/><rect x="{}" y="{head_y}" width="210" height="62" rx="10" fill="#FFFFFF" stroke="{c}" stroke-width="2.5"/><rect x="{}" y="{head_y}" width="210" height="8" rx="4" fill="{c}"/><text x="{x}" y="{}" font-size="21" font-weight="700" text-anchor="middle" fill="{INK}">{}</text><text x="{x}" y="{}" font-size="14" text-anchor="middle" fill="{MUTED}">{}</text>"##,
            x - 105.0, x - 105.0, head_y + 32.0, esc(&a.label), head_y + 52.0, esc(&a.sub),
            c = a.color
        );
    }

    let rows = tl.rows.max(1) as f64;
    let first_y = life_top + 46.0;
    let avail = life_bot - 20.0 - first_y;
    let row_h = if avail / rows >= MIN_ROW_H { (avail / rows).min(70.0) } else { SCROLL_ROW_H };
    let row_y = |r: usize| first_y + row_h * r as f64;

    // When the rows don't fit, scroll up so that each new row is visible when
    // it appears (the scroll ends exactly as the row starts).
    let scroll = tl
        .events
        .iter()
        .filter_map(|e| match e.ev {
            Ev::Message { row, .. } | Ev::Note { row, .. } => {
                let need = ((row + 1) as f64 * row_h - avail).max(0.0);
                Some(need * Ease::InOut.apply((t - e.t0 + 0.6) / 0.6))
            }
            _ => None,
        })
        .fold(0.0, f64::max);
    let _ = write!(
        o,
        r#"<clipPath id="seq"><rect x="{}" y="{life_top}" width="{}" height="{}"/></clipPath><g clip-path="url(#seq)"><g transform="translate(0,{:.1})">"#,
        SEQ.0, SEQ.2, life_bot - life_top, -scroll
    );

    for e in &tl.events {
        if t < e.t0 {
            continue;
        }
        let p = clamp01((t - e.t0) / (e.t1 - e.t0).max(1e-6));
        let fresh = t <= e.t1 + 1.5;
        let alpha = clamp01((t - e.t0) / 0.3);
        match &e.ev {
            Ev::Message { from, to, text, params, reply, row, color } => {
                let y = row_y(*row);
                let (x1, x2) = (ax(*from), ax(*to));
                let dir = if x2 > x1 { 1.0 } else { -1.0 };
                let x2a = x2 - dir * 4.0;
                let xt = x1 + (x2a - x1) * Ease::Out.apply(p);
                let col = color.as_deref().unwrap_or(&sc.actors[*from].color);
                let sw = if fresh { 4.0 } else { 2.5 };
                let dash = if *reply { r#" stroke-dasharray="10 7""# } else { "" };
                if fresh {
                    let _ = write!(o, r#"<line x1="{x1}" y1="{y}" x2="{xt:.1}" y2="{y}" stroke="{ACCENT}" stroke-width="12" opacity="0.25" stroke-linecap="round"/>"#);
                }
                let _ = write!(o, r#"<line x1="{x1}" y1="{y}" x2="{xt:.1}" y2="{y}" stroke="{col}" stroke-width="{sw}"{dash}/>"#);
                let (hx, hy) = (xt, y);
                if *reply {
                    let _ = write!(o, r#"<path d="M{:.1},{:.1} L{hx:.1},{hy} L{:.1},{:.1}" fill="none" stroke="{col}" stroke-width="{sw}" stroke-linejoin="round"/>"#, hx - dir * 14.0, hy - 8.0, hx - dir * 14.0, hy + 8.0);
                } else {
                    let _ = write!(o, r#"<path d="M{:.1},{:.1} L{hx:.1},{hy} L{:.1},{:.1} Z" fill="{col}"/>"#, hx - dir * 16.0, hy - 8.0, hx - dir * 16.0, hy + 8.0);
                }
                let mx = (x1 + x2) / 2.0;
                let wgt = if fresh { "700" } else { "600" };
                let ptxt = if params.is_empty() {
                    String::new()
                } else {
                    format!(r#"<tspan dx="10" font-size="14" font-weight="400" font-family="{MONO}" fill="{MUTED}">{}</tspan>"#, esc(params))
                };
                let lw = text.chars().count() as f64 * 10.2
                    + if params.is_empty() { 0.0 } else { 10.0 + params.chars().count() as f64 * 8.5 };
                let _ = write!(o, r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="24" rx="4" fill="#FFFFFF" opacity="{alpha:.2}"/>"##, mx - lw / 2.0 - 4.0, y - 29.0, lw + 8.0);
                let _ = write!(o, r##"<text x="{mx}" y="{:.1}" font-size="18" font-weight="{wgt}" text-anchor="middle" fill="{INK}" opacity="{alpha:.2}">{}{ptxt}</text>"##, y - 11.0, esc(text));
                if sc.video.show_times {
                    let _ = write!(o, r##"<text x="{}" y="{:.1}" font-size="13" font-family="{MONO}" fill="#94A3B8" opacity="{alpha:.2}">{:.1}s</text>"##, SEQ.0 + 14.0, y + 5.0, e.t0);
                }
            }
            Ev::Note { actor, text, row } => {
                let y = row_y(*row);
                let x = ax(*actor);
                let w = 30.0 + text.chars().count() as f64 * 9.5;
                let (fill, stroke) = if t <= e.t1 { ("#FEF3C7", ACCENT) } else { ("#FFFBEB", "#FCD34D") };
                let _ = write!(
                    o,
                    r##"<g opacity="{alpha:.2}"><rect x="{:.1}" y="{:.1}" width="{w:.1}" height="34" rx="8" fill="{fill}" stroke="{stroke}" stroke-width="2"/><text x="{x}" y="{:.1}" font-size="16" font-weight="600" text-anchor="middle" fill="#92400E">{}</text></g>"##,
                    x - w / 2.0, y - 17.0, y + 6.0, esc(text)
                );
                if sc.video.show_times {
                    let _ = write!(o, r##"<text x="{}" y="{:.1}" font-size="13" font-family="{MONO}" fill="#94A3B8" opacity="{alpha:.2}">{:.1}s</text>"##, SEQ.0 + 14.0, y + 5.0, e.t0);
                }
            }
            _ => {}
        }
    }
    o.push_str("</g></g>");
}

fn caption(o: &mut String, tl: &Timeline, t: f64) {
    let cur = tl.events.iter().filter(|e| matches!(e.ev, Ev::Caption { .. }) && e.t0 <= t).last();
    let _ = write!(o, r#"<rect x="30" y="{CAPTION_Y}" width="1860" height="150" rx="16" fill="{INK}"/>"#);
    if let Some(e) = cur {
        if let Ev::Caption { title, text } = &e.ev {
            let a = clamp01((t - e.t0) / 0.4);
            let _ = write!(o, r#"<g opacity="{a:.2}">"#);
            let mut y = CAPTION_Y + 42.0;
            if !title.is_empty() {
                let _ = write!(o, r#"<text x="64" y="{y}" font-size="17" font-weight="700" letter-spacing="1.5" fill="{ACCENT}">{}</text>"#, esc(&title.to_uppercase()));
                y += 40.0;
            } else {
                y += 14.0;
            }
            let max_lines = if title.is_empty() { 3 } else { 2 };
            for line in wrap(text, 118).iter().take(max_lines) {
                let _ = write!(o, r##"<text x="64" y="{y}" font-size="27" fill="#F8FAFC">{}</text>"##, esc(line));
                y += 36.0;
            }
            o.push_str("</g>");
        }
    }
}
