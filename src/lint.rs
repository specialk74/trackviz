//! Warnings on a valid scene: things that render without errors but look wrong.
//! They are printed by `--check` and when rendering, and don't stop anything.

use crate::render::caption_lines;
use crate::scene::*;

use crate::physics::TRACK_X;

/// A warning, with where the step it is about is written (file, line).
#[derive(Debug)]
pub struct Warning {
    pub at: Option<(String, usize)>,
    pub text: String,
}

impl Warning {
    /// As printed: "<file> line <n>: <text>" (the scene's own path for a step
    /// written in it), or "<scene>: <text>".
    pub fn show(&self, path: &str) -> String {
        match &self.at {
            Some((file, line)) => format!("{} line {line}: {}", if file.is_empty() { path } else { file }, self.text),
            None => format!("{path}: {}", self.text),
        }
    }
}

/// Where a step is written: (file, line).
type At = Option<(String, usize)>;

fn w(at: &At, text: String) -> Warning {
    Warning { at: at.clone(), text }
}

/// " (line n)": where another step is written, in the text of a warning.
fn line_of(at: &At) -> String {
    at.as_ref().map_or(String::new(), |a| format!(" (line {})", a.1))
}

pub fn warnings_at(sc: &Scene, tl: &Timeline) -> Vec<Warning> {
    let mut w: Vec<Warning> = overlapping_carriers(sc, tl).into_iter().map(|text| Warning { at: None, text }).collect();
    w.extend(cut_captions(tl));
    w.extend(short_captions(tl));
    w.extend(messages_out_of_order(sc, tl));
    w
}

/// The texts of the warnings.
#[cfg(test)]
pub fn warnings(sc: &Scene, tl: &Timeline) -> Vec<String> {
    warnings_at(sc, tl).into_iter().map(|w| w.text).collect()
}

/// Physical carriers touching on the track (once for each pair and contact).
fn overlapping_carriers(sc: &Scene, tl: &Timeline) -> Vec<String> {
    let n = sc.carriers.len();
    let mut out = Vec::new();
    let mut touching = vec![false; n * n];
    let mut t = 0.0;
    while t < tl.duration {
        // Outside the panel carriers may share the spawn or exit point.
        let pos: Vec<Option<(f64, f64)>> = (0..n)
            .map(|ci| {
                let p = tl.carrier_pos(ci, t);
                (tl.carriers[ci].physical(t) && TRACK_X.contains(&p.0)).then_some(p)
            })
            .collect();
        for i in 0..n {
            for j in i + 1..n {
                let hit = match (pos[i], pos[j]) {
                    (Some(a), Some(b)) => ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt() < 2.0 * CARRIER_R - 0.5,
                    _ => false,
                };
                if hit && !touching[i * n + j] {
                    out.push(format!("carriers '{}' and '{}' overlap at t={t:.2}s", sc.carriers[i].id, sc.carriers[j].id));
                }
                touching[i * n + j] = hit;
            }
        }
        t += 0.02;
    }
    out
}

/// Captions longer than the band are cut.
fn cut_captions(tl: &Timeline) -> Vec<Warning> {
    tl.events
        .iter()
        .filter_map(|e| match &e.ev {
            Ev::Caption { title, text } => {
                let (lines, max) = caption_lines(title, text);
                (lines.len() > max).then(|| w(&e.at, format!("caption '{title}' at {:.2}s is cut: {} lines, {max} fit", e.t0, lines.len())))
            }
            _ => None,
        })
        .collect()
}

/// Messages and notes written in a different order from the one in which they
/// are sent: the video follows time, but the Mermaid diagram follows the file.
fn messages_out_of_order(sc: &Scene, tl: &Timeline) -> Vec<Warning> {
    let name = |e: &Timed| match &e.ev {
        Ev::Message { from, to, text, .. } => format!("{} -> {} {text}", sc.actors[*from].id, sc.actors[*to].id),
        Ev::Note { actor, text, .. } => format!("note {}: {text}", sc.actors[*actor].id),
        _ => String::new(),
    };
    let msgs: Vec<&Timed> = tl.events.iter().filter(|e| matches!(e.ev, Ev::Message { .. } | Ev::Note { .. })).collect();
    let mut out = Vec::new();
    for p in msgs.windows(2) {
        if p[1].t0 < p[0].t0 - 1e-6 {
            out.push(w(
                &p[1].at,
                format!(
                    "'{}' at {:.2}s is written after '{}'{} at {:.2}s: the Mermaid diagram shows them in the wrong order",
                    name(p[1]),
                    p[1].t0,
                    name(p[0]),
                    line_of(&p[0].at),
                    p[0].t0
                ),
            ));
        }
    }
    out
}

/// Shortest time (s) a caption should stay on screen to be read.
const CAPTION_MIN: f64 = 2.5;

/// Captions replaced by the next one before they can be read, or out of order
/// (a caption shows until the next one in time).
fn short_captions(tl: &Timeline) -> Vec<Warning> {
    let caps: Vec<(f64, &str, usize, &At)> = tl
        .events
        .iter()
        .filter_map(|e| match &e.ev {
            Ev::Caption { title, .. } => Some((e.t0, title.as_str(), &e.at)),
            _ => None,
        })
        .enumerate()
        .map(|(i, (t, title, at))| (t, title, i, at))
        .collect();
    let mut out = Vec::new();
    for p in caps.windows(2) {
        let ((t0, a, _, at_a), (t1, b, _, at_b)) = (p[0], p[1]);
        if t1 < t0 {
            out.push(w(at_b, format!("caption '{b}' at {t1:.2}s comes before '{a}'{} at {t0:.2}s, which is written before it", line_of(at_a))));
        }
    }
    let mut sorted = caps.clone();
    sorted.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.2.cmp(&y.2)));
    for p in sorted.windows(2) {
        let ((t0, a, _, at_a), (t1, b, _, at_b)) = (p[0], p[1]);
        if t1 - t0 < CAPTION_MIN {
            out.push(w(at_a, format!("caption '{a}' at {t0:.2}s is shown only {:.1}s before '{b}'{}", t1 - t0, line_of(at_b))));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::tests::{ok, SAMPLING};

    fn warned(steps: &str) -> Vec<String> {
        let (sc, tl) = ok(SAMPLING, steps);
        warnings(&sc, &tl)
    }
    fn msg(text: &str, at: f64, params: &str) -> String {
        format!("\n%% @step action=message at={at} from=IF to=AN text=\"{text}\" params=\"{params}\" ")
    }

    #[test]
    fn a_clean_scene_has_no_warnings() {
        assert!(warned(&msg("A", 0.0, "")).is_empty());
    }

    #[test]
    fn overlapping_carriers_are_reported_once_per_contact() {
        // Two real carriers on the same point: the second one starts there too.
        let base = SAMPLING.replace("ghost=true visible=false ", "");
        let (sc, tl) = ok(&base, &msg("A", 0.0, ""));
        let w = warnings(&sc, &tl);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("'C1' and 'G' overlap at t=0.00s"), "{w:?}");
        // A ghost on the same point is not a contact.
        assert!(warned("\n%% @step action=show carrier=G ").is_empty());
    }

    #[test]
    fn cut_captions_are_reported() {
        let long = "word ".repeat(60);
        let w = warned(&format!("\n%% @step action=caption title=Long text=\"{long}\" "));
        assert!(w[0].contains("caption 'Long'") && w[0].contains("is cut"), "{w:?}");
    }

    #[test]
    fn messages_written_out_of_time_order_are_reported() {
        let w = warned(&[msg("A", 2.0, ""), msg("B", 1.0, "")].concat());
        assert!(w.iter().any(|x| x.contains("'IF -> AN B' at 1.00s is written after 'IF -> AN A' (line ")), "{w:?}");
        // On the line of the step written later, naming the line of the other one.
        let (sc, tl) = ok(SAMPLING, &[msg("A", 2.0, ""), msg("B", 1.0, "")].concat());
        let shown: Vec<String> = warnings_at(&sc, &tl).iter().map(|w| w.show("x.mmd")).collect();
        assert!(shown.iter().any(|x| x.starts_with("x.mmd line 18: 'IF -> AN B'") && x.contains("'IF -> AN A' (line 17)")), "{shown:?}");
    }

    #[test]
    fn short_and_out_of_order_captions_are_reported() {
        let cap = |title: &str, at: f64| format!("\n%% @step action=caption at={at} title={title} text=x ");
        assert!(warned(&[cap("A", 0.0), cap("B", 3.0)].concat()).is_empty());
        let w = warned(&[cap("A", 0.0), cap("B", 1.0)].concat());
        assert!(w[0].contains("'A' at 0.00s is shown only 1.0s"), "{w:?}");
        let w = warned(&[cap("A", 5.0), cap("B", 1.0)].concat());
        assert!(w[0].contains("'B' at 1.00s comes before 'A'"), "{w:?}");
    }
}
