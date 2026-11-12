//! trackviz — renders scenario files (track + message sequence) to MP4.
//!
//! Usage:
//!   trackviz <scene.mmd>... [-o out.mp4] [-j jobs]   (default: out/<scene name>.mp4)
//!   trackviz <scene.mmd> --still <seconds> [-o frame.png]
//!   trackviz <scene.mmd>... --layout      (only out/<scene name>_layout.png: points and devices)
//!   trackviz <scene.mmd>... --sheet       (out/<scene name>_sheet.png: one frame per caption)
//!   trackviz new <scene> [--geometry spur2] [--carriers 3]   (the skeleton of a new scene)
//!   trackviz index [<scene.mmd>...] [-o out/index.html]   (the page that lists the videos)
//!   trackviz sort <scene.mmd>... [--dry-run]   (the steps in time order, same video)
//!   trackviz <scene.mmd>... --check         (validate + print the timeline)
//!   trackviz <scene.mmd> --preview --from 15 --to 25   (quick look, out/<scene name>_preview.mp4)
//!   trackviz <scene.mmd> --preview --watch  (regenerate when the scene or a file it uses changes)
//!
//! A scene is a Mermaid sequence diagram with `%%` directives for the physical side.
//! Scene arguments may contain the wildcards `*` and `?` in the file name: several
//! scenes are rendered in parallel, sharing the `-j` threads.
//!
//! Frames are rasterized in-process with resvg; only `ffmpeg` is needed on PATH
//! (and only for video output).

mod cli;
mod doc;
mod index;
mod lint;
mod mermaid;
mod physics;
mod render;
mod scene;
mod sort;
mod template;

fn main() {
    if let Err(e) = cli::run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
