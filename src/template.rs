//! `trackviz new`: the skeleton of a new scene, valid as it is, for one geometry.
//! Carrier C1 goes the whole way (Prepare, Sample Acquisition, pipetting, Sample
//! Complete, Release); the other carriers are declared and wait to be used.

/// The geometries in scenes/common, with what a skeleton needs of them.
struct Geo {
    name: &'static str,
    /// Route of the first carrier (it starts on the panel) and of the others.
    kind: Kind,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// A sampling point on the pit-lane or on the spur: antenna1 NSD, antenna2
    /// sampling point, antenna3 end of the pit-lane.
    Sampling,
    /// A turnstile: antenna1 NSD, antenna2 in front of the gate, antenna3 under
    /// the needle, antenna4 end of the pit-lane.
    Turnstile,
    /// Two sampling points: antenna1 NSD, antenna2 on the spur (pass only),
    /// antenna3 Divert 1, antenna4 sampling point 1, antenna5 Divert 2, antenna6
    /// sampling point 2, antenna7 exit (gate: pass to the Main-Lane, divert on the
    /// return belt).
    Spur2,
}

const GEOMETRIES: [Geo; 7] = [
    Geo { name: "sampling", kind: Kind::Sampling },
    Geo { name: "sampling_queue", kind: Kind::Sampling },
    Geo { name: "sampling_queue2", kind: Kind::Sampling },
    Geo { name: "turnstile", kind: Kind::Turnstile },
    Geo { name: "turnstile_queue", kind: Kind::Turnstile },
    Geo { name: "spur", kind: Kind::Sampling },
    Geo { name: "spur2", kind: Kind::Spur2 },
];

pub fn geometry_names() -> Vec<&'static str> {
    GEOMETRIES.iter().map(|g| g.name).collect()
}

/// The text of a new scene.
pub fn scene(name: &str, geometry: &str, carriers: usize) -> Result<String, String> {
    let g = GEOMETRIES
        .iter()
        .find(|g| g.name == geometry)
        .ok_or_else(|| format!("unknown geometry '{geometry}' (one of: {})", geometry_names().join(", ")))?;
    if carriers == 0 {
        return Err("a scene needs at least one carrier".into());
    }
    let mut s = format!(
        "%% # {name}\n\
         %% # Written by `trackviz new`: C1 goes the whole way; change it as you need.\n\
         %% # Check it with: trackviz <this file> --check (or Ctrl+Shift+B in VS Code).\n\
         %% geometry {geometry}\n\
         %% subtitle {name}\n\
         sequenceDiagram\n\
         \x20   participant IF as Interface\n\
         \x20   participant AN as Analyzer\n\
         \x20   participant DMX as DMX\n\n"
    );
    for n in 1..=carriers {
        s.push_str(&format!("    %% carrier id=C{n} sid=SID-{:06}\n", 122 + n));
    }
    s.push_str(
        "\n    %% caption 1 · Start | A carrier holding a sample tube travels along the Main-Lane towards the analyzer interface.\n\
         \x20   %% at 2.5\n",
    );
    s.push_str(match g.kind {
        Kind::Sampling => SAMPLING,
        Kind::Turnstile => TURNSTILE,
        Kind::Spur2 => SPUR2,
    });
    Ok(s)
}

/// C1 through a sampling point (pit-lane or spur).
const SAMPLING: &str = "    %% move C1 to antenna2
    %% on antenna1 C1
    %% caption 2 · Diverted — Prepare — Sample Acquisition | The carrier is diverted without stopping and the interface sends Prepare. The gate stops it on the sampling point: Sample Acquisition.
    %% on antenna1
    IF->>AN: Prepare SampleId=SID-000123
    %% after 0.1
    AN-->>IF: OK SampleId=SID-000123
    %% on antenna2
    IF->>AN: Sample Acquisition SampleId=SID-000123
    %% after 0.1
    %% for C1
    AN-->>IF: OK
    %% after 0.1
    %% caption 3 · Pipetting | The analyzer aspirates the sample while the gate holds the carrier.
    %% pipette antenna2 duration=4.5 sid=SID-000123
    %% after 0.3 pipette antenna2
    %% caption 4 · Sample Complete | The analyzer sends Sample Complete; the interface replies and opens the gate.
    %% after 0.3 pipette antenna2
    AN->>IF: Sample Complete SampleId=SID-000123
    %% after 0.1
    %% for C1
    IF-->>AN: OK
    %% pass antenna2 C1
    %% on antenna3 C1
    %% caption 5 · Release | Before the carrier is back on the Main-Lane the interface sends Release.
    %% on antenna3
    IF->>AN: Release SampleId=SID-000123
    %% after 0.1
    %% for C1
    AN-->>IF: OK
";

/// C1 through a turnstile.
const TURNSTILE: &str = "    %% move C1 to antenna2
    %% on antenna1 C1
    %% caption 2 · Diverted — Prepare — antenna | The carrier is diverted without stopping (Prepare) and read at the antenna in front of the gate: Prepare again. Nothing is sampled, so the gate opens.
    %% on antenna1
    IF->>AN: Prepare SampleId=SID-000123
    %% after 0.1
    AN-->>IF: OK SampleId=SID-000123
    %% after 0.2 antenna2 C1
    %% color #9333EA
    IF->>AN: Prepare SampleId=SID-000123
    %% after 0.1
    %% for C1
    AN-->>IF: OK
    %% after 0.1
    %% pass antenna2 C1
    %% after 0.15
    %% caption 3 · Rotation — Sample Acquisition | The wheel brings the carrier under the needle: Sample Acquisition, then the analyzer aspirates.
    %% rotate ts
    %% after 0.25 rotate ts
    IF->>AN: Sample Acquisition SampleId=SID-000123
    %% after 0.1
    %% for C1
    AN-->>IF: OK
    %% after 0.1
    %% pipette antenna3 duration=4.5 sid=SID-000123
    %% after 0.3 pipette antenna3
    %% caption 4 · Sample Complete | The analyzer sends Sample Complete; the wheel brings the carrier to the exit.
    %% after 0.3 pipette antenna3
    AN->>IF: Sample Complete SampleId=SID-000123
    %% after 0.1
    %% for C1
    IF-->>AN: OK
    %% rotate ts
    %% after 0.1
    %% move C1 to gone
    %% on antenna4 C1
    %% caption 5 · Release | Before the carrier is back on the Main-Lane the interface sends Release.
    %% on antenna4
    IF->>AN: Release SampleId=SID-000123
    %% after 0.1
    %% for C1
    AN-->>IF: OK
";

/// C1 through needle 1 of a spur with two sampling points.
const SPUR2: &str = "    %% move C1 to antenna2
    %% on antenna1 C1
    %% caption 2 · Diverted — Prepare | The carrier is diverted into the spur without stopping and the interface sends Prepare. The antenna on the spur stops it and lets it go on; at Divert 1 it is read: Prepare again, and it is diverted to needle 1.
    %% on antenna1
    IF->>AN: Prepare SampleId=SID-000123
    %% after 0.1
    AN-->>IF: OK SampleId=SID-000123
    %% after 0.2 antenna2 C1
    %% pass antenna2 C1
    %% after 0.1 antenna3 C1
    %% color #9333EA
    IF->>AN: Prepare SampleId=SID-000123
    %% after 0.1
    %% for C1
    AN-->>IF: OK
    %% after 0.1
    %% divert antenna3 C1
    %% on antenna4 C1
    %% caption 3 · Sample Acquisition | The gate stops the carrier on sampling point 1: Sample Acquisition with Needle=1, then needle 1 aspirates.
    %% on antenna4
    IF->>AN: Sample Acquisition SampleId=SID-000123 Needle=1
    %% after 0.1
    %% for C1
    AN-->>IF: OK
    %% after 0.1
    %% pipette antenna4 duration=4 sid=SID-000123
    %% after 0.3 pipette antenna4
    %% caption 4 · Sample Complete | The analyzer sends Sample Complete; the carrier goes on round the spur.
    %% after 0.3 pipette antenna4
    AN->>IF: Sample Complete SampleId=SID-000123 Needle=1
    %% after 0.1
    %% for C1
    IF-->>AN: OK
    %% pass antenna4 C1
    %% after 0.4 antenna5 C1
    %% pass antenna5 C1
    %% on antenna7 C1
    %% caption 5 · Release | Before the carrier is back on the Main-Lane the interface sends Release.
    %% on antenna7
    IF->>AN: Release SampleId=SID-000123
    %% after 0.1
    %% for C1
    AN-->>IF: OK
    %% pass antenna7 C1
";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Scene, Timeline};
    use crate::{lint, mermaid};
    use std::path::Path;

    /// Every skeleton is a valid scene at any belt speed, and without warnings at
    /// the usual ones.
    #[test]
    fn every_skeleton_is_valid_and_clean() {
        let scenes = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/scenes"));
        for g in geometry_names() {
            for belt in ["%% @video belt=5\n", "%% @video belt=10\n", "%% @video belt=17.5\n", "%% @video belt=35\n"] {
                let text = scene("Test", g, 3).unwrap().replacen("sequenceDiagram\n", &format!("{belt}sequenceDiagram\n"), 1);
                let path = std::env::temp_dir().join(format!("trackviz_new_{g}_{}.mmd", belt.len()));
                std::fs::write(&path, text).unwrap();
                let doc = mermaid::load(&path, scenes).unwrap_or_else(|e| panic!("{g} {belt}: {e}"));
                let sc = Scene::from_doc(&doc).unwrap_or_else(|e| panic!("{g} {belt}: {e}"));
                let tl = Timeline::build(&sc).unwrap_or_else(|e| panic!("{g} {belt}: {e}"));
                // The order of the messages in the file matches the video only within
                // a range of speeds: no warnings at the usual ones.
                if belt.contains("=10") || belt.contains("=17.5") {
                    let w = lint::warnings(&sc, &tl);
                    assert!(w.is_empty(), "{g} {belt}: {w:?}");
                }
                let _ = std::fs::remove_file(path);
            }
        }
    }

    #[test]
    fn skeleton_errors() {
        assert!(scene("x", "nowhere", 1).unwrap_err().contains("unknown geometry 'nowhere'"));
        assert!(scene("x", "spur", 0).is_err());
        assert!(scene("x", "spur", 2).unwrap().contains("%% carrier id=C2 sid=SID-000124\n"));
    }
}
