//! Warnings on a valid scene: things that render without errors but look wrong.
//! They are printed by `--check` and when rendering, and don't stop anything.

use crate::render::caption_lines;
use crate::scene::*;

const TRACK_X: std::ops::Range<f64> = 30.0..950.0;

pub fn warnings(sc: &Scene, tl: &Timeline) -> Vec<String> {
    let mut w = overlapping_carriers(sc, tl);
    w.extend(cut_captions(tl));
    w.extend(queries(tl));
    w
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
                    (Some(a), Some(b)) => ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt() < 2.0 * CARRIER_R,
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
fn cut_captions(tl: &Timeline) -> Vec<String> {
    tl.events
        .iter()
        .filter_map(|e| match &e.ev {
            Ev::Caption { title, text } => {
                let (lines, max) = caption_lines(title, text);
                (lines.len() > max).then(|| format!("caption '{title}' at {:.2}s is cut: {} lines, {max} fit", e.t0, lines.len()))
            }
            _ => None,
        })
        .collect()
}

/// The analyzer sends one Query at a time, after the OK to the Prepare of that sample.
fn queries(tl: &Timeline) -> Vec<String> {
    let mut msgs: Vec<(f64, f64, &str, &str)> = tl
        .events
        .iter()
        .filter_map(|e| match &e.ev {
            Ev::Message { text, params, .. } => Some((e.t0, e.t1, text.as_str(), params.as_str())),
            _ => None,
        })
        .collect();
    msgs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = Vec::new();
    let (mut waiting, mut free_at) = (false, f64::NEG_INFINITY);
    for &(t0, t1, text, params) in &msgs {
        match text {
            "Query" => {
                if waiting || t0 < free_at {
                    out.push(format!("Query at {t0:.2}s ({params}) before the reply to the previous one has arrived"));
                }
                if !msgs.iter().any(|m| m.2 == "OK" && m.3 == params && m.1 <= t0) {
                    out.push(format!("Query at {t0:.2}s ({params}) before the OK to its Prepare has arrived"));
                }
                waiting = true;
            }
            "Tests to perform" => {
                waiting = false;
                free_at = t1;
            }
            _ => {}
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
        format!("[[step]]\naction = \"message\"\nat = {at}\nfrom = \"IF\"\nto = \"AN\"\ntext = \"{text}\"\nparams = \"{params}\"\n")
    }

    #[test]
    fn a_clean_scene_has_no_warnings() {
        assert!(warned(&msg("A", 0.0, "")).is_empty());
    }

    #[test]
    fn overlapping_carriers_are_reported_once_per_contact() {
        // Two real carriers on the same point: the second one starts there too.
        let base = SAMPLING.replace("ghost = true\nvisible = false\n", "");
        let (sc, tl) = ok(&base, &msg("A", 0.0, ""));
        let w = warnings(&sc, &tl);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("'C1' and 'G' overlap at t=0.00s"), "{w:?}");
        // A ghost on the same point is not a contact.
        assert!(warned("[[step]]\naction = \"show\"\ncarrier = \"G\"\n").is_empty());
    }

    #[test]
    fn cut_captions_are_reported() {
        let long = "word ".repeat(60);
        let w = warned(&format!("[[step]]\naction = \"caption\"\ntitle = \"Long\"\ntext = \"{long}\"\n"));
        assert!(w[0].contains("caption 'Long'") && w[0].contains("is cut"), "{w:?}");
    }

    #[test]
    fn queries_are_sent_one_at_a_time() {
        let (q, t) = ("Query", "Tests to perform");
        let ok = |n: &str, at: f64| msg("OK", at, &format!("SampleId={n}"));
        let good = [ok("1", -2.0), ok("2", -1.0), msg(q, 0.0, "SampleId=1"), msg(t, 2.0, "SampleId=1"), msg(q, 3.5, "SampleId=2"), msg(t, 5.0, "SampleId=2")];
        assert!(warned(&good.concat()).is_empty());
        // Before the OK to its Prepare has arrived.
        let w = warned(&[ok("1", 0.0), msg(q, 1.0, "SampleId=1")].concat());
        assert!(w[0].contains("before the OK to its Prepare"), "{w:?}");
        // Before the first reply has been sent, and before it has arrived.
        let w = warned(&[ok("1", -2.0), ok("2", -2.0), msg(q, 0.0, "SampleId=1"), msg(q, 1.0, "SampleId=2")].concat());
        assert!(w[0].contains("Query at 1.00s"), "{w:?}");
        let w = warned(&[ok("1", -2.0), ok("2", -2.0), msg(q, 0.0, "SampleId=1"), msg(t, 1.0, "SampleId=1"), msg(q, 2.0, "SampleId=2")].concat());
        assert!(w[0].contains("Query at 2.00s"), "{w:?}");
        // Sample Acquisition may come before the tests are known.
        let s = "Sample Acquisition";
        assert!(warned(&[ok("1", -2.0), msg(q, 0.0, "SampleId=1"), msg(s, 0.5, "SampleId=1"), msg(t, 2.0, "SampleId=1")].concat()).is_empty());
    }
}
