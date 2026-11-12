//! The probe of the analyzer, drawn above the carrier.

use super::*;
use super::devices::{device_state, needle_bands};
use super::track::Ctx;
use crate::scene::{AnalyzerState, DeviceKind, Ease, Ev, TS_RIM};
use std::fmt::Write;

/// Analyzer probe: a `pipette` (down, aspirate, up) or the needle moved by
/// `needle` steps (it stays down in between).
pub(super) fn probes(o: &mut String, c: &Ctx) {
    let Ctx { sc, tl, t } = *c;
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
        let Some(an_idx) = sc.analyzer() else { continue };
        // In error: the whole instrument, or just this needle.
        let error = [an_idx, device].iter().any(|&d| device_state(tl, d, t).is_some_and(|s| s.0 == AnalyzerState::Error));
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
        let (ax, ay, aw, ah) = sc.devices[an_idx].rect;
        // The probe leaves the analyzer from the side nearest to the sampling
        // point: the bottom (analyzer above the lane) or a side (e.g. a spur).
        let vertical = sy >= ay + ah || sy <= ay;
        let (bx, by) = (sx.clamp(ax, ax + aw), sy.clamp(ay, ay + ah));
        let (tx, ty) = (bx + (sx - bx) * ext, by + (sy - by) * ext);
        let col = if error { "#B91C1C" } else { "#475569" };
        let (hw, hh) = if vertical { (28.0, 18.0) } else { (18.0, 28.0) };
        let _ = write!(
            o,
            r##"<rect x="{:.1}" y="{:.1}" width="{hw}" height="{hh}" rx="4" fill="{col}"/><line x1="{bx}" y1="{by}" x2="{tx:.1}" y2="{ty:.1}" stroke="{col}" stroke-width="6" stroke-linecap="round"/><circle cx="{tx:.1}" cy="{ty:.1}" r="6" fill="#1E293B"/>"##,
            bx - hw / 2.0, by - hh / 2.0
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
                let _ = write!(o, r##"<text x="{}" y="{:.1}" font-size="17" font-weight="600" text-anchor="end" fill="{fill}">{text}</text>"##, sx - 18.0, (by + rim_top) / 2.0 + 6.0);
            } else if vertical {
                let _ = write!(o, r##"<text x="{}" y="{}" font-size="17" font-weight="600" fill="{fill}">{text}</text>"##, sx + 22.0, by + 105.0);
            } else {
                // Probe from a side (e.g. a spur): the lane side is busy with
                // carriers and tags, so the label goes inside the analyzer, in
                // the band of this needle if it has several.
                let ly = if needle_bands(sc, an_idx).len() > 1 { sy + 6.0 } else { ay + ah / 2.0 + 64.0 };
                let _ = write!(o, r##"<text x="{}" y="{ly}" font-size="17" font-weight="600" text-anchor="middle" fill="{fill}">{text}</text>"##, ax + aw / 2.0);
            }
        }
    }
}
