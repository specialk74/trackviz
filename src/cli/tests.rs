use super::*;
use crate::{lint, scene, sort};
use resvg::tiny_skia;

fn m(p: &str, n: &str) -> bool {
    wildcard_match(&p.chars().collect::<Vec<_>>(), &n.chars().collect::<Vec<_>>())
}

#[test]
fn the_contact_sheet_shows_every_caption_at_its_end() {
    let (_, tl) = load(concat!(env!("CARGO_MANIFEST_DIR"), "/scenes/02_turnstile_three_carriers.mmd")).unwrap();
    let mut caps: Vec<f64> = tl.events.iter().filter(|e| matches!(e.ev, scene::Ev::Caption { .. })).map(|e| e.t0).collect();
    caps.sort_by(f64::total_cmp);
    let times = sheet_times(&tl);
    assert_eq!(times.len(), caps.len());
    for (i, &t) in times.iter().enumerate() {
        // The frame shows caption i: after it has faded in, before the next one.
        assert!(t >= caps[i] + 0.4 && caps.get(i + 1).is_none_or(|&n| t < n), "caption {i}: {t}");
    }
    assert!((times.last().unwrap() - tl.duration).abs() < 1e-9);
}

#[test]
fn a_scene_that_breaks_has_a_video_up_to_where_it_breaks() {
    let dir = std::env::temp_dir().join(format!("trackviz_err_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("common")).unwrap();
    for f in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/scenes/common")).unwrap() {
        let f = f.unwrap().path();
        std::fs::copy(&f, dir.join("common").join(f.file_name().unwrap())).unwrap();
    }
    // C1 stays on the exit: C2 never gets there.
    let p = dir.join("broken.mmd");
    std::fs::write(&p, "%% geometry spur2\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n%% carrier id=C2 sid=SID-2\n\
        %% caption 1 · Start | b\n%% move C1 to antenna7\n%% at 1\n%% move C2 to antenna7\n%% on antenna7 C2\nIF->>AN: Release\n").unwrap();
    let path = p.to_string_lossy().to_string();
    let e = load(&path).err().unwrap();
    let (_, tl, line) = until_error(&path, &e).unwrap_or_else(|| panic!("{e}"));
    assert_eq!(line, 10);
    // The moves are in it, the step that breaks is not; a red caption says why.
    assert!(tl.log.iter().any(|l| l.contains("C2 -> ex  (stops behind C1")), "{:?}", tl.log);
    assert!(!tl.log.iter().any(|l| l.contains("Release")));
    let last = tl.events.iter().rfind(|e| matches!(e.ev, scene::Ev::Caption { .. })).unwrap();
    let scene::Ev::Caption { title, text } = &last.ev else { unreachable!() };
    assert_eq!(title, "Error · line 10");
    assert!(text.starts_with("'C2' never gets to antenna7"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn sort_puts_the_steps_in_time_order_and_keeps_the_video() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/scenes");
    let src = format!("{dir}/02_turnstile_three_carriers.mmd");
    let copy = format!("{dir}/.test_sort_02.mmd");
    std::fs::copy(&src, &copy).unwrap();
    let r = sort_one(&copy, false);
    let after = load(&copy);
    let text = std::fs::read_to_string(&copy).unwrap();
    let (sc2, tl2) = after.unwrap();
    let again = sort::sorted(&copy, &text, &sc2, &tl2);
    let _ = std::fs::remove_file(&copy);
    r.unwrap();
    let (_, tl) = load(&src).unwrap();
    sort::same_timeline(&tl, &tl2).unwrap();
    // The messages are now written in time order, and a second sort changes nothing.
    assert!(lint::warnings(&sc2, &tl2).iter().all(|w| !w.contains("wrong order")), "{:?}", lint::warnings(&sc2, &tl2));
    assert!(again.unwrap().is_none());
}

#[test]
fn a_contact_sheet_is_a_grid_of_frames() {
    let (sc, tl) = load(concat!(env!("CARGO_MANIFEST_DIR"), "/scenes/01_prepare_sample_release.mmd")).unwrap();
    let path = std::env::temp_dir().join("trackviz_sheet_test.png");
    write_sheet(&sc, &tl, path.to_str().unwrap(), 4).unwrap();
    let img = tiny_skia::Pixmap::load_png(&path).unwrap();
    let rows = (sheet_times(&tl).len() as u32).div_ceil(2);
    assert_eq!((img.width(), img.height()), (2 * 960 + 3 * 16, rows * 540 + (rows + 1) * 16));
    let _ = std::fs::remove_file(path);
}

#[test]
fn wildcards_in_names() {
    assert!(m("*.mmd", "01_a.mmd"));
    assert!(m("0?_a.mmd", "01_a.mmd"));
    assert!(m("*spur*", "09_spur_happy_path.mmd"));
    assert!(m("*", "x"));
    assert!(!m("*.mmd", "a.mmd.bak"));
    assert!(!m("0?_a.mmd", "012_a.mmd"));
    assert!(!m("?", ""));
}

#[test]
fn expand_lists_matching_files_sorted() {
    let dir = std::env::temp_dir().join(format!("trackviz_expand_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("sub.mmd")).unwrap();
    for f in ["b.mmd", "a.mmd", "c.txt"] {
        std::fs::write(dir.join(f), "").unwrap();
    }
    let d = dir.to_string_lossy();
    // Directories are skipped, the result is sorted.
    assert_eq!(expand(&format!("{d}/*.mmd")).unwrap(), vec![format!("{d}/a.mmd"), format!("{d}/b.mmd")]);
    assert!(expand(&format!("{d}/*.zip")).unwrap_err().contains("no matching files"));
    assert!(expand("sc*nes/a.mmd").unwrap_err().contains("only in the file name"));
    // Without wildcards the argument is left to the loader.
    assert_eq!(expand("missing.mmd").unwrap(), vec!["missing.mmd"]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn threads_are_shared_among_the_scenes() {
    assert_eq!(share_jobs(10, 3), (3, 3));
    assert_eq!(share_jobs(10, 16), (10, 1));
    assert_eq!(share_jobs(1, 4), (1, 1));
}

fn video() -> scene::Video {
    scene::Video { title: String::new(), subtitle: String::new(), width: 1920, height: 1080, fps: 30, tail: 0.0, show_clock: true, show_times: true, show_line: true, belt: scene::BELT_SPEED, message: 1.5 }
}

#[test]
fn a_full_render_covers_the_whole_scene() {
    let p = plan(&video(), 10.0, VideoOpts::default()).unwrap();
    assert_eq!((p.w, p.h, p.fps, p.start, p.frames), (1920, 1080, 30, 0.0, 300));
}

#[test]
fn a_part_of_the_scene_and_the_preview_settings() {
    let part = VideoOpts { from: Some(2.0), to: Some(4.0), preview: false };
    let p = plan(&video(), 10.0, part).unwrap();
    assert_eq!((p.w, p.fps, p.start, p.frames), (1920, 30, 2.0, 60));
    let prev = VideoOpts { preview: true, ..VideoOpts::default() };
    let p = plan(&video(), 10.0, prev).unwrap();
    assert_eq!((p.w, p.h, p.fps, p.frames), (960, 540, 15, 150));
    // Beyond the end: cut at the end of the scene.
    let late = VideoOpts { from: Some(8.0), to: Some(99.0), preview: false };
    assert_eq!(plan(&video(), 10.0, late).unwrap().frames, 60);
    // A slow scene is not sped up, odd sizes become even.
    let odd = scene::Video { width: 1001, height: 501, fps: 10, ..video() };
    let p = plan(&odd, 10.0, prev).unwrap();
    assert_eq!((p.w, p.h, p.fps), (500, 250, 10));
}

#[test]
fn an_empty_range_is_an_error() {
    for (from, to) in [(5.0, 2.0), (12.0, 20.0), (-1.0, 2.0)] {
        let o = VideoOpts { from: Some(from), to: Some(to), preview: false };
        assert!(plan(&video(), 10.0, o).unwrap_err().contains("nothing to render"));
    }
}

#[test]
fn a_partial_render_keeps_the_full_video() {
    assert_eq!(default_out("scenes/a.mmd", VideoOpts::default()), "out/a.mp4");
    let prev = VideoOpts { preview: true, ..VideoOpts::default() };
    assert_eq!(default_out("scenes/a.mmd", prev), "out/a_preview.mp4");
    let part = VideoOpts { to: Some(3.0), ..VideoOpts::default() };
    assert_eq!(default_out("scenes/a.mmd", part), "out/a_preview.mp4");
}

#[test]
fn a_change_is_seen_in_every_watched_file() {
    let dir = std::env::temp_dir().join(format!("trackviz_watch_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (a, b) = (dir.join("a.mmd"), dir.join("b.mmd"));
    std::fs::write(&a, "x").unwrap();
    std::fs::write(&b, "x").unwrap();
    let files = vec![a, b.clone()];
    let before = stamps(&files);
    std::fs::File::options().write(true).open(&b).unwrap().set_modified(SystemTime::now() + Duration::from_secs(5)).unwrap();
    assert_ne!(stamps(&files), before);
    std::fs::remove_file(&b).unwrap();
    assert_eq!(stamps(&files)[1], None, "a file being replaced doesn't stop the watch");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_diagrams_in_scenes_are_valid_and_clean() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/scenes");
    let list: Vec<PathBuf> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|e| e == "mmd")).collect();
    assert!(!list.is_empty());
    for p in list {
        let (sc, tl) = load(&p.to_string_lossy()).unwrap();
        let w = lint::warnings(&sc, &tl);
        assert!(w.is_empty(), "{}: {w:?}", p.display());
    }
}

#[test]
fn an_error_in_a_diagram_names_its_line() {
    let dir = std::env::temp_dir().join(format!("trackviz_flow_line_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("common")).unwrap();
    let common = concat!(env!("CARGO_MANIFEST_DIR"), "/scenes/common");
    for e in std::fs::read_dir(common).unwrap() {
        let p = e.unwrap().path();
        std::fs::copy(&p, dir.join("common").join(p.file_name().unwrap())).unwrap();
    }
    let f = dir.join("a.mmd");
    std::fs::write(&f, "%% geometry spur\nsequenceDiagram\n%% carrier id=C1 sid=SID-1\n\n%% move C9 to nsd\n").unwrap();
    let e = load(&f.to_string_lossy()).err().unwrap();
    assert!(e.contains("line 5") && e.contains("unknown carrier 'C9'"), "{e}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn help_shows_version_and_repository() {
    let u = usage();
    assert!(u.contains(VERSION) && u.contains("https://github.com/specialk74/trackviz"));
}
