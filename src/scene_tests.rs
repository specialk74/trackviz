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

/// The scenes in scenes/, Mermaid diagrams (the shared files are in scenes/common/).
fn scene_files() -> Vec<std::path::PathBuf> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/scenes");
    let list = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path());
    list.filter(|p| p.extension().is_some_and(|e| e == "mmd")).collect()
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
fn rows_follow_time_whatever_the_file_order() {
    let (_, tl) = ok(SAMPLING, &[msg("at = 5\ntext = \"\"").replace("text = \"M\"\n", ""), msg("at = 2")].concat());
    let row_of = |i: usize| match tl.events[i].ev {
        Ev::Message { row, .. } => row,
        _ => panic!(),
    };
    assert_eq!((row_of(0), row_of(1)), (1, 0));
}

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
    let list = scene_files();
    assert!(list.len() >= 17, "found only {} scenes", list.len());
    for path in list {
        crate::load(&path.to_string_lossy()).unwrap_or_else(|e| panic!("{e}"));
    }
}

#[test]
fn a_runaway_stays_flagged_during_a_stop() {
    let steps = "[[step]]\naction = \"move\"\nat = 1\ncarrier = \"C1\"\nto = \"b\"\nease = \"linear\"\nrunaway = true\n\
                 [[step]]\naction = \"move\"\nat = 5\ncarrier = \"C1\"\nto = \"c\"\nease = \"linear\"\nrunaway = true\n";
    let (_, tl) = ok(SAMPLING, steps);
    let c = &tl.carriers[0];
    assert!(c.running_away(2.0) && c.running_away(4.0) && c.running_away(6.0));
    assert!(!c.running_away(0.5) && !c.running_away(100.0));
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

/// The videos never show the internal term "NSD": no caption, label or message.
#[test]
fn the_videos_do_not_mention_the_nsd() {
    let bad = |what: &str, text: &str, path: &std::path::Path| {
        assert!(!text.contains("NSD") && !text.contains("No Stop"), "{}: {what}: {text}", path.display());
    };
    for path in scene_files() {
        let (sc, tl) = crate::load(&path.to_string_lossy()).unwrap_or_else(|e| panic!("{e}"));
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

const TYPES: &str = "[[message]]\nid = \"acq\"\nfrom = \"IF\"\nto = \"AN\"\ntext = \"Acquire\"\nparams = \"SampleId={SampleId} Needle={Needle}\"\nreply = true\ncolor = \"#123456\"\n";

#[test]
fn named_messages_take_everything_from_their_type() {
    let (_, tl) = ok(SAMPLING, &format!("{TYPES}[[step]]\naction = \"message\"\ntype = \"acq\"\ncarrier = \"G\"\nneedle = 2\n"));
    match &tl.events[0].ev {
        Ev::Message { from, to, text, params, reply, color, .. } => {
            assert_eq!((*from, *to, text.as_str(), params.as_str(), *reply), (0, 1, "Acquire", "SampleId=SID-2 Needle=2", true));
            assert_eq!(color.as_deref(), Some("#123456"));
        }
        _ => panic!("not a message"),
    }
}

#[test]
fn named_message_errors() {
    let step = "[[step]]\naction = \"message\"\n";
    err(SAMPLING, &format!("{step}type = \"nope\"\n"), "unknown message type 'nope'");
    err(SAMPLING, &format!("{TYPES}{step}type = \"acq\"\n"), "add a 'needle' key");
    // A step with a type doesn't redefine it.
    err(SAMPLING, &format!("{TYPES}{step}type = \"acq\"\nneedle = 1\ntext = \"x\"\n"), "unknown key 'text'");
    err(SAMPLING, &format!("{TYPES}{TYPES}{step}type = \"acq\"\nneedle = 1\n"), "duplicate message id 'acq'");
}

fn notes(tl: &Timeline) -> Vec<(String, f64, f64)> {
    tl.events.iter().filter_map(|e| match &e.ev { Ev::Note { text, .. } => Some((text.clone(), e.t0, e.t1)), _ => None }).collect()
}

#[test]
fn pipette_adds_its_note_on_the_analyzer_actor() {
    let steps = "[[step]]\naction = \"pipette\"\nat = 1\ndevice = \"sp\"\nduration = 4\n";
    let with_actor = SAMPLING.replace("label = \"Analyzer\"\n[[carrier]]", "label = \"Analyzer\"\nanalyzer = true\n[[carrier]]");
    assert_ne!(with_actor, SAMPLING);
    let (_, tl) = ok(&with_actor, steps);
    assert_eq!(notes(&tl), vec![("Pipetting".to_string(), 1.0, 5.0)]);
    assert_eq!(tl.rows, 1);
    // Several sampling points: it names the needle.
    let two = with_actor.replace("[[device]]\nid = \"sp\"\nkind = \"sampling\"\nat = \"s\"", "[[device]]\nid = \"sp1\"\nkind = \"sampling\"\nat = \"c\"\n[[device]]\nid = \"sp\"\nkind = \"sampling\"\nat = \"s\"");
    let (_, tl) = ok(&two, steps);
    assert_eq!(notes(&tl)[0].0, "Pipetting · Needle 2");
    // No analyzer actor: no note.
    let (_, tl) = ok(SAMPLING, steps);
    assert!(notes(&tl).is_empty());
}

#[test]
fn no_scene_has_warnings() {
    for path in scene_files() {
        let (sc, tl) = crate::load(&path.to_string_lossy()).unwrap_or_else(|e| panic!("{e}"));
        let w = crate::lint::warnings(&sc, &tl);
        assert!(w.is_empty(), "{}: {w:?}", path.display());
    }
}

// ------------------------------------------------------------ when

const C1_MOVES: &str = "[[step]]\naction = \"move\"\nat = 1\ncarrier = \"C1\"\nto = \"c\"\nease = \"linear\"\n";

#[test]
fn when_starts_a_step_as_the_carrier_reaches_a_point() {
    // a -> b -> s -> c: 450 px at 75 px/s, starting at t=1.
    let (_, tl) = ok(SAMPLING, &[C1_MOVES, &msg("when = \"C1 at c\"")].concat());
    assert!((tl.events[1].t0 - 7.0).abs() < 1e-9);
    // A point crossed in the middle of the move, with a delay.
    let (_, tl) = ok(SAMPLING, &[C1_MOVES, &msg("when = \"C1 at b\"\ndelay = 0.5")].concat());
    assert!((tl.events[1].t0 - 3.5).abs() < 0.03, "{}", tl.events[1].t0);
    // It follows the move: a slower carrier moves the message too.
    let slow = SAMPLING.replace("speed = 75", "speed = 150");
    let (_, tl) = ok(&slow, &[C1_MOVES, &msg("when = \"C1 at c\"")].concat());
    assert!((tl.events[1].t0 - 4.0).abs() < 1e-9);
}

#[test]
fn when_follows_messages_and_actions() {
    let chain = format!(
        "{TYPES}[[step]]\naction = \"message\"\ntype = \"acq\"\nat = 2\ncarrier = \"C1\"\nneedle = 1\n\
         [[step]]\naction = \"pipette\"\nwhen = \"acq C1 arrived\"\ndevice = \"sp\"\nduration = 4\n\
         [[step]]\naction = \"message\"\ntype = \"acq\"\nwhen = \"pipette sp done\"\ndelay = 0.5\ncarrier = \"G\"\nneedle = 2\n\
         [[step]]\naction = \"wait\"\nwhen = \"acq sent\"\nduration = 1\n[[step]]\naction = \"wait\"\nwhen = \"acq C1 sent\"\nduration = 1\n"
    );
    let (_, tl) = ok(SAMPLING, &chain);
    // 2 + 1.5 flight = 3.5; + 4 pipette + 0.5 = 8; the latest "acq"; C1's own.
    assert_eq!(starts(&tl), vec![2.0, 3.5, 8.0, 8.0, 2.0]);
}

#[test]
fn when_errors() {
    err(SAMPLING, &[C1_MOVES, &msg("when = \"C1 near c\"")].concat(), "'when' is");
    err(SAMPLING, &msg("when = \"query C1 sent\""), "no earlier step matches 'query C1'");
    err(SAMPLING, &msg("when = \"pipette sp done\""), "no earlier step matches 'pipette sp'");
    err(SAMPLING, &[C1_MOVES, &msg("when = \"C9 at c\"")].concat(), "unknown carrier 'C9'");
    err(SAMPLING, &[C1_MOVES, &msg("when = \"C1 at nowhere\"")].concat(), "not on the route");
    err(SAMPLING, &[C1_MOVES, &msg("when = \"C1 at c\"\nat = 2")].concat(), "use only one of");
    // The move comes later in the file.
    err(SAMPLING, &[&msg("when = \"C1 at c\""), C1_MOVES].concat(), "must come before this step");
}

/// The scenes are a chain of causes: slowing the carriers moves everything after.
#[test]
fn a_slower_carrier_moves_the_whole_scene() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/scenes/09_spur_happy_path.mmd");
    let times = |slow: bool| {
        let (mut doc, _) = crate::scene_doc(path).unwrap();
        if slow {
            for c in doc.arrays.get_mut("carrier").unwrap() {
                c.entries.insert("speed".into(), toml::Value::Num(80.0));
            }
        }
        let sc = Scene::from_doc(&doc).unwrap();
        let tl = Timeline::build(&sc).unwrap();
        let of = |name: &str| tl.events.iter().find(|e| matches!(&e.ev, Ev::Message { text, .. } if text == name)).unwrap().t0;
        let pipette = tl.events.iter().find(|e| matches!(e.ev, Ev::Pipette { .. })).unwrap().t0;
        (of("Sample Acquisition"), pipette, of("Sample Complete"), of("Release"))
    };
    let (a, b) = (times(false), times(true));
    assert!(b.0 > a.0 + 1.0 && b.1 > a.1 + 1.0 && b.2 > a.2 + 1.0 && b.3 > a.3 + 1.0, "{a:?} {b:?}");
}
