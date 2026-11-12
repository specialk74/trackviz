use super::*;
use super::caption::wrap;
use crate::scene::tests::{ok, SAMPLING};

#[test]
fn the_layout_names_points_and_devices_without_carriers() {
    let base = format!("{SAMPLING}%% @device id=trigger kind=antenna at=c visible=false\n");
    let (sc, tl) = ok(&base, "\n%% @step action=caption title=t text=x ");
    let svg = layout(&sc, &tl);
    for name in [">sp<", ">nsd<", ">trigger<", "antenna (hidden)", ">a<", ">c<"] {
        assert!(svg.contains(name), "{name}");
    }
    assert!(!svg.contains("SID-1"), "no carriers in the layout");
}

fn frame_at(steps: &str, t: f64) -> String {
    let (sc, tl) = ok(SAMPLING, steps);
    frame(&sc, &tl, t)
}

#[test]
fn analyzer_status_badges() {
    let steps = "\n%% @step action=status at=1 device=an state=error \
                 \n%% @step action=status at=5 device=an state=ready ";
    assert!(!frame_at(steps, 0.5).contains(">ERROR<"));
    assert!(frame_at(steps, 2.0).contains(">ERROR<"));
    let f = frame_at(steps, 6.0);
    assert!(f.contains(">READY<") && !f.contains(">ERROR<"));
}

#[test]
fn needle_label_depends_on_the_device_state() {
    let steps = "\n%% @step action=needle at=0 device=sp to=down \
                 \n%% @step action=status at=3 device=an state=error ";
    assert!(frame_at(steps, 2.0).contains("Aspirating…"));
    let f = frame_at(steps, 4.0);
    assert!(f.contains("Needle down") && !f.contains("Aspirating…"));
}

#[test]
fn ghost_is_drawn_only_while_visible_with_a_custom_tag() {
    let base_tag = SAMPLING.replace("ghost=true ", "ghost=true tag=\"SID-2 (expected)\" ");
    let steps = "\n%% @step action=show at=1 carrier=G ";
    let (sc, tl) = ok(&base_tag, steps);
    assert!(!frame(&sc, &tl, 0.5).contains("(expected)"));
    assert!(frame(&sc, &tl, 2.0).contains("(expected)"));
}

#[test]
fn operator_is_drawn_only_during_its_step() {
    let steps = "\n%% @step action=operator at=1 device=an ";
    assert!(!frame_at(steps, 0.5).contains(">Operator<"));
    assert!(frame_at(steps, 4.0).contains(">Operator<"));
    assert!(!frame_at(steps, 9.0).contains(">Operator<"));
}

#[test]
fn the_last_started_caption_is_shown() {
    let steps = "\n%% @step action=caption at=0 title=first text=x \
                 \n%% @step action=caption at=2 title=second text=y ";
    assert!(frame_at(steps, 1.0).contains("FIRST"));
    let f = frame_at(steps, 3.0);
    assert!(f.contains("SECOND") && !f.contains("FIRST"));
}

#[test]
fn runaway_tag_is_shown_during_the_move() {
    let steps = "\n%% @step action=move at=1 carrier=C1 to=s runaway=true ";
    assert!(!frame_at(steps, 0.5).contains("RUN-AWAY"));
    assert!(frame_at(steps, 1.5).contains("SID-1 · RUN-AWAY"));
}

#[test]
fn side_probe_puts_the_label_inside_the_analyzer() {
    // Analyzer on the right of the sampling point (as in a spur): x 650..850, center y 700.
    let base = SAMPLING.replace("x=100 y=150 w=400 h=150\n", "x=650 y=600 w=200 h=200\n");
    let (sc, tl) = ok(&base, "\n%% @step action=pipette device=sp duration=5 ");
    let f = frame(&sc, &tl, 2.5);
    // label centered in the analyzer (x 750), probe leaving its left side (x 650)
    assert!(f.contains(r##"<text x="750" y="764" font-size="17" font-weight="600" text-anchor="middle" fill="#1D4ED8">Aspirating…</text>"##), "{f}");
    assert!(f.contains(r#"<line x1="650" y1="700""#));
}

#[test]
fn device_label_position_can_be_given() {
    let base = SAMPLING.replace("id=sp kind=sampling at=s\n", "id=sp kind=sampling at=s label=\"Sampling point\" label_x=123 label_y=456\n");
    let (sc, tl) = ok(&base, "\n%% @step action=wait duration=1 ");
    assert!(frame(&sc, &tl, 0.5).contains(r#"<text x="123" y="456""#));
}

#[test]
fn a_diversion_turns_blue_while_a_carrier_is_stopped_on_it() {
    let base = SAMPLING.replace("id=nsd kind=nsd at=b\n", "id=nsd kind=divert at=b label=D\n");
    let (sc, tl) = ok(&base, "\n%% @step action=move carrier=C1 to=b duration=2 \n%% @step action=move at=5 carrier=C1 to=s ");
    let read = r##"fill="#2563EB">D</text>"##;
    assert!(!frame(&sc, &tl, 1.0).contains(read));
    assert!(frame(&sc, &tl, 3.0).contains(read));
    assert!(!frame(&sc, &tl, 6.0).contains(read));
}

#[test]
fn with_two_needles_the_status_of_one_leaves_the_other_working() {
    let base = SAMPLING.replace("%% @device id=sp kind=sampling at=s\n", "%% @device id=sp2 kind=sampling at=c\n%% @device id=sp kind=sampling at=s\n");
    let steps = "\n%% @step action=needle at=0 device=sp to=down \n%% @step action=needle at=0 device=sp2 to=down \n%% @step action=status at=2 device=sp state=error ";
    let (sc, tl) = ok(&base, steps);
    let f = frame(&sc, &tl, 3.0);
    assert!(f.contains(">ERROR<") && f.contains("Needle down") && f.contains("Aspirating…"), "{f}");
    assert!(f.contains(">Needle 1<") && f.contains(">Needle 2<"));
}

#[test]
fn wrap_splits_on_words() {
    assert_eq!(wrap("aa bb cc", 5), vec!["aa bb", "cc"]);
    assert_eq!(wrap("a\nb", 10), vec!["a", "b"]);
}
