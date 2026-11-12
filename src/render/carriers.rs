//! The carriers and their tags.

use super::*;

/// The carriers. SampleId tags go right of the carrier if there is room,
/// otherwise left, below or above: whichever doesn't cover other carriers or tags.
/// Ghosts (logical positions) are drawn first, so real carriers cover them.
pub(super) fn carriers(o: &mut String, c: &Ctx) {
    let Ctx { sc, tl, t } = *c;
    let pos: Vec<(f64, f64)> = (0..sc.carriers.len()).map(|ci| tl.carrier_pos(ci, t)).collect();
    let op: Vec<f64> = tl.carriers.iter().map(|c| c.opacity(t)).collect();
    let mut order: Vec<usize> = (0..sc.carriers.len()).filter(|&ci| op[ci] > 0.0).collect();
    order.sort_by_key(|&ci| !sc.carriers[ci].ghost);
    let mut taken: Vec<(usize, (f64, f64, f64, f64))> = order.iter().map(|&ci| (ci, (pos[ci].0 - 24.0, pos[ci].1 - 24.0, pos[ci].0 + 24.0, pos[ci].1 + 24.0))).collect();
    // Tags don't cover the analyzer either.
    for d in sc.devices.iter().filter(|d| d.kind == DeviceKind::Analyzer) {
        let (x, y, w, h) = d.rect;
        taken.push((usize::MAX, (x, y, x + w, y + h)));
    }
    for &ci in &order {
        let c = &sc.carriers[ci];
        let (x, y) = pos[ci];
        let _ = write!(o, r#"<g opacity="{:.2}">"#, op[ci]);
        if tl.carriers[ci].running_away(t) {
            let a = 0.35 + 0.45 * (t * 2.0 * std::f64::consts::PI * 2.0).sin().abs();
            let _ = write!(o, r##"<circle cx="{x:.1}" cy="{y:.1}" r="27.9" fill="none" stroke="#DC2626" stroke-width="5" opacity="{a:.2}"/>"##);
        }
        if c.ghost {
            let _ = write!(
                o,
                r##"<circle cx="{x:.1}" cy="{y:.1}" r="23.4" fill="#FFFFFF" fill-opacity="0.6" stroke="#475569" stroke-width="2.5" stroke-dasharray="6 4"/>"##
            );
            if c.tube {
                let _ = write!(
                    o,
                    r##"<circle cx="{x:.1}" cy="{y:.1}" r="9.9" fill="none" stroke="{}" stroke-width="2.5" stroke-dasharray="4 3"/>"##,
                    c.cap_color
                );
            }
        } else {
            let _ = write!(
                o,
                r##"<circle cx="{:.1}" cy="{:.1}" r="20.7" fill="#0F172A" opacity="0.18"/><circle cx="{x:.1}" cy="{y:.1}" r="19.8" fill="#334155" stroke="#1E293B" stroke-width="2"/>"##,
                x + 2.0, y + 3.0
            );
            if c.tube {
                let _ = write!(
                    o,
                    r##"<circle cx="{x:.1}" cy="{y:.1}" r="11.7" fill="#FFFFFF" stroke="#CBD5E1" stroke-width="1.5"/><circle cx="{x:.1}" cy="{y:.1}" r="8.6" fill="{}"/>"##,
                    c.cap_color
                );
            } else {
                let _ = write!(o, r##"<circle cx="{x:.1}" cy="{y:.1}" r="10.8" fill="#1E293B" stroke="#475569" stroke-width="1.5"/>"##);
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
        let runaway = tl.carriers[ci].running_away(t);
        let mut tag = match &c.tag {
            Some(tag) => tag.clone(),
            None if c.tube && !c.sample_id.is_empty() => c.sample_id.clone(),
            None => format!("{} (empty)", c.id),
        };
        if runaway {
            tag.push_str(" · RUN-AWAY");
        }
        let w = 16.0 + tag.chars().count() as f64 * 9.0;
        let cands = [(x + 27.0, y - 13.0), (x - 27.0 - w, y - 13.0), (x - w / 2.0, y + 27.0), (x - w / 2.0, y - 53.0)];
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
        let (fill, stroke, dash, ink, style) = if runaway {
            ("#FEE2E2", "#DC2626", "", "#B91C1C", r#" font-weight="700""#)
        } else if c.ghost {
            ("#F8FAFC", "#64748B", r#" stroke-dasharray="5 4""#, INK, r#" font-style="italic""#)
        } else {
            ("#FFFFFF", "#94A3B8", "", INK, "")
        };
        let _ = write!(
            o,
            r##"<g opacity="{:.2}"><rect x="{:.1}" y="{:.1}" width="{w:.1}" height="26" rx="6" fill="{fill}" stroke="{stroke}"{dash}/><text x="{:.1}" y="{:.1}" font-size="15" font-family="{MONO}" fill="{ink}"{style}>{}</text></g>"##,
            op[ci], tx, ty, tx + 8.0, ty + 18.5, esc(&tag)
        );
    }
}
