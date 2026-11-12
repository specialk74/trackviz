//! The caption band.

use super::*;
use crate::scene::{Ev, Timeline};
use std::fmt::Write;

/// Caption lines, and how many of them fit in the band.
pub fn caption_lines(title: &str, text: &str) -> (Vec<String>, usize) {
    (wrap(text, 118), if title.is_empty() { 3 } else { 2 })
}

pub(super) fn wrap(text: &str, max: usize) -> Vec<String> {
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

/// The title of the caption that shows where a scene breaks (the video up to the
/// error): its band is red.
pub const ERROR_TITLE: &str = "Error";

pub(super) fn caption(o: &mut String, tl: &Timeline, t: f64) {
    let cur = tl.events.iter().filter(|e| matches!(e.ev, Ev::Caption { .. }) && e.t0 <= t).max_by(|a, b| a.t0.total_cmp(&b.t0));
    let error = cur.is_some_and(|e| matches!(&e.ev, Ev::Caption { title, .. } if title.starts_with(ERROR_TITLE)));
    let band = if error { "#991B1B" } else { INK };
    let _ = write!(o, r#"<rect x="30" y="{CAPTION_Y}" width="1860" height="150" rx="16" fill="{band}"/>"#);
    if let Some(e) = cur {
        if let Ev::Caption { title, text } = &e.ev {
            let a = clamp01((t - e.t0) / 0.4);
            let _ = write!(o, r#"<g opacity="{a:.2}">"#);
            let mut y = CAPTION_Y + 42.0;
            if !title.is_empty() {
                let _ = write!(o, r#"<text x="64" y="{y}" font-size="17" font-weight="700" letter-spacing="1.5" fill="{}">{}</text>"#, if error { "#FECACA" } else { ACCENT }, esc(&title.to_uppercase()));
                y += 40.0;
            } else {
                y += 14.0;
            }
            let (lines, max_lines) = caption_lines(title, text);
            for line in lines.iter().take(max_lines) {
                let _ = write!(o, r##"<text x="64" y="{y}" font-size="27" fill="#F8FAFC">{}</text>"##, esc(line));
                y += 36.0;
            }
            o.push_str("</g>");
        }
    }
}
