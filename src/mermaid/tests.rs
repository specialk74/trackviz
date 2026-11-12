use super::*;
use crate::doc::{Doc, Value};
use crate::lint;
use crate::scene::{Ev, Scene, Timeline};
use std::path::{Path, PathBuf};

/// The messages and notes of a timeline as Mermaid arrows, in the order of the
/// sequence diagram rows.
fn arrows(sc: &Scene, tl: &Timeline) -> Vec<String> {
    let mut rows: Vec<(usize, String)> = tl
        .events
        .iter()
        .filter_map(|e| match &e.ev {
            Ev::Message { from, to, text, params, reply, row, .. } => {
                let arrow = if *reply { "-->>" } else { "->>" };
                let label = if params.is_empty() { text.clone() } else { format!("{text} {params}") };
                Some((*row, format!("{}{arrow}{}: {label}", sc.actors[*from].id, sc.actors[*to].id)))
            }
            Ev::Note { actor, text, row } => Some((*row, format!("Note over {}: {text}", sc.actors[*actor].id))),
            _ => None,
        })
        .collect();
    rows.sort_by_key(|r| r.0);
    rows.into_iter().map(|r| r.1).collect()
}

const SCENES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/scenes");

/// A diagram given as text, with the shared files of scenes/common.
fn doc(src: &str) -> Result<Doc, String> {
    let mut im = Importer::new(Some(PathBuf::from(SCENES)));
    for name in SHARED {
        im.include(&Path::new(SCENES).join("common").join(format!("{name}.mmd")))?;
    }
    for (i, t) in src.lines().enumerate() {
        im.read(&Line { file: String::new(), no: i + 1, text: t.to_string() })?;
    }
    im.finish("test")
}

fn build(d: &Doc) -> (Scene, Timeline) {
    let sc = Scene::from_doc(d).unwrap_or_else(|e| panic!("{e}"));
    let tl = Timeline::build(&sc).unwrap_or_else(|e| panic!("{e}"));
    (sc, tl)
}

/// Does a table of the section hold all these keys with these values?
fn has(d: &Doc, section: &str, keys: &[(&str, &str)]) -> bool {
    let same = |v: &Value, want: &str| match v {
        Value::Str(s) => s == want,
        Value::Num(n) => want.parse::<f64>().is_ok_and(|w| w == *n),
        Value::Bool(b) => want == if *b { "true" } else { "false" },
        Value::Arr(_) => false,
    };
    d.arrays.get(section).is_some_and(|ts| ts.iter().any(|t| keys.iter().all(|(k, w)| t.entries.get(*k).is_some_and(|v| same(v, w)))))
}

fn fails(src: &str, what: &str) {
    let e = doc(src).err().unwrap_or_else(|| panic!("expected an error with '{what}'"));
    assert!(e.contains(what), "'{e}' does not contain '{what}'");
}

/// The arrows of a diagram, without comments and layout.
fn lines(s: &str) -> Vec<String> {
    s.lines().map(str::trim).filter(|l| l.contains("->>") || l.starts_with("Note")).filter(|l| !l.starts_with("%%")).map(String::from).collect()
}

const EX: &str = "%% geometry spur\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\nIF->>AN: Prepare SampleId=SID-1\nAN-->>IF: OK SampleId=SID-1\nAN->>DMX: Query SampleId=SID-1\nDMX-->>AN: Tests to perform SampleId=SID-1\nIF->>AN: Sample Acquisition SampleId=SID-1\nAN-->>IF: OK\n%% pipette antenna2\nNote over AN: Pipetting\nAN->>IF: Sample Complete sid=SID-1\nIF-->>AN: OK\n";

#[test]
fn a_diagram_builds_a_valid_scene_with_the_messages_written() {
    let (sc, tl) = build(&doc(EX).unwrap());
    assert!(lint::warnings(&sc, &tl).is_empty(), "{:?}", lint::warnings(&sc, &tl));
    assert_eq!(arrows(&sc, &tl), lines(EX));
}

const HEAD: &str = "%% geometry spur\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n";

#[test]
fn messages_use_the_library_and_color_picks_the_purple_prepare() {
    let src = format!("{HEAD}IF->>AN: Prepare SampleId=SID-1\nAN-->>IF: OK SampleId=SID-1\n%% color #9333EA\nIF->>AN: Prepare SampleId=SID-1\nIF->>AN: Sample Acquisition SampleId=SID-1 Needle=2\n%% color #DC2626\nIF->>AN: Prepare SampleId=SID-1\n");
    let d = doc(&src).unwrap();
    for ty in ["prepare", "sample_ok", "prepare_again"] {
        assert!(has(&d, "step", &[("type", ty), ("carrier", "C1")]), "{ty}");
    }
    assert!(has(&d, "step", &[("type", "sample_acquisition_needle"), ("needle", "2")]));
    // Another color is not a library message: the text with that color.
    assert!(has(&d, "step", &[("text", "Prepare"), ("color", "#DC2626")]));
}

#[test]
fn participants_are_found_by_label_and_new_ones_are_added() {
    let d = doc("%% geometry spur\nsequenceDiagram\nparticipant I as Interface\nparticipant L as Lab\n%% carrier id=C1 sid=SID-1\nI->>AN: Prepare SampleId=SID-1\nI->>L: Hello\n").unwrap();
    assert!(has(&d, "step", &[("type", "prepare")]));
    assert!(has(&d, "actor", &[("id", "L"), ("label", "Lab")]));
    assert!(has(&d, "step", &[("from", "IF"), ("to", "L"), ("text", "Hello")]));
}

#[test]
fn timing_directives_apply_to_the_next_step() {
    let d = doc(&format!("{HEAD}%% move C1 to nsd ease=in\n%% on antenna1\nIF->>AN: Prepare SampleId=SID-1\n%% at 3\n%% highlight nsd duration=2\n%% after 0.5\n%% caption a | b\n")).unwrap();
    assert!(has(&d, "step", &[("when", "C1 at nsd"), ("type", "prepare")]));
    assert!(has(&d, "step", &[("at", "3"), ("device", "nsd"), ("duration", "2")]));
    assert!(has(&d, "step", &[("after", "0.5"), ("action", "caption")]));
    assert!(has(&d, "step", &[("action", "move"), ("ease", "in")]) && !has(&d, "step", &[("action", "move"), ("when", "C1 at nsd")]));
}

#[test]
fn for_duration_and_runaway() {
    let d = doc(&format!("{HEAD}%% move C1 to nsd runaway\n%% for C1\nAN-->>IF: OK\n%% duration 6\nNote over AN: Busy\n")).unwrap();
    assert!(has(&d, "step", &[("action", "move"), ("runaway", "true")]));
    assert!(has(&d, "step", &[("type", "analyzer_ok"), ("carrier", "C1")]));
    assert!(has(&d, "step", &[("action", "note"), ("duration", "6"), ("actor", "AN")]));
}

const ON: &str = "%% geometry spur2\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n%% carrier id=C2 sid=SID-2\n\
    %% move C1 to d1\n%% move C1 to s1\n%% at 0\n%% move C2 to w1\n\
    %% on antenna1\n%% id tube1\nIF->>AN: Add Tube SampleId=SID-1\nAN-->>IF: Fine\nAN->>DMX: Which tests? SampleId=SID-1\nDMX-->>AN: These\n\
    %% on antenna1 C2\n%% caption Second | The next one.\n%% on antenna1\nIF->>AN: Add Tube SampleId=SID-2\n%% after 0.2 tube1\nAN-->>IF: Late\n\
    %% color #9333EA\n%% on antenna3\nIF->>AN: Again SampleId=SID-1\n%% after 1\nIF->>AN: Go SampleId=SID-1\n\
    %% after 0.5 antenna3 C1\n%% caption Read | x\n%% pipette antenna4 duration=2\n%% after 0.2 pipette antenna4\nAN->>IF: Done sid=SID-1\n";

#[test]
fn on_after_and_color_set_the_moment_and_the_look() {
    let d = doc(ON).unwrap();
    // The text as written, at the antenna of its carrier.
    assert!(has(&d, "step", &[("text", "Add Tube"), ("params", "SampleId={SampleId}"), ("carrier", "C1"), ("when", "C1 at nsd")]));
    assert!(has(&d, "step", &[("action", "caption"), ("when", "C2 at nsd")]) && has(&d, "step", &[("carrier", "C2"), ("when", "C2 at nsd")]));
    assert!(has(&d, "step", &[("text", "Again"), ("when", "C1 at d1"), ("color", "#9333EA")]));
    assert!(has(&d, "step", &[("text", "Add Tube"), ("id", "tube1")]));
    assert!(has(&d, "step", &[("text", "Late"), ("when", "step tube1 done"), ("delay", "0.2")]));
    assert!(has(&d, "step", &[("text", "Go"), ("after", "1")]));
    assert!(has(&d, "step", &[("action", "caption"), ("when", "C1 at d1"), ("delay", "0.5")]));
    assert!(has(&d, "step", &[("text", "Done"), ("when", "pipette sp1 done"), ("delay", "0.2")]));
    let (_, tl) = build(&d);
    let at = |name: &str| tl.events.iter().find(|e| matches!(&e.ev, Ev::Message { text, .. } if text == name)).unwrap().t0;
    let same = |a: f64, b: f64| assert!((a - b).abs() < 0.011, "{a} != {b}");
    // Without a directive a message starts when the one before has arrived.
    same(at("Fine"), at("Add Tube") + 1.5);
    same(at("Late"), at("Add Tube") + 1.5 + 0.2);
    same(at("Go"), at("Again") + 1.5 + 1.0);
}

#[test]
fn after_an_id_waits_for_the_end_of_that_line() {
    // The id of a move: it ends when the carrier arrives.
    let d = doc(&format!("{ON_HEAD}%% id go\n%% move C1 to d1\nIF->>AN: Here SampleId=SID-1\n%% id ok\nAN-->>IF: Fine\n%% after 0.5 go\nIF->>AN: Later SampleId=SID-1\n%% after 0.25 ok\nIF->>AN: Last SampleId=SID-1\n")).unwrap();
    assert!(has(&d, "step", &[("action", "move"), ("id", "go")]));
    let (_, tl) = build(&d);
    let at = |name: &str| tl.events.iter().find(|e| matches!(&e.ev, Ev::Message { text, .. } if text == name)).unwrap().t0;
    let same = |a: f64, b: f64| assert!((a - b).abs() < 0.011, "{a} != {b}");
    same(at("Later"), at("Here") + 0.5);
    same(at("Last"), at("Fine") + 1.5 + 0.25);
}

#[test]
fn br_also_ends_a_word() {
    // The parameters are read across the lines of the label.
    let d = doc(&format!("{ON_HEAD}IF->>AN: OK SampleId=SID-1<br/>Needle=1\nIF->>AN: EAC^U07<br/>SAC-3: SID-1\n")).unwrap();
    assert!(has(&d, "step", &[("text", "OK"), ("carrier", "C1"), ("needle", "1"), ("params", "SampleId={SampleId} <br/> Needle={Needle}")]));
    assert!(has(&d, "step", &[("text", "EAC^U07 <br/> SAC-3: SID-1")]));
}

#[test]
fn waiting_for_a_carrier_stuck_in_a_queue_names_the_one_in_front() {
    let src = "%% geometry spur2\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n%% carrier id=C2 sid=SID-2\n\
        %% move C1 to antenna2\n%% at 1\n%% move C2 to antenna2\n%% on antenna2 C2\nIF->>AN: A\n";
    let sc = Scene::from_doc(&doc(src).unwrap()).unwrap();
    let e = Timeline::build(&sc).err().unwrap();
    assert!(e.contains("'C2' never gets to antenna2: after line 7 it stops behind 'C1', which stays on antenna2 because no step in the file sends it on (e.g. pass antenna2 C1)"), "{e}");
}

#[test]
fn a_fast_carrier_goes_in_before_a_slow_one_that_gets_to_the_merge_later() {
    // C1 is sampled on needle 1; C2 creeps up the spur. When C1 leaves its pit-lane
    // it gets to the merge (rj1) long before C2: it goes in front, C2 follows.
    let src = "%% geometry spur2\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n%% carrier id=C2 sid=SID-2\n\
        %% move C1 to antenna3\n%% at 0.5\n%% move C2 to antenna3\n%% on antenna3 C1\n%% divert antenna3 C1\n\
        %% on antenna4 C1\n%% id at-s1\n%% pass antenna3 C2 duration=15\n%% after 1 at-s1\n%% pass antenna4 C1\n\
        %% on antenna5 C1\n%% pass antenna5 C1\n%% on antenna5 C2\n%% pass antenna5 C2\n";
    let sc = Scene::from_doc(&doc(src).unwrap()).unwrap();
    let tl = Timeline::build(&sc).unwrap();
    let at_d2 = |c: &str| -> f64 {
        let l = tl.log.iter().find(|l| l.contains(&format!("move      {c} -> d2"))).unwrap();
        assert!(!l.contains("waits"), "{l}");
        l.split_whitespace().nth(2).unwrap().trim_end_matches('s').parse().unwrap()
    };
    assert!(at_d2("C1") < at_d2("C2"), "{:?}", tl.log);
}

#[test]
fn a_pass_written_before_the_pass_of_the_carrier_in_front_is_right() {
    // C1 and C2 both on needle 1; the pass of C2 is written before the one of C1,
    // which happens earlier: when C2 is on antenna4, C1 has already gone.
    let src = "%% geometry spur2\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n%% carrier id=C2 sid=SID-2\n\
        %% move C1 to antenna3\n%% at 0.5\n%% move C2 to antenna3\n%% on antenna3 C1\n%% divert antenna3 C1\n\
        %% on antenna3 C2\n%% divert antenna3 C2\n%% on antenna4 C2\n%% pass antenna4 C2\n\
        %% on antenna4 C1\n%% id at-s1\n%% caption a | b\n%% after 1 at-s1\n%% pass antenna4 C1\n";
    let sc = Scene::from_doc(&doc(src).unwrap()).unwrap();
    let tl = Timeline::build(&sc).unwrap_or_else(|e| panic!("{e}"));
    assert!(tl.log.iter().any(|l| l.contains("C2 -> d2")), "{:?}", tl.log);
}

#[test]
fn pass_and_divert_take_a_duration_or_a_speed() {
    let head = "%% geometry spur2\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n%% move C1 to antenna3\n";
    // How long the trip after the pass takes (the last move in the log).
    let trip = |body: &str| {
        let sc = Scene::from_doc(&doc(&format!("{head}{body}")).unwrap()).unwrap();
        let tl = Timeline::build(&sc).unwrap();
        let l = tl.log.iter().rfind(|l| l.contains("move      C1")).unwrap().clone();
        let t: Vec<f64> = l.split_whitespace().take(3).filter(|w| *w != "-").map(|w| w.trim_end_matches('s').parse().unwrap()).collect();
        t[1] - t[0]
    };
    let free = trip("%% divert antenna3 C1\n");
    assert!((trip("%% divert antenna3 C1 duration=4\n") - 4.0).abs() < 0.02);
    assert!((trip("%% pass antenna3 C1 duration=6\n") - 6.0).abs() < 0.02);
    // Half the speed of the belt: twice the time.
    let belt = Scene::from_doc(&doc(head).unwrap()).unwrap().carriers[0].speed;
    assert!((trip(&format!("%% divert antenna3 C1 speed={}\n", belt / 2.0)) - 2.0 * free).abs() < 0.05);
    fails(&format!("{head}%% pass antenna3 C1 speed=fast\n"), "speed");
}

#[test]
fn the_exit_of_spur2_sends_a_carrier_out_or_round_again() {
    let head = "%% geometry spur2\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n";
    let lap = "%% pass antenna3 C1\n%% on antenna5 C1\n%% pass antenna5 C1\n%% on antenna7 C1\n";
    let body = format!("%% move C1 to antenna2\n%% pass antenna2 C1\n%% on antenna3 C1\n{lap}%% divert antenna7 C1\n%% on antenna3 C1\n%% caption a | b\n{lap}%% pass antenna7 C1\n");
    let sc = Scene::from_doc(&doc(&format!("{head}{body}")).unwrap()).unwrap();
    let route = sc.carriers[0].route.join(",");
    // divert: the return belt, back on the spur between antenna2 and antenna3; pass: to the Main-Lane.
    assert!(route.contains("ex,rcj,w1,d1"), "{route}");
    assert_eq!(route.matches("spur_rd").count(), 1, "{route}");
    assert!(route.ends_with("ex,po,merge,iface_out,main_out,gone"), "{route}");
    let tl = Timeline::build(&sc).unwrap();
    // The second `on antenna3 C1` waits for the second arrival there, after the lap.
    let cap = tl.events.iter().find(|e| matches!(e.ev, Ev::Caption { .. })).unwrap().t0;
    let arrivals: Vec<f64> = tl
        .log
        .iter()
        .filter(|l| l.contains("C1 -> d1"))
        .map(|l| l.split_whitespace().nth(2).unwrap().trim_end_matches('s').parse().unwrap())
        .collect();
    assert_eq!(arrivals.len(), 2);
    assert!((cap - arrivals[1]).abs() < 0.01, "caption at {cap}, arrivals {arrivals:?}");
}

#[test]
fn pass_goes_on_and_divert_turns_into_the_lane() {
    // At Divert 1 (antenna3) C1 turns into needle 1, C2 goes on to Divert 2.
    let head = "%% geometry spur2\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n%% carrier id=C2 sid=SID-2\n";
    let d = doc(&format!("{head}%% divert antenna3 C1\n")).unwrap();
    assert!(has(&d, "step", &[("action", "pass"), ("device", "antenna3"), ("carrier", "C1"), ("divert", "true")]));
    let run = |body: &str| Scene::from_doc(&doc(&format!("{head}{body}")).unwrap()).and_then(|sc| Timeline::build(&sc).map(|_| sc));
    let sc = run("%% move C1 to antenna3\n%% divert antenna3 C1\n%% move C2 to antenna3\n%% pass antenna3 C2\n%% on antenna5 C2\n%% divert antenna5 C2\n").unwrap();
    let tail = |ci: usize, n: usize| sc.carriers[ci].route[sc.carriers[ci].route.len() - n..].join(",");
    assert_eq!(tail(0, 5), "d1,q2,pc1,q1,s1");
    assert_eq!(tail(1, 8), "d1,rj1,w2,d2,qb2,pc2,qb1,s2");
    let err = |body: &str, what: &str| {
        let e = run(body).err().unwrap_or_else(|| panic!("expected an error with '{what}'"));
        assert!(e.contains(what), "'{e}' does not contain '{what}'");
    };
    // A ghost too: it starts on Divert 1 and turns into needle 1.
    let g = Scene::from_doc(&doc(&format!("{head}%% carrier id=C2L sid=SID-2 start=antenna3 ghost\n%% divert antenna3 C2L\n")).unwrap()).unwrap();
    Timeline::build(&g).unwrap();
    assert_eq!(g.carriers[2].route.join(","), "d1,q2,pc1,q1,s1");
    // Antenna 2 can't divert.
    err("%% move C1 to antenna2\n%% divert antenna2 C1\n", "divert antenna2 C1: no lane turns off there: pass antenna2 C1");
    assert!(run("%% move C1 to antenna2\n%% pass antenna2 C1\n").is_ok());
    // The carrier written must be the one stopped there.
    err("%% move C1 to antenna3\n%% move C2 to antenna3\n%% pass antenna3 C2\n", "the carrier stopped there at t=");
    fails(&format!("{head}%% divert antenna3\n"), "'divert' is written: divert <antenna> <carrier>");
    fails(&format!("{head}%% pass antenna3 C9\n"), "no carrier 'C9'");
}

#[test]
fn a_ghost_stops_behind_the_carrier_in_front_but_is_driven_through() {
    let log = |carriers: &str, body: &str| {
        let d = doc(&format!("%% geometry spur2\nsequenceDiagram\n{carriers}{body}")).unwrap();
        build(&d).1.log.join("\n")
    };
    // C1 waits at Divert 1; the ghost of C2 goes there too and stops behind it.
    let l = log("%% carrier id=C1 sid=SID-1\n%% carrier id=C2 sid=SID-2\n%% carrier id=C2L sid=SID-2 start=antenna1 ghost\n", "%% move C1 to antenna3\n%% move C2L to antenna3\n");
    assert!(l.contains("C2L -> d1  (stops behind C1"), "{l}");
    // A ghost standing on antenna 2: C1 drives through it.
    let l = log("%% carrier id=C1 sid=SID-1\n%% carrier id=C2L sid=SID-2 start=antenna2 ghost\n", "%% move C1 to antenna3\n");
    assert!(l.contains("move      C1 -> d1\n") || l.ends_with("move      C1 -> d1"), "{l}");
}

const ON_HEAD: &str = "%% geometry spur2\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n";

#[test]
fn on_after_and_color_errors() {
    let k = |body: &str| format!("{ON_HEAD}{body}");
    fails(&k("%% on antena1\nIF->>AN: X SampleId=SID-1\n"), "not an antenna of this scene (antennas: antenna1, antenna2");
    fails(&k("%% on antenna1 C9\n%% caption a | b\n"), "no carrier 'C9'");
    fails(&k("%% on antenna1\n%% caption a | b\n"), "write the carrier, on antenna1 <carrier>");
    fails(&k("%% on antenna1\n%% at 3\nIF->>AN: X SampleId=SID-1\n"), "only one of");
    fails(&k("%% after 1\n%% on antenna1\nIF->>AN: X SampleId=SID-1\n"), "only one of");
    fails(&k("%% after 1 antenna1\nIF->>AN: X SampleId=SID-1\n"), "needs the carrier");
    fails(&k("%% after 1 antenna1 C9\nIF->>AN: X SampleId=SID-1\n"), "no carrier 'C9'");
    fails(&k("%% on antenna1\n"), "its step is missing");
    fails(&k("%% after 1 nothing\nIF->>AN: X\n"), "no line with id 'nothing' before this one");
    fails(&k("%% after 1 Add Tube C1\nIF->>AN: X\n"), "write the id of a line");
    fails(&k("IF->>AN: X\n%% after 1 later\n%% id later\nIF->>AN: Y\n"), "no line with id 'later'");
    fails(&k("%% id a b\nIF->>AN: X\n"), "'id' is written: id <name>");
    fails(&k("%% id a.b\nIF->>AN: X\n"), "only letters, digits");
    fails(&k("%% id antenna1\nIF->>AN: X\n"), "already the name of an antenna");
    fails(&k("%% id a\nIF->>AN: X\n%% id a\nIF->>AN: Y\n"), "the id 'a' is already used");
    fails(&k("%% id a\n%% id b\nIF->>AN: X\n"), "a line has one id");
    fails(&k("%% id a\n"), "'id a' at the end of the diagram");
    fails(&k("%% color blue\nIF->>AN: X\n"), "like #9333EA");
    fails(&k("%% color #9333EA\n%% caption a | b\n"), "applies to the next message");
    fails(&k("%% color #9333EA\n"), "its message is missing");
    fails(&k("%% Prepare 1\nIF->>AN: X SampleId=SID-1\n"), "unknown directive 'Prepare'");
    fails(&k("%% onn antenna1\nIF->>AN: X SampleId=SID-1\n"), "did you mean 'on'");
}

#[test]
fn a_note_after_a_pipette_is_its_text() {
    let d = doc(&format!("{HEAD}%% pipette antenna2\nNote over AN: Aspirating SID-1\nNote over AN: Other\n")).unwrap();
    assert_eq!(d.arrays["step"].iter().filter(|t| t.str_req("action").unwrap() == "note").count(), 1);
    assert!(has(&d, "step", &[("action", "pipette"), ("note", "Aspirating SID-1")]));
    assert!(has(&doc(&format!("{HEAD}%% pass antenna2 C1\n")).unwrap(), "step", &[("action", "pass"), ("device", "antenna2"), ("carrier", "C1")]));
    // Only antennas, not the names of the devices.
    fails(&format!("{HEAD}%% pass sp C1\n"), "'pass sp': write the antenna, pass antenna2");
    // pipette and needle: the antenna of the sampling point.
    assert!(has(&doc(&format!("{HEAD}%% pipette antenna2\n%% needle antenna2 down\n")).unwrap(), "step", &[("action", "needle"), ("device", "sp")]));
    fails(&format!("{HEAD}%% pipette sp\n"), "'pipette sp': write the antenna, pipette antenna2");
    fails(&format!("{HEAD}%% needle antenna1 down\n"), "'needle antenna1': antenna1 is not on a sampling point");
    fails(&format!("{HEAD}%% after 1 pipette sp\nIF->>AN: X\n"), "'pipette sp': write the antenna, pipette antenna2");
    // The turnstile: its antenna is under the needle.
    let ts = "%% geometry turnstile\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n";
    assert!(has(&doc(&format!("{ts}%% pipette antenna3\n")).unwrap(), "step", &[("action", "pipette"), ("device", "ts")]));
    fails(&format!("{ts}%% pipette ts\n"), "'pipette ts': write the antenna, pipette antenna3");
    fails(&format!("{ts}%% on antenna3 C1\n%% caption a | b\n"), "the wheel brings the carrier under the needle; start from the rotation: after <seconds> rotate ts");
    // A move to an antenna goes to its point.
    assert!(has(&doc(&format!("{HEAD}%% move C1 to antenna2\n")).unwrap(), "step", &[("action", "move"), ("to", "sampling")]));
    fails(&format!("{HEAD}%% move C1 to antenn2\n"), "'antenn2' is neither an antenna nor a point of this scene (antennas: antenna1, antenna2");
    fails(&format!("{HEAD}%% divert an C1\n"), "'an' is not an antenna of this scene");
    let d = doc(&format!("{HEAD}%% pipette antenna2 duration=4 sid=SID-1\n%% pipette antenna2 sid=SID-2\n")).unwrap();
    assert!(has(&d, "step", &[("action", "pipette"), ("sample_id", "SID-1"), ("duration", "4")]) && has(&d, "step", &[("sample_id", "SID-2")]));
    fails(&format!("{HEAD}%% rotate sp sid=SID-1\n"), "only pipette does");
    fails("%% geometry spur\nsequenceDiagram\n%% carrier C1 SID-1\n", "carrier id=<id> [sid=<sample id>]");
    // Without sid= the carrier is empty.
    let d = doc("%% geometry spur\nsequenceDiagram\n%% carrier id=E\n%% move E to nsd\n").unwrap();
    assert!(has(&d, "carrier", &[("id", "E"), ("tube", "false")]) && !d.arrays["carrier"][0].entries.contains_key("sample_id"));
}

#[test]
fn the_way_of_a_carrier_comes_from_its_steps() {
    let way = |body: &str| {
        let d = doc(&format!("%% geometry sampling\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n%% carrier id=C2 sid=SID-2 start=antenna1 ghost\n{body}%% caption a | b\n")).unwrap();
        build(&d).0.carriers.iter().map(|c| c.route.join(",")).collect::<Vec<_>>()
    };
    // A move: the shortest way along the lanes; pass: on to the next stop or the end.
    assert_eq!(way("%% move C1 to antenna2\n%% pass antenna2 C1\n")[0], "main_in,nsd,pit_in,sampling,pit_out,merge,main_out,gone");
    assert_eq!(way("%% move C1 to main_out\n")[0], "main_in,nsd,merge,main_out");
    // start=: where it is at the beginning (an antenna stands for its point).
    assert_eq!(way("")[1], "nsd");
    let e = |body: &str| Scene::from_doc(&doc(&format!("%% geometry sampling\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n{body}")).unwrap()).err().unwrap();
    let msg = e("%% pass antenna2 C1\n");
    assert!(msg.contains("pass antenna2 C1: 'C1' is not on antenna2: it is still where it starts, on 'main_in'; send it there first (move C1 to antenna2"), "{msg}");
    let msg = e("%% move C1 to main_out\n%% move C1 to antenna1\n");
    assert!(msg.contains("move C1 to antenna1: 'C1' can't go there along the lanes from 'main_out', where it is"), "{msg}");
    // The first wrong step in the file, with who is on the antenna instead.
    let msg = e("%% carrier id=C2 sid=SID-2\n%% move C1 to antenna2\n%% move C2 to main_out\n%% pass antenna2 C2\n");
    assert!(msg.contains("pass antenna2 C2: 'C2' is not on antenna2: after line 6 (move C2 to 'main_out') it goes to 'main_out'; on antenna2 there is C1: did you mean pass antenna2 C1?"), "{msg}");
}

#[test]
fn the_belt_speed_is_in_the_video_settings() {
    let speed = |extra: &str| {
        let d = doc(&format!("%% geometry sampling\n{extra}sequenceDiagram\n%% carrier id=C1 sid=SID-1\n%% caption a | b\n")).unwrap();
        build(&d).0.carriers[0].speed
    };
    // 11 px/cm on screen.
    assert!((speed("%% @video belt=17.5\n") - 192.5).abs() < 1e-9);
    assert!((speed("%% @video belt=10\n") - 110.0).abs() < 1e-9);
    let d = doc("%% geometry sampling\n%% @video belt=0\nsequenceDiagram\n%% caption a | b\n").unwrap();
    assert!(Scene::from_doc(&d).err().unwrap().contains("belt must be a speed"));
}

#[test]
fn import_errors() {
    fails("graph TD\n", "must start with");
    fails(&format!("{HEAD}%% mvoe C1 to nsd\n"), "unknown directive 'mvoe'");
    fails(&format!("{HEAD}%% move C1 nsd\n"), "'move' is written");
    fails(&format!("{HEAD}alt ok\nIF->>AN: A\nend\n"), "'alt' is not supported");
    fails(&format!("{HEAD}IF->>AN: Prepare SampleId=SID-9\n"), "no carrier has sample id 'SID-9'");
    fails(&format!("{HEAD}%% carrier id=C2 sid=SID-2 route=main_in,pit,gone\n"), "a carrier has no route= or needle=");
    fails(&format!("{HEAD}%% on antenna1\n%% at 3\nIF->>AN: A SampleId=SID-1\n"), "only one of");
    fails(&format!("{HEAD}%% at soon\nIF->>AN: A\n"), "needs seconds");
    fails(&format!("{HEAD}%% when C1 at nsd\nIF->>AN: A\n"), "'when' no longer exists");
    fails("%% geometry nowhere\n", "nowhere.mmd not found");
    fails("%% @point id=a id=b\n", "duplicate key 'id'");
}

#[test]
fn a_plain_comment_and_mermaid_init_are_skipped() {
    assert!(doc(&format!("{HEAD}%% # just a note\n%%{{init: {{}}}}%%\nIF->>AN: Hello\n")).is_ok());
}

#[test]
fn at_lines_become_tables_with_their_place() {
    let d = parse_text("%% @video title=t\n%% @point id=a x=1 y=2\n%% @video tail=3\n").unwrap();
    assert!(has(&d, "point", &[("id", "a"), ("x", "1")]));
    assert_eq!(d.arrays["point"][0].line, 2);
    assert_eq!(d.tables["video"].num_req("tail").unwrap(), 3.0);
    assert_eq!(d.tables["video"].str_req("subtitle").unwrap(), "test", "the name is the default subtitle");
}

fn temp(tag: &str, list: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("trackviz_flow_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (name, text) in list {
        let p = dir.join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    dir
}

#[test]
fn includes_replace_the_line_with_their_directives() {
    let dir = temp(
        "inc",
        &[
            ("a.mmd", "%% include common/b.mmd\nsequenceDiagram\nIF->>AN: Prepare SampleId=SID-1\n"),
            ("common/b.mmd", "%% @point id=p x=1 y=1\n%% include c.mmd\n\n%% caption T | x\n"),
            ("common/c.mmd", "%% carrier id=C1 sid=SID-1 start=p\n"),
            ("common/messages.mmd", "%% @message id=prepare from=IF to=AN text=Prepare params=\"SampleId={SampleId}\"\n"),
        ],
    );
    let d = load(&dir.join("a.mmd"), &dir).unwrap();
    assert!(has(&d, "carrier", &[("id", "C1")]) && has(&d, "step", &[("type", "prepare")]) && has(&d, "step", &[("title", "T")]));
    assert_eq!(d.files.len(), 4, "{:?}", d.files);
    // Every table knows the file and line it comes from.
    let c1 = &d.arrays["carrier"][0];
    assert!(c1.file.ends_with("c.mmd") && c1.line == 1, "{} {}", c1.file, c1.line);
    let prepare = d.arrays["step"].iter().find(|t| t.entries.contains_key("type")).unwrap();
    assert!(prepare.file.ends_with("a.mmd") && prepare.line == 3);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_scene_file_has_no_step_tables() {
    let dir = temp("step", &[("a.mmd", "sequenceDiagram\n%% @step action=wait duration=1\n")]);
    let e = load(&dir.join("a.mmd"), &dir).err().unwrap();
    assert!(e.contains("'@step' is not written in a scene") && e.contains("line 2"), "{e}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn include_errors() {
    let dir = temp(
        "err",
        &[
            ("msg.mmd", "%% include m.mmd\n"),
            ("m.mmd", "IF->>AN: Hello\n"),
            ("loop.mmd", "%% include loop.mmd\n"),
            ("no.mmd", "%% include missing.mmd\n"),
        ],
    );
    let e = |f: &str| read_lines(&dir.join(f), &mut Vec::new()).err().unwrap();
    assert!(e("msg.mmd").contains("can only hold %% directives"), "{}", e("msg.mmd"));
    assert!(e("msg.mmd").contains("line 1"));
    assert!(e("loop.mmd").contains("circular include"));
    assert!(e("no.mmd").contains("missing.mmd"));
    std::fs::remove_dir_all(dir).unwrap();
}
