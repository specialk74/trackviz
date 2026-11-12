//! The sequence diagram: lifelines, messages and notes, scrolling when the rows
//! don't fit.

use super::*;

pub(super) fn sequence(o: &mut String, sc: &Scene, tl: &Timeline, t: f64) {
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

    let rows = (tl.rows as f64 / 2.0).max(1.0);
    let first_y = life_top + 46.0;
    let avail = life_bot - 20.0 - first_y;
    let row_h = if avail / rows >= MIN_ROW_H { (avail / rows).min(70.0) } else { SCROLL_ROW_H };
    // `r` in half rows.
    let row_y = |r: usize| first_y + row_h / 2.0 * r as f64;

    // When the rows don't fit, scroll up so that each new row is visible when
    // it appears (the scroll ends exactly as the row starts).
    let scroll = tl
        .events
        .iter()
        .filter_map(|e| match e.ev {
            Ev::Message { row, .. } | Ev::Note { row, .. } => {
                let need = ((row + 2) as f64 * row_h / 2.0 - avail).max(0.0);
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
                // One line of the label in each row, the last one on the arrow.
                // The backgrounds first, so that one doesn't cover the line above.
                let lines = label_lines(text, params);
                let mut texts = String::new();
                for (k, (line, par)) in lines.iter().enumerate() {
                    let ly = row_y(row + 1 + k - lines.len());
                    let gap = if line.is_empty() || par.is_empty() { 0.0 } else { 10.0 };
                    let ptxt = if par.is_empty() {
                        String::new()
                    } else {
                        format!(r#"<tspan dx="{gap}" font-size="13" font-weight="400" font-family="{MONO}" fill="{MUTED}">{}</tspan>"#, esc(par))
                    };
                    let lw = line.chars().count() as f64 * 9.6 + if par.is_empty() { 0.0 } else { gap + par.chars().count() as f64 * 7.9 };
                    let _ = write!(o, r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="24" rx="4" fill="#FFFFFF" opacity="{alpha:.2}"/>"##, mx - lw / 2.0 - 4.0, ly - 29.0, lw + 8.0);
                    let _ = write!(texts, r##"<text x="{mx}" y="{:.1}" font-size="17" font-weight="{wgt}" text-anchor="middle" fill="{INK}" opacity="{alpha:.2}">{}{ptxt}</text>"##, ly - 11.0, esc(line));
                }
                o.push_str(&texts);
                if sc.video.show_times {
                    let _ = write!(o, r##"<text x="{}" y="{:.1}" font-size="13" font-family="{MONO}" fill="#94A3B8" opacity="{alpha:.2}">{:.1}s</text>"##, SEQ.0 + 14.0, y + 5.0, e.t0);
                }
            }
            Ev::Note { actor, text, row } => {
                // In the same space as the label of a message (above its row line), so
                // that the label of the next row doesn't cover it.
                // A note of n lines spans n rows, ending on its own.
                let y = row_y(*row);
                let x = ax(*actor);
                let lines: Vec<String> = label_lines(text, "").into_iter().map(|l| l.0).collect();
                let top = row_y(row + 1 - lines.len());
                let w = 30.0 + lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f64 * 8.9;
                let (fill, stroke) = if t <= e.t1 { ("#FEF3C7", ACCENT) } else { ("#FFFBEB", "#FCD34D") };
                let _ = write!(
                    o,
                    r##"<g opacity="{alpha:.2}"><rect x="{:.1}" y="{:.1}" width="{w:.1}" height="{:.1}" rx="8" fill="{fill}" stroke="{stroke}" stroke-width="2"/>"##,
                    x - w / 2.0,
                    top - 30.0,
                    y - top + 34.0
                );
                for (k, line) in lines.iter().enumerate() {
                    let ly = top + (y - top) * if lines.len() > 1 { k as f64 / (lines.len() - 1) as f64 } else { 0.0 };
                    let _ = write!(o, r##"<text x="{x}" y="{:.1}" font-size="15" font-weight="600" text-anchor="middle" fill="#92400E">{}</text>"##, ly - 7.0, esc(line));
                }
                o.push_str("</g>");
                if sc.video.show_times {
                    let _ = write!(o, r##"<text x="{}" y="{:.1}" font-size="13" font-family="{MONO}" fill="#94A3B8" opacity="{alpha:.2}">{:.1}s</text>"##, SEQ.0 + 14.0, y + 5.0, e.t0);
                }
            }
            _ => {}
        }
    }
    o.push_str("</g></g>");
}
