//! Frame renderer: builds one SVG per instant `t`.
//! All drawing happens in a fixed 1920x1080 coordinate space; the output size
//! from [video] only scales it.
//!
//! - `track` (with `devices`, `carriers`, `probe`, `operator`): the track panel;
//! - `sequence`: the sequence diagram; `caption`: the caption band;
//! - `layout`: the layout image.

mod caption;
mod carriers;
mod devices;
mod layout;
mod operator;
mod probe;
mod sequence;
mod track;

pub use caption::{caption_lines, ERROR_TITLE};
pub use layout::layout;
use caption::caption;
use devices::{device_state, needle_bands};
use operator::operator;
use sequence::sequence;
use track::{track, Ctx};

use crate::scene::*;
use std::fmt::Write;

const FONT: &str = "Inter, 'DejaVu Sans', sans-serif";
const MONO: &str = "'DejaVu Sans Mono', monospace";
const INK: &str = "#0F172A";
const MUTED: &str = "#64748B";
const ACCENT: &str = "#F59E0B";
/// Thin nitrile gloves of the lab operator.
const GLOVE: &str = "#3B82F6";

// Layout of the two panels and the caption band (in the 1920x1080 space).
const TRACK: (f64, f64, f64, f64) = (30.0, 120.0, 920.0, 765.0);
const SEQ: (f64, f64, f64, f64) = (970.0, 120.0, 920.0, 765.0);
/// Radius of a carrier as drawn: 90% of CARRIER_R, which stays the size for the
/// physics and the scale (so the times of the videos don't change).
const DRAW_R: f64 = 0.9 * CARRIER_R;
const CAPTION_Y: f64 = 905.0;

/// Sequence diagram: below this row height the diagram scrolls instead of
/// shrinking, with rows of SCROLL_ROW_H.
const MIN_ROW_H: f64 = 38.0;
const SCROLL_ROW_H: f64 = 46.0;

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
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
    // The line of the scene that ran last, small, under the clock.
    if v.show_line {
        if let Some((_, line)) = tl.lines.iter().rfind(|l| l.0 <= t + 1e-9) {
            let mut s: String = line.chars().take(96).collect();
            if line.chars().count() > 96 {
                s.push('…');
            }
            let _ = write!(o, r#"<text x="1880" y="100" font-size="14" text-anchor="end" font-family="{MONO}" fill="{MUTED}">{}</text>"#, esc(&s));
        }
    }

    for (r, title) in [(TRACK, "TRACK · TOP VIEW"), (SEQ, "MESSAGE SEQUENCE")] {
        let _ = write!(
            o,
            r##"<rect x="{}" y="{}" width="{}" height="{}" rx="16" fill="#FFFFFF" stroke="#DCE2EA"/><text x="{}" y="{}" font-size="14" font-weight="600" letter-spacing="1.5" fill="#94A3B8">{title}</text>"##,
            r.0, r.1, r.2, r.3, r.0 + 22.0, r.1 + 30.0
        );
    }

    let _ = write!(o, r#"<clipPath id="trk"><rect x="{}" y="{}" width="{}" height="{}" rx="16"/></clipPath><g clip-path="url(#trk)">"#, TRACK.0, TRACK.1, TRACK.2, TRACK.3);
    track(&mut o, sc, tl, t, true);
    o.push_str("</g>");
    sequence(&mut o, sc, tl, t);
    caption(&mut o, tl, t);
    o.push_str("</svg>");
    o
}

fn active(tl: &Timeline, t: f64, pred: impl Fn(&Ev) -> bool) -> Option<&Timed> {
    tl.events.iter().find(|e| t >= e.t0 && t <= e.t1 && pred(&e.ev))
}

#[cfg(test)]
mod tests;
