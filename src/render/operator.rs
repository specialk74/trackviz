//! The lab operator working on the analyzer.

use super::*;

/// Stylized clinical lab operator (front view: white lab coat, thin blue
/// gloves): walks in during the first part of the step, works on the analyzer
/// with one hand, then walks away.
pub(super) fn operator(o: &mut String, an: &Device, u: f64, dur: f64, t: f64) {
    use std::f64::consts::PI;
    let (ax, ay, aw, ah) = an.rect;
    let walk = (dur / 3.0).min(2.0);
    // Right of the analyzer if there is room, otherwise below it (bottom right),
    // working with the arm raised.
    let x_out = TRACK.0 + TRACK.2 + 40.0;
    let below = ax + aw + 75.0 > TRACK.0 + TRACK.2;
    let (x_work, y) = if below { (ax + aw - 45.0, ay + ah + 38.0) } else { (ax + aw + 36.0, ay + ah / 2.0 - 4.0) };
    let (x, walking, facing_left) = if u < walk {
        (x_out + (x_work - x_out) * Ease::InOut.apply(u / walk), true, true)
    } else if u > dur - walk {
        (x_work + (x_out - x_work) * Ease::InOut.apply((u - (dur - walk)) / walk), true, false)
    } else {
        (x_work, false, true)
    };
    let dir = if facing_left { -1.0 } else { 1.0 };
    let step = if walking { (t * 2.0 * PI * 1.6).sin() } else { 0.0 };
    let bob = if walking { -(t * 2.0 * PI * 3.2).sin().abs() * 2.5 } else { 0.0 };
    let y = y + bob;
    // Shadow, legs
    let _ = write!(o, r##"<ellipse cx="{x:.1}" cy="{:.1}" rx="20" ry="5" fill="#0F172A" opacity="0.15"/>"##, y + 36.0 - bob);
    for k in [-1.0, 1.0] {
        let _ = write!(
            o,
            r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#334155" stroke-width="7" stroke-linecap="round"/>"##,
            x + k * 5.0, y + 14.0, x + k * 6.0 + k * step * 6.0, y + 33.0
        );
    }
    // Arms: the back one hangs down; the front one, while working, moves on the analyzer.
    let (bx, by) = (x - dir * 15.0 - dir * step * 4.0, y + 8.0);
    let _ = write!(
        o,
        r##"<line x1="{:.1}" y1="{:.1}" x2="{bx:.1}" y2="{by:.1}" stroke="#CBD5E1" stroke-width="8" stroke-linecap="round"/><line x1="{:.1}" y1="{:.1}" x2="{bx:.1}" y2="{by:.1}" stroke="#FFFFFF" stroke-width="5.5" stroke-linecap="round"/><circle cx="{bx:.1}" cy="{:.1}" r="3.6" fill="{GLOVE}"/>"##,
        x - dir * 12.0, y - 12.0, x - dir * 12.0, y - 12.0, by + 2.0
    );
    let ang = if walking {
        PI / 2.0 - 0.3 + step * 0.3
    } else if below {
        -1.0 + 0.3 * (t * 2.0 * PI * 1.8).sin()
    } else {
        0.15 + 0.35 * (t * 2.0 * PI * 1.8).sin()
    };
    let (sx, sy) = (x + dir * 12.0, y - 12.0);
    let (hx, hy) = (sx + dir * 24.0 * ang.cos(), sy + 24.0 * ang.sin());
    // Body: long white lab coat (open front, collar, pocket); head with hair.
    let _ = write!(
        o,
        r##"<path d="M{:.1},{:.1} Q{:.1},{:.1} {:.1},{:.1} L{:.1},{:.1} L{:.1},{:.1} L{:.1},{:.1} Q{:.1},{:.1} {:.1},{:.1} Z" fill="#FFFFFF" stroke="#94A3B8" stroke-width="1.8" stroke-linejoin="round"/><line x1="{x:.1}" y1="{:.1}" x2="{x:.1}" y2="{:.1}" stroke="#94A3B8" stroke-width="1.5"/><path d="M{:.1},{:.1} L{x:.1},{:.1} L{:.1},{:.1}" fill="none" stroke="#94A3B8" stroke-width="1.5"/><rect x="{:.1}" y="{:.1}" width="7" height="5" rx="1" fill="none" stroke="#94A3B8" stroke-width="1.2"/>"##,
        x - 9.0, y - 20.0, x - 15.0, y - 18.0, x - 14.0, y - 10.0, x - 15.0, y + 18.0, x + 15.0, y + 18.0, x + 14.0, y - 10.0, x + 15.0, y - 18.0, x + 9.0, y - 20.0,
        y - 12.0, y + 18.0, x - 6.0, y - 20.0, y - 12.0, x + 6.0, y - 20.0, x + 4.0, y - 7.0
    );
    let _ = write!(
        o,
        r##"<circle cx="{x:.1}" cy="{:.1}" r="10" fill="#F2C9A0"/><path d="M{:.1},{:.1} A10.5,10.5 0 0 1 {:.1},{:.1} Q{x:.1},{:.1} {:.1},{:.1} Z" fill="#4B3621"/>"##,
        y - 30.0, x - 10.5, y - 29.0, x + 10.5, y - 29.0, y - 35.0, x - 10.5, y - 29.0
    );
    let _ = write!(
        o,
        r##"<line x1="{sx:.1}" y1="{sy:.1}" x2="{hx:.1}" y2="{hy:.1}" stroke="#CBD5E1" stroke-width="8" stroke-linecap="round"/><line x1="{sx:.1}" y1="{sy:.1}" x2="{hx:.1}" y2="{hy:.1}" stroke="#FFFFFF" stroke-width="5.5" stroke-linecap="round"/><circle cx="{hx:.1}" cy="{hy:.1}" r="3.8" fill="{GLOVE}"/>"##
    );
    let _ = write!(o, r##"<text x="{x:.1}" y="{:.1}" font-size="14" font-weight="600" text-anchor="middle" fill="{MUTED}">Operator</text>"##, y + 54.0 - bob);
}
