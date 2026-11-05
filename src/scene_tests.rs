//! Unit tests of the scene model and of the timeline.

use super::*;
use crate::toml;

/// Straight Main-Lane with an NSD, a Pit-Lane with a sampling point.
pub(crate) const SAMPLING: &str = r#"
[video]
title = "t"
tail = 1
[[point]]
id = "a"
x = 900
y = 700
[[point]]
id = "b"
x = 750
y = 700
[[point]]
id = "s"
x = 600
y = 700
[[point]]
id = "c"
x = 450
y = 700
[[lane]]
id = "main"
points = ["a", "b", "s", "c"]
[[device]]
id = "an"
kind = "analyzer"
x = 100
y = 150
w = 400
h = 150
[[device]]
id = "sp"
kind = "sampling"
at = "s"
[[device]]
id = "nsd"
kind = "nsd"
at = "b"
[[actor]]
id = "IF"
label = "Interface"
[[actor]]
id = "AN"
label = "Analyzer"
[[carrier]]
id = "C1"
sample_id = "SID-1"
route = ["a", "b", "s", "c"]
speed = 75
[[carrier]]
id = "G"
ghost = true
visible = false
sample_id = "SID-2"
route = ["a", "b", "s", "c"]
"#;

/// A Pit-Lane ending in a turnstile with a gate in front of it.
const TURNSTILE: &str = r#"
[video]
title = "t"
[[point]]
id = "a"
x = 900
y = 540
[[point]]
id = "w"
x = 640
y = 540
[[point]]
id = "g"
x = 596
y = 540
[[point]]
id = "out"
x = 200
y = 540
[[device]]
id = "ts"
kind = "turnstile"
x = 440
y = 505
[[device]]
id = "gate"
kind = "gate"
at = "g"
[[device]]
id = "an"
kind = "analyzer"
x = 200
y = 170
w = 480
h = 150
[[actor]]
id = "IF"
label = "Interface"
[[actor]]
id = "AN"
label = "Analyzer"
[[carrier]]
id = "C1"
route = ["a", "w", "ts.in", "ts.out", "out"]
speed = 100
[[carrier]]
id = "C2"
route = ["a", "w", "ts.in", "ts.out", "out"]
speed = 100
[[carrier]]
id = "G"
ghost = true
route = ["a", "w", "ts.in", "ts.out", "out"]
speed = 100
"#;

/// The scenes in scenes/ (the shared files in scenes/common/ are not scenes).
fn scene_files() -> Vec<std::path::PathBuf> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/scenes");
    let list = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path());
    list.filter(|p| p.extension().is_some_and(|e| e == "toml")).collect()
}

fn build(base: &str, steps: &str) -> Result<(Scene, Timeline), String> {
    let doc = toml::parse(&format!("{base}\n{steps}"))?;
    let sc = Scene::from_doc(&doc)?;
    let tl = Timeline::build(&sc)?;
    Ok((sc, tl))
}

pub(crate) fn ok(base: &str, steps: &str) -> (Scene, Timeline) {
    build(base, steps).unwrap_or_else(|e| panic!("unexpected error: {e}"))
}

fn err(base: &str, steps: &str, needle: &str) {
    match build(base, steps) {
        Ok(_) => panic!("expected an error containing '{needle}'"),
        Err(e) => assert!(e.contains(needle), "error '{e}' does not contain '{needle}'"),
    }
}

/// Start times of the events, in file order.
fn starts(tl: &Timeline) -> Vec<f64> {
    tl.events.iter().map(|e| (e.t0 * 1000.0).round() / 1000.0).collect()
}

fn msg(extra: &str) -> String {
    format!("[[step]]\naction = \"message\"\nfrom = \"IF\"\nto = \"AN\"\ntext = \"M\"\n{extra}\n")
}

// ------------------------------------------------------------ start times

#[test]
fn steps_without_timing_keys_run_one_after_the_other() {
    let (_, tl) = ok(SAMPLING, &[msg(""), msg(""), msg("")].concat());
    assert_eq!(starts(&tl), vec![0.0, 1.5, 3.0]);
}

#[test]
fn start_with_after_all_and_delay() {
    let steps = [msg("duration = 4"), msg("start = \"with\"\ndelay = 1"), msg("start = \"after_all\"")].concat();
    let (_, tl) = ok(SAMPLING, &steps);
    // second: starts 1 s after the first (0 + 1) and ends at 2.5; after_all waits for 4.
    assert_eq!(starts(&tl), vec![0.0, 1.0, 4.0]);
}

#[test]
fn at_is_absolute_plus_delay() {
    let (_, tl) = ok(SAMPLING, &[msg("at = 10"), msg("at = 2\ndelay = 0.5")].concat());
    assert_eq!(starts(&tl), vec![10.0, 2.5]);
}

#[test]
fn after_start_and_after_end_are_relative_to_the_previous_step() {
    let steps = [msg("at = 10"), msg("after_start = 3"), msg("after_end = 3"), msg("after_end = 1\ndelay = 0.5")].concat();
    let (_, tl) = ok(SAMPLING, &steps);
    assert_eq!(starts(&tl), vec![10.0, 13.0, 17.5, 20.5]);
}

#[test]
fn only_one_timing_key_per_step() {
    err(SAMPLING, &msg("at = 1\nafter_end = 2"), "use only one of");
    err(SAMPLING, &msg("start = \"with\"\nafter_start = 2"), "use only one of");
    err(SAMPLING, &msg("after_end = -1"), "must not be negative");
    err(SAMPLING, &msg("start = \"later\""), "unknown start");
}

#[test]
fn duplicate_ids_are_rejected() {
    let dup = "[[actor]]\nid = \"IF\"\nlabel = \"x\"\n[[step]]\naction = \"wait\"\nduration = 1\n";
    err(SAMPLING, dup, "duplicate actor id 'IF'");
}

#[test]
fn unknown_keys_and_actions_are_rejected() {
    err(SAMPLING, &msg("txet = \"typo\""), "txet");
    err(SAMPLING, "[[step]]\naction = \"jump\"\n", "unknown action");
    err(SAMPLING, "[[step]]\naction = \"pipette\"\ndevice = \"sp\"\ncarrier = \"C1\"\n", "carrier");
}

// ------------------------------------------------------------ messages

#[test]
fn messages_take_rows_and_substitute_placeholders() {
    let steps = [msg("params = \"SampleId={SampleId} C={CarrierId}\""), msg("carrier = \"G\"\nparams = \"{SampleId}\"")].concat();
    let (_, tl) = ok(SAMPLING, &steps);
    let got: Vec<(usize, String)> = tl
        .events
        .iter()
        .filter_map(|e| match &e.ev {
            Ev::Message { row, params, .. } => Some((*row, params.clone())),
            _ => None,
        })
        .collect();
    // Without `carrier` the first carrier is used.
    assert_eq!(got, vec![(0, "SampleId=SID-1 C=C1".to_string()), (1, "SID-2".to_string())]);
    assert_eq!(tl.rows, 2);
}

#[test]
fn message_errors() {
    err(SAMPLING, "[[step]]\naction = \"message\"\nfrom = \"IF\"\nto = \"IF\"\ntext = \"x\"\n", "to itself");
    err(SAMPLING, "[[step]]\naction = \"message\"\nfrom = \"IF\"\nto = \"XX\"\ntext = \"x\"\n", "unknown actor 'XX'");
    err(SAMPLING, &msg("carrier = \"C9\""), "unknown carrier 'C9'");
}

// ------------------------------------------------------------ moves

#[test]
fn move_durations_follow_speed_and_easing() {
    let steps = "[[step]]\naction = \"move\"\ncarrier = \"C1\"\nto = \"b\"\nease = \"linear\"\n\
                 [[step]]\naction = \"move\"\ncarrier = \"C1\"\nto = \"s\"\nease = \"out\"\n";
    let (_, tl) = ok(SAMPLING, steps);
    // 150 px at 75 px/s: 2 s linear, 2 * pi/2 s with easing (same peak speed).
    assert!((tl.events[0].t1 - 2.0).abs() < 1e-9);
    assert!((tl.events[1].t1 - tl.events[1].t0 - std::f64::consts::PI).abs() < 1e-9);
    assert_eq!(tl.carrier_pos(0, 100.0), (600.0, 700.0));
    // halfway through the linear move
    assert_eq!(tl.carrier_pos(0, 1.0), (825.0, 700.0));
}

#[test]
fn move_errors() {
    err(SAMPLING, "[[step]]\naction = \"move\"\ncarrier = \"C1\"\nto = \"a\"\n", "not ahead");
    err(
        SAMPLING,
        "[[step]]\naction = \"move\"\ncarrier = \"C1\"\nto = \"s\"\n[[step]]\naction = \"move\"\nstart = \"with\"\ncarrier = \"C1\"\nto = \"c\"\n",
        "still moving",
    );
}

#[test]
fn ease_curves_start_and_end_on_the_targets() {
    for e in [Ease::Linear, Ease::In, Ease::Out, Ease::InOut] {
        assert!(e.apply(0.0).abs() < 1e-12);
        assert!((e.apply(1.0) - 1.0).abs() < 1e-12);
        assert!((e.apply(2.0) - 1.0).abs() < 1e-12, "clamped");
    }
}

// ------------------------------------------------------------ ghosts and visibility

#[test]
fn show_and_hide_fade_a_carrier() {
    let steps = "[[step]]\naction = \"show\"\nat = 1\ncarrier = \"G\"\nduration = 1\n\
                 [[step]]\naction = \"hide\"\nat = 5\ncarrier = \"G\"\n";
    let (_, tl) = ok(SAMPLING, steps);
    let g = &tl.carriers[1];
    assert_eq!(g.opacity(0.5), 0.0);
    assert!((g.opacity(1.5) - 0.5).abs() < 1e-9);
    assert_eq!(g.opacity(3.0), 1.0);
    assert_eq!(g.opacity(6.0), 0.0);
    assert!(!g.physical(3.0), "a ghost is never physical");
    assert!(tl.carriers[0].physical(0.0));
}

#[test]
fn overlapping_fades_are_rejected() {
    err(
        SAMPLING,
        "[[step]]\naction = \"show\"\nat = 1\ncarrier = \"G\"\nduration = 2\n[[step]]\naction = \"hide\"\nat = 2\ncarrier = \"G\"\n",
        "still fading",
    );
}

#[test]
fn ghosts_do_not_open_gates() {
    let steps = "[[step]]\naction = \"move\"\ncarrier = \"G\"\nto = \"ts.in\"\n";
    let (_, tl) = ok(TURNSTILE, steps);
    assert!(tl.gates[0].1.is_empty());
    let steps = "[[step]]\naction = \"move\"\ncarrier = \"C1\"\nto = \"ts.in\"\n";
    let (_, tl) = ok(TURNSTILE, steps);
    assert_eq!(tl.gates[0].1.len(), 1);
}

// ------------------------------------------------------------ turnstile

const C1_IN: &str = "[[step]]\naction = \"move\"\ncarrier = \"C1\"\nto = \"ts.in\"\n";

#[test]
fn a_carrier_rides_the_wheel_and_leaves_from_the_exit() {
    let steps = format!(
        "{C1_IN}[[step]]\naction = \"rotate\"\ndevice = \"ts\"\n[[step]]\naction = \"rotate\"\ndevice = \"ts\"\n\
         [[step]]\naction = \"move\"\ncarrier = \"C1\"\nto = \"out\"\n"
    );
    let (sc, tl) = ok(TURNSTILE, &steps);
    let (nx, ny) = sc.pt("ts.needle");
    let t_needle = tl.events[1].t1 + 0.01;
    let (x, y) = tl.carrier_pos(0, t_needle);
    assert!((x - nx).abs() < 1.0 && (y - ny).abs() < 1.0, "under the needle after one rotation");
}

#[test]
fn turnstile_rules() {
    // leaving from the entry position
    err(TURNSTILE, &format!("{C1_IN}[[step]]\naction = \"move\"\ncarrier = \"C1\"\nto = \"out\"\n"), "not in the exit position");
    // a second carrier on an occupied entry
    err(
        TURNSTILE,
        &format!("{C1_IN}[[step]]\naction = \"move\"\ncarrier = \"C2\"\nto = \"ts.in\"\n"),
        "still occupied",
    );
    // driving into the wheel during a pipette
    err(
        TURNSTILE,
        &format!(
            "{C1_IN}[[step]]\naction = \"rotate\"\ndevice = \"ts\"\n[[step]]\naction = \"pipette\"\ndevice = \"ts\"\nduration = 10\n\
             [[step]]\naction = \"move\"\nstart = \"with\"\ncarrier = \"C2\"\nto = \"ts.in\"\n"
        ),
        "must wait at the gate",
    );
    err(TURNSTILE, "[[step]]\naction = \"rotate\"\ndevice = \"gate\"\n", "rotate needs a device of kind 'turnstile'");
}

// ------------------------------------------------------------ analyzer status, needle, operator

#[test]
fn status_needs_an_analyzer_and_a_known_state() {
    err(SAMPLING, "[[step]]\naction = \"status\"\ndevice = \"nsd\"\nstate = \"error\"\n", "status needs a device of kind 'analyzer' or 'zone' or 'sampling'");
    err(SAMPLING, "[[step]]\naction = \"status\"\ndevice = \"an\"\nstate = \"broken\"\n", "unknown state");
    let (_, tl) = ok(SAMPLING, "[[step]]\naction = \"status\"\ndevice = \"an\"\nstate = \"ready\"\n");
    assert!(matches!(tl.events[0].ev, Ev::Status { state: AnalyzerState::Ready, .. }));
}

#[test]
fn probe_actions_need_a_sampling_device() {
    err(SAMPLING, "[[step]]\naction = \"pipette\"\ndevice = \"nsd\"\n", "pipette needs a device of kind 'sampling' or 'turnstile'");
    err(SAMPLING, "[[step]]\naction = \"needle\"\ndevice = \"an\"\nto = \"down\"\n", "needle needs a device");
    err(SAMPLING, "[[step]]\naction = \"needle\"\ndevice = \"sp\"\nto = \"sideways\"\n", "unknown needle position");
}

const C1_TO_S: &str = "[[step]]\naction = \"move\"\ncarrier = \"C1\"\nto = \"s\"\n";

#[test]
fn a_carrier_cannot_leave_while_the_needle_is_down() {
    let down = "[[step]]\naction = \"needle\"\ndevice = \"sp\"\nto = \"down\"\n";
    let up = "[[step]]\naction = \"needle\"\ndevice = \"sp\"\nto = \"up\"\n";
    let leave = "[[step]]\naction = \"move\"\ncarrier = \"C1\"\nto = \"c\"\n";
    err(SAMPLING, &format!("{C1_TO_S}{down}{leave}"), "while the needle is down");
    ok(SAMPLING, &format!("{C1_TO_S}{down}{up}{leave}"));
}

#[test]
fn the_wheel_cannot_turn_while_the_needle_is_down() {
    let steps = format!(
        "{C1_IN}[[step]]\naction = \"rotate\"\ndevice = \"ts\"\n[[step]]\naction = \"needle\"\ndevice = \"ts\"\nto = \"down\"\n\
         [[step]]\naction = \"rotate\"\ndevice = \"ts\"\n"
    );
    err(TURNSTILE, &steps, "while the needle is down");
}

#[test]
fn operator_rules() {
    err(SAMPLING, "[[step]]\naction = \"operator\"\ndevice = \"sp\"\n", "operator needs a device of kind 'analyzer'");
    err(SAMPLING, "[[step]]\naction = \"operator\"\ndevice = \"an\"\nduration = 1\n", "at least 2 s");
    let (_, tl) = ok(SAMPLING, "[[step]]\naction = \"operator\"\ndevice = \"an\"\n");
    assert_eq!(tl.events[0].t1, 7.0);
}

// ------------------------------------------------------------ the shipped scenes

#[test]
fn all_scenes_in_the_repository_are_valid() {
    let mut n = 0;
    for path in scene_files() {
        let doc = toml::parse_file(&path).unwrap();
        let sc = Scene::from_doc(&doc).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        Timeline::build(&sc).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        n += 1;
    }
    assert!(n >= 6, "found only {n} scenes");
}

#[test]
fn runaway_moves_are_flagged_only_while_moving() {
    let (_, tl) = ok(SAMPLING, "[[step]]\naction = \"move\"\nat = 1\ncarrier = \"C1\"\nto = \"c\"\nease = \"linear\"\nrunaway = true\n");
    let c = &tl.carriers[0];
    assert!(!c.running_away(0.5));
    assert!(c.running_away(2.0));
    assert!(!c.running_away(100.0));
}

#[test]
fn the_wheel_waits_until_a_leaving_carrier_is_clear() {
    let base = format!(
        "{C1_IN}[[step]]\naction = \"rotate\"\ndevice = \"ts\"\n[[step]]\naction = \"rotate\"\ndevice = \"ts\"\n\
         [[step]]\naction = \"move\"\ncarrier = \"C1\"\nto = \"out\"\nease = \"linear\"\n"
    );
    // ts.out is 70 px from the center; clear means r + rim + carrier = 124 px.
    err(TURNSTILE, &format!("{base}[[step]]\naction = \"rotate\"\nstart = \"with\"\ndelay = 0.2\ndevice = \"ts\"\n"), "not yet clear of the wheel");
    ok(TURNSTILE, &format!("{base}[[step]]\naction = \"rotate\"\nstart = \"with\"\ndelay = 1.0\ndevice = \"ts\"\n"));
}

#[test]
fn status_also_works_on_zones() {
    let base = SAMPLING.replace("[[device]]\nid = \"sp\"", "[[device]]\nid = \"zone\"\nkind = \"zone\"\nx = 90\ny = 400\nw = 700\nh = 400\n[[device]]\nid = \"sp\"");
    let (_, tl) = ok(&base, "[[step]]\naction = \"status\"\ndevice = \"zone\"\nstate = \"error\"\n");
    assert!(matches!(tl.events[0].ev, Ev::Status { state: AnalyzerState::Error, .. }));
    err(&base, "[[step]]\naction = \"status\"\ndevice = \"nsd\"\nstate = \"error\"\n", "status needs a device of kind 'analyzer' or 'zone' or 'sampling'");
}

/// Physical carriers never overlap on the track (checked on the scenes with
/// several analyzers, where carriers stop and queue the most).
#[test]
fn carriers_do_not_overlap_in_the_multi_analyzer_scenes() {
    let mut n = 0;
    for path in scene_files() {
        if !path.file_name().unwrap().to_str().unwrap().contains("_spur2_") {
            continue;
        }
        n += 1;
        let doc = toml::parse_file(&path).unwrap();
        let sc = Scene::from_doc(&doc).unwrap();
        let tl = Timeline::build(&sc).unwrap();
        let mut t = 0.0;
        while t < tl.duration {
            let pos: Vec<Option<(f64, f64)>> = (0..sc.carriers.len())
                .map(|ci| {
                    let p = tl.carrier_pos(ci, t);
                    // Outside the panel carriers may share the spawn or exit point.
                    (tl.carriers[ci].physical(t) && (30.0..950.0).contains(&p.0)).then_some(p)
                })
                .collect();
            for i in 0..pos.len() {
                for j in i + 1..pos.len() {
                    if let (Some(a), Some(b)) = (pos[i], pos[j]) {
                        let d = ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
                        assert!(d >= 2.0 * CARRIER_R, "{}: {} and {} overlap at t={t:.2}s ({d:.0} px)", path.display(), sc.carriers[i].id, sc.carriers[j].id);
                    }
                }
            }
            t += 0.02;
        }
    }
    assert_eq!(n, 4);
}

/// Domain rules on the message order, checked on every scene: the analyzer sends
/// one Query at a time (the next one only after the reply to the previous one has
/// arrived), and a sample gets Sample Acquisition only after its tests are known.
#[test]
fn queries_to_dmx_are_sent_one_at_a_time() {
    for path in scene_files() {
        let doc = toml::parse_file(&path).unwrap();
        let sc = Scene::from_doc(&doc).unwrap();
        let tl = Timeline::build(&sc).unwrap();
        let mut msgs: Vec<(f64, f64, &str, &str)> = tl
            .events
            .iter()
            .filter_map(|e| match &e.ev {
                Ev::Message { text, params, .. } => Some((e.t0, e.t1, text.as_str(), params.as_str())),
                _ => None,
            })
            .collect();
        msgs.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut waiting = false;
        let mut free_at = f64::NEG_INFINITY;
        for &(t0, t1, text, params) in &msgs {
            match text {
                "Query" => {
                    assert!(!waiting && t0 >= free_at, "{}: Query at {t0:.2}s before the previous reply has arrived ({params})", path.display());
                    waiting = true;
                }
                "Tests to perform" => {
                    assert!(waiting, "{}: reply at {t0:.2}s without a Query", path.display());
                    waiting = false;
                    free_at = t1;
                }
                "Sample Acquisition" => {
                    let id = params.split_whitespace().next().unwrap();
                    let known = msgs.iter().any(|m| m.2 == "Tests to perform" && m.3.split_whitespace().next() == Some(id) && m.1 <= t0);
                    assert!(known, "{}: Sample Acquisition at {t0:.2}s before the tests of {id} are known", path.display());
                }
                _ => {}
            }
        }
    }
}

/// The videos never show the internal term "NSD": no caption, label or message.
#[test]
fn the_videos_do_not_mention_the_nsd() {
    let bad = |what: &str, text: &str, path: &std::path::Path| {
        assert!(!text.contains("NSD") && !text.contains("No Stop"), "{}: {what}: {text}", path.display());
    };
    for path in scene_files() {
        let doc = toml::parse_file(&path).unwrap();
        let sc = Scene::from_doc(&doc).unwrap();
        let tl = Timeline::build(&sc).unwrap();
        bad("title", &sc.video.title, &path);
        bad("subtitle", &sc.video.subtitle, &path);
        sc.devices.iter().for_each(|d| bad("device", &d.label, &path));
        sc.lanes.iter().for_each(|l| bad("lane", &l.label, &path));
        for e in &tl.events {
            match &e.ev {
                Ev::Caption { title, text } => {
                    bad("caption", title, &path);
                    bad("caption", text, &path);
                }
                Ev::Message { text, params, .. } => {
                    bad("message", text, &path);
                    bad("message", params, &path);
                }
                Ev::Note { text, .. } => bad("note", text, &path),
                _ => {}
            }
        }
    }
}
