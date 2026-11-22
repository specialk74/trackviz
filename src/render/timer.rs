//! A fast clock next to a device: plant time running quickly towards a timeout.

use super::*;
use crate::scene::Device;
use std::f64::consts::PI;

/// Seconds the clock stays, stopped, after its step ends (then it fades out).
pub(super) const LINGER: f64 = 1.5;
const FADE: f64 = 0.4;
const CARD_W: f64 = 280.0;

fn mmss(s: f64) -> String {
    let s = s.max(0.0).round() as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

/// The clock at time t of an event from t0 to t1: `value` seconds of the plant
/// pass in t1 - t0; the dial turns once in `limit`.
#[allow(clippy::too_many_arguments)]
pub(super) fn timer(o: &mut String, d: &Device, pos: Option<(f64, f64)>, value: f64, limit: f64, label: &str, t0: f64, t1: f64, t: f64) {
    let dur = (t1 - t0).max(1e-6);
    let u = clamp01((t - t0) / dur);
    let cur = value * u;
    let alpha = clamp01((t - t0) / FADE).min(clamp01((t1 + LINGER + FADE - t) / FADE));
    // Default: in the top-right corner of the device.
    let (cx, cy) = pos.unwrap_or((d.rect.0 + d.rect.2 - 12.0 - CARD_W + 46.0, d.rect.1 + 52.0));
    let r = 34.0;
    let expired = cur >= limit - 1e-6;
    let running = t < t1;
    let (col, edge) = if expired { ("#DC2626", "#DC2626") } else { ("#D97706", "#CBD5E1") };
    let _ = write!(o, r#"<g opacity="{alpha:.2}">"#);
    // The card.
    let _ = write!(
        o,
        r##"<rect x="{}" y="{}" width="{CARD_W}" height="80" rx="14" fill="#FFFFFF" stroke="{edge}" stroke-width="{}"/>"##,
        cx - r - 12.0, cy - 40.0, if expired { 3.0 } else { 1.5 }
    );
    // The dial: ticks, the part of the timeout gone (a sector), two hands.
    let _ = write!(o, r##"<circle cx="{cx}" cy="{cy}" r="{r}" fill="#F8FAFC" stroke="#334155" stroke-width="2.5"/>"##);
    let frac = clamp01(cur / limit);
    if frac > 0.0 {
        if frac >= 0.999 {
            let _ = write!(o, r##"<circle cx="{cx}" cy="{cy}" r="{}" fill="{col}" opacity="0.25"/>"##, r - 3.0);
        } else {
            let a = frac * 2.0 * PI;
            let rr = r - 3.0;
            let (ex, ey) = (cx + rr * a.sin(), cy - rr * a.cos());
            let large = if frac > 0.5 { 1 } else { 0 };
            let _ = write!(o, r##"<path d="M{cx},{cy} L{cx},{:.1} A{rr},{rr} 0 {large} 1 {ex:.1},{ey:.1} Z" fill="{col}" opacity="0.25"/>"##, cy - rr);
        }
    }
    for k in 0..12 {
        let a = k as f64 / 12.0 * 2.0 * PI;
        let (r0, w) = if k % 3 == 0 { (r - 9.0, 2.5) } else { (r - 6.0, 1.5) };
        let _ = write!(
            o,
            r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#334155" stroke-width="{w}"/>"##,
            cx + r0 * a.sin(), cy - r0 * a.cos(), cx + (r - 2.0) * a.sin(), cy - (r - 2.0) * a.cos()
        );
    }
    let hand = |o: &mut String, a: f64, len: f64, w: f64, c: &str| {
        let _ = write!(o, r##"<line x1="{cx}" y1="{cy}" x2="{:.1}" y2="{:.1}" stroke="{c}" stroke-width="{w}" stroke-linecap="round"/>"##, cx + len * a.sin(), cy - len * a.cos());
    };
    hand(o, frac * 2.0 * PI, r - 14.0, 4.0, "#0F172A");
    hand(o, (cur % 60.0) / 60.0 * 2.0 * PI, r - 5.0, 1.8, "#DC2626");
    let _ = write!(o, r##"<circle cx="{cx}" cy="{cy}" r="3.5" fill="#0F172A"/>"##);
    // The text: label, time gone / timeout, how fast it runs.
    let tx = cx + r + 14.0;
    let _ = write!(o, r##"<text x="{tx}" y="{}" font-size="13" font-weight="700" letter-spacing="1" fill="{MUTED}">{}</text>"##, cy - 20.0, esc(&label.to_uppercase()));
    let _ = write!(
        o,
        r##"<text x="{tx}" y="{}" font-family="{MONO}" font-size="26" font-weight="700" fill="{}">{}</text><text x="{}" y="{}" font-family="{MONO}" font-size="14" fill="{MUTED}">/ {}</text>"##,
        cy + 8.0, if expired { "#DC2626" } else { INK }, mmss(cur), tx + 84.0, cy + 7.0, mmss(limit)
    );
    if expired && !running {
        // TIMEOUT, blinking.
        let on = ((t - t1) * 3.0).fract() < 0.6;
        if on {
            let _ = write!(o, r##"<rect x="{tx}" y="{}" width="92" height="20" rx="10" fill="#DC2626"/><text x="{}" y="{}" font-size="12" font-weight="700" letter-spacing="1.2" text-anchor="middle" fill="#FFFFFF">TIMEOUT</text>"##, cy + 15.0, tx + 46.0, cy + 29.5);
        }
    } else if !running {
        let _ = write!(o, r##"<text x="{tx}" y="{}" font-size="13" font-weight="600" fill="#16A34A">stopped</text>"##, cy + 30.0);
    }
    o.push_str("</g>");
}
