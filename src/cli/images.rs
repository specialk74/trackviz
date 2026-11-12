//! The images next to a video: the layout (points and devices) and the contact
//! sheet (one frame per caption).

use super::*;
use crate::{render, scene};
use resvg::tiny_skia;

/// The layout image next to a video: out/a.mp4 -> out/a_layout.png.
pub(super) fn layout_path(video: &str) -> String {
    let stem = video.strip_suffix(".mp4").unwrap_or(video);
    format!("{stem}_layout.png")
}

/// Writes the overview of the points and devices of a scene as a PNG.
pub(super) fn write_layout(sc: &scene::Scene, tl: &scene::Timeline, path: &str) -> Result<(), String> {
    let svg = render::layout(sc, tl);
    let png = rasterize(&raster_options(), &svg, sc.video.width, sc.video.height)?.encode_png().map_err(|e| e.to_string())?;
    std::fs::write(path, png).map_err(|e| format!("{path}: {e}"))?;
    eprintln!("wrote {path}");
    Ok(())
}

/// The contact sheet next to a video: out/a.mp4 -> out/a_sheet.png.
pub(super) fn sheet_path(video: &str) -> String {
    let stem = video.strip_suffix(".mp4").unwrap_or(video);
    format!("{stem}_sheet.png")
}

/// The instants of the contact sheet: for every caption, the last moment before
/// the next one appears (the end of the scene for the last one), when what it
/// describes has happened; never before the caption has faded in.
pub(super) fn sheet_times(tl: &scene::Timeline) -> Vec<f64> {
    let mut starts: Vec<f64> = tl.events.iter().filter(|e| matches!(e.ev, scene::Ev::Caption { .. })).map(|e| e.t0).collect();
    starts.sort_by(f64::total_cmp);
    starts.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    (0..starts.len())
        .map(|i| {
            let end = starts.get(i + 1).map_or(tl.duration, |&n| n - 0.1);
            end.max(starts[i] + 0.5).min(tl.duration)
        })
        .collect()
}

/// Writes the contact sheet of a scene: one frame per caption, two per row, at
/// half size.
pub(super) fn write_sheet(sc: &scene::Scene, tl: &scene::Timeline, path: &str, jobs: usize) -> Result<(), String> {
    const COLS: u32 = 2;
    const GAP: u32 = 16;
    let times = sheet_times(tl);
    if times.is_empty() {
        return Err(format!("{path}: the scene has no captions, so there is no contact sheet"));
    }
    let (w, h) = ((sc.video.width / 2).max(2), (sc.video.height / 2).max(2));
    let opt = raster_options();
    // The frames, a few at a time.
    let mut tiles: Vec<Option<Result<tiny_skia::Pixmap, String>>> = (0..times.len()).map(|_| None).collect();
    for chunk in tiles.chunks_mut(jobs.max(1)).enumerate() {
        let (ci, slots) = chunk;
        std::thread::scope(|scope| {
            for (k, slot) in slots.iter_mut().enumerate() {
                let t = times[ci * jobs.max(1) + k];
                let opt = &opt;
                scope.spawn(move || *slot = Some(rasterize(opt, &render::frame(sc, tl, t), w, h)));
            }
        });
    }
    let rows = (times.len() as u32).div_ceil(COLS);
    let (sw, sh) = (COLS * w + (COLS + 1) * GAP, rows * h + (rows + 1) * GAP);
    let mut sheet = tiny_skia::Pixmap::new(sw, sh).ok_or_else(|| format!("invalid sheet size {sw}x{sh}"))?;
    sheet.fill(tiny_skia::Color::from_rgba8(0xCB, 0xD5, 0xE1, 0xFF));
    for (i, tile) in tiles.into_iter().enumerate() {
        let tile = tile.expect("every tile is rendered")?;
        let (c, r) = (i as u32 % COLS, i as u32 / COLS);
        let (x, y) = (GAP + c * (w + GAP), GAP + r * (h + GAP));
        sheet.draw_pixmap(x as i32, y as i32, tile.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
    }
    let png = sheet.encode_png().map_err(|e| e.to_string())?;
    std::fs::write(path, png).map_err(|e| format!("{path}: {e}"))?;
    eprintln!("wrote {path} ({} frames)", times.len());
    Ok(())
}
