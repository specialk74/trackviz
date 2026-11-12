//! SVG frames to pixels, with the fonts embedded in the executable.

use resvg::{tiny_skia, usvg};

/// Inter (variable font: font-weight maps to the wght axis), fetched by build.rs.
pub(super) const INTER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/Inter.ttf"));

/// Rendering options shared by all frames. Fonts are loaded once (this is the
/// slow part), then the options are reused for every frame.
pub(super) fn raster_options() -> usvg::Options<'static> {
    let mut opt = usvg::Options::default();
    let db = opt.fontdb_mut();
    // Both fonts are embedded in the executable, so every machine renders the
    // same frames regardless of what is installed.
    db.load_font_data(INTER.to_vec());
    db.load_font_data(dejavu::sans_mono::regular().to_vec());
    db.set_sans_serif_family("Inter");
    db.set_monospace_family("DejaVu Sans Mono");
    // System fonts only as a fallback for glyphs the bundled fonts lack.
    db.load_system_fonts();
    opt
}

/// Rasterizes one SVG frame into a w x h pixmap (premultiplied RGBA).
pub(super) fn rasterize(opt: &usvg::Options, svg: &str, w: u32, h: u32) -> Result<tiny_skia::Pixmap, String> {
    let tree = usvg::Tree::from_str(svg, opt).map_err(|e| format!("invalid frame SVG: {e}"))?;
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or_else(|| format!("invalid frame size {w}x{h}"))?;
    let size = tree.size();
    let tr = tiny_skia::Transform::from_scale(w as f32 / size.width(), h as f32 / size.height());
    resvg::render(&tree, tr, &mut pixmap.as_mut());
    Ok(pixmap)
}
