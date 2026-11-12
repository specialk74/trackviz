//! The layout image: the track with the names of every point and device, and
//! the list of the devices.

use super::*;
use super::track::track;
use crate::scene::{Device, DeviceKind, Point, Scene, Timeline};
use std::fmt::Write;

/// Where a device is drawn, for the layout.
fn device_place(sc: &Scene, d: &Device) -> (f64, f64) {
    match (&d.at, d.kind) {
        (_, DeviceKind::Turnstile) => (d.rect.0, d.rect.1),
        (Some(p), _) => sc.pt(p),
        (None, _) => (d.rect.0 + d.rect.2 / 2.0, d.rect.1 + d.rect.3 / 2.0),
    }
}

fn kind_name(k: DeviceKind) -> &'static str {
    match k {
        DeviceKind::Nsd => "nsd",
        DeviceKind::Sampling => "sampling",
        DeviceKind::Analyzer => "analyzer",
        DeviceKind::Zone => "zone",
        DeviceKind::Turnstile => "turnstile",
        DeviceKind::Gate => "gate",
        DeviceKind::Antenna => "antenna",
        DeviceKind::Divert => "divert",
    }
}

/// A device with the antenna and the gate declared on it: sp.antenna2.gate.
fn full_name(sc: &Scene, d: &Device) -> String {
    let mut name = d.id.clone();
    for p in sc.devices.iter().filter(|p| p.part_of.as_deref() == Some(d.id.as_str())) {
        name.push('.');
        name.push_str(match p.kind {
            DeviceKind::Gate => "gate",
            _ => &p.id,
        });
    }
    name
}

/// An overview of the scene: the track with the names of every point and device
/// (hidden antennas too), and the list of the devices. No carriers, no messages.
pub fn layout(sc: &Scene, tl: &Timeline) -> String {
    let mut o = String::with_capacity(64 * 1024);
    let v = &sc.video;
    let _ = write!(
        o,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 1920 1080" font-family="{FONT}">"#,
        v.width, v.height
    );
    o.push_str(r##"<rect width="1920" height="1080" fill="#F1F4F8"/>"##);
    let _ = write!(o, r#"<text x="40" y="62" font-size="32" font-weight="600" fill="{INK}">Layout · {}</text>"#, esc(&v.subtitle));
    let _ = write!(o, r#"<text x="40" y="96" font-size="19" fill="{MUTED}">Names of the points (grey) and of the devices (purple) to use in the scene</text>"#);
    for (r, title) in [(TRACK, "TRACK · TOP VIEW"), (SEQ, "DEVICES")] {
        let _ = write!(
            o,
            r##"<rect x="{}" y="{}" width="{}" height="{}" rx="16" fill="#FFFFFF" stroke="#DCE2EA"/><text x="{}" y="{}" font-size="14" font-weight="600" letter-spacing="1.5" fill="#94A3B8">{title}</text>"##,
            r.0, r.1, r.2, r.3, r.0 + 22.0, r.1 + 30.0
        );
    }
    let _ = write!(o, r#"<clipPath id="trk"><rect x="{}" y="{}" width="{}" height="{}" rx="16"/></clipPath><g clip-path="url(#trk)">"#, TRACK.0, TRACK.1, TRACK.2, TRACK.3);
    track(&mut o, sc, tl, 0.0, false);
    let halo = r##"stroke="#FFFFFF" stroke-width="4" stroke-linejoin="round" paint-order="stroke""##;
    // Points, then devices on top.
    let mut points: Vec<(&String, &Point)> = sc.points.iter().collect();
    points.sort_by(|a, b| a.0.cmp(b.0));
    // The point of a generated gate has the name of its device.
    for (id, p) in points.into_iter().filter(|(id, _)| !id.ends_with(".gate")) {
        let _ = write!(
            o,
            r##"<circle cx="{}" cy="{}" r="3" fill="#64748B"/><text x="{}" y="{}" font-size="12" fill="#475569" {halo}>{}</text>"##,
            p.x, p.y, p.x + 5.0, p.y - 5.0, esc(id)
        );
    }
    // Devices on the same spot get their names one under the other; boxes get
    // theirs in the lower left corner.
    let mut used: Vec<((f64, f64), usize)> = Vec::new();
    for d in sc.devices.iter().filter(|d| d.part_of.is_none()) {
        let (x, y) = device_place(sc, d);
        if d.kind == DeviceKind::Antenna && !d.visible {
            let _ = write!(o, r##"<circle cx="{x}" cy="{y}" r="15" fill="none" stroke="#7C3AED" stroke-width="2" stroke-dasharray="4 3"/>"##);
        }
        let (tx, ty) = match d.kind {
            DeviceKind::Zone | DeviceKind::Analyzer => (d.rect.0 + 12.0, d.rect.1 + d.rect.3 - 12.0),
            _ => {
                let k = match used.iter_mut().find(|u| ((u.0 .0 - x).powi(2) + (u.0 .1 - y).powi(2)).sqrt() < 8.0) {
                    Some(u) => {
                        u.1 += 1;
                        u.1
                    }
                    None => {
                        used.push(((x, y), 0));
                        0
                    }
                };
                (x + 10.0, y + 22.0 + 17.0 * k as f64)
            }
        };
        let _ = write!(o, r##"<text x="{tx}" y="{ty}" font-size="15" font-weight="700" fill="#7C3AED" {halo}>{}</text>"##, esc(&full_name(sc, d)));
    }
    o.push_str("</g>");
    // The list of the devices.
    let (x0, mut y) = (SEQ.0 + 30.0, SEQ.1 + 70.0);
    let _ = write!(o, r##"<text x="{x0}" y="{y}" font-size="15" font-weight="700" fill="#475569">id</text><text x="{}" y="{y}" font-size="15" font-weight="700" fill="#475569">kind</text><text x="{}" y="{y}" font-size="15" font-weight="700" fill="#475569">where · label</text>"##, x0 + 230.0, x0 + 480.0);
    for d in sc.devices.iter().filter(|d| d.part_of.is_none()) {
        y += 26.0;
        if y > SEQ.1 + SEQ.3 - 20.0 {
            break;
        }
        let place = match (&d.at, d.kind) {
            (_, DeviceKind::Turnstile) => format!("center {:.0},{:.0}", d.rect.0, d.rect.1),
            (Some(p), _) => format!("at {p}"),
            (None, _) => format!("box {:.0},{:.0} {:.0}×{:.0}", d.rect.0, d.rect.1, d.rect.2, d.rect.3),
        };
        let mut kind = kind_name(d.kind).to_string();
        if d.kind == DeviceKind::Antenna && !d.visible {
            kind.push_str(" (hidden)");
        }
        for part in sc.devices.iter().filter(|p| p.part_of.as_deref() == Some(d.id.as_str())) {
            kind.push_str(" + ");
            kind.push_str(kind_name(part.kind));
        }
        let label = if d.label.is_empty() { String::new() } else { format!(" · {}", d.label) };
        let _ = write!(
            o,
            r##"<text x="{x0}" y="{y}" font-size="15" font-family="{MONO}" fill="#7C3AED">{}</text><text x="{}" y="{y}" font-size="15" fill="{INK}">{}</text><text x="{}" y="{y}" font-size="15" fill="{MUTED}">{}{}</text>"##,
            esc(&full_name(sc, d)), x0 + 230.0, esc(&kind), x0 + 480.0, esc(&place), esc(&label)
        );
    }
    o.push_str("</svg>");
    o
}
