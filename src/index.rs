//! `trackviz index`: one HTML page listing the videos, to hand them over as a
//! package (the page and the MP4 files in the same folder). For each scene: its
//! subtitle, its length, the video with a cover frame, and its captions as
//! chapters that jump to that point of the video. The page needs no other file:
//! styles, script and covers are inside it.

use std::fmt::Write;

/// One video of the page.
pub struct Entry {
    /// The video file, relative to the page (e.g. `02_turnstile_three_carriers.mp4`).
    pub video: String,
    /// Whether the video file is there.
    pub exists: bool,
    pub subtitle: String,
    pub duration: f64,
    /// (start, caption title, caption text).
    pub chapters: Vec<(f64, String, String)>,
    /// The cover frame, as a PNG.
    pub cover: Option<Vec<u8>>,
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// m:ss
fn clock(t: f64) -> String {
    let s = t.max(0.0).round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

pub fn base64(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            out.push(if i <= c.len() { A[(n >> shift & 63) as usize] as char } else { '=' });
        }
    }
    out
}

const STYLE: &str = r#"
:root { --bg:#F1F4F8; --card:#FFFFFF; --ink:#0F172A; --muted:#64748B; --line:#DCE2EA; --accent:#F59E0B; --chip:#F8FAFC; }
* { box-sizing: border-box; }
body { margin:0; background:var(--bg); color:var(--ink); font:16px/1.5 -apple-system, BlinkMacSystemFont, "Segoe UI", Inter, Roboto, sans-serif; }
main { max-width:1100px; margin:0 auto; padding:32px 16px 64px; }
h1 { font-size:28px; margin:0 0 4px; }
.lead { color:var(--muted); margin:0 0 24px; }
.toc { background:var(--card); border:1px solid var(--line); border-radius:14px; padding:8px 0; margin:0 0 32px; list-style:none; }
.toc li a { display:flex; gap:12px; padding:8px 20px; color:inherit; text-decoration:none; }
.toc li a:hover { background:var(--chip); }
.toc .n { color:var(--muted); font-variant-numeric:tabular-nums; min-width:2ch; text-align:right; }
.toc .t { flex:1; }
.toc .d { color:var(--muted); font-variant-numeric:tabular-nums; }
section { background:var(--card); border:1px solid var(--line); border-radius:14px; padding:20px; margin:0 0 24px; scroll-margin-top:16px; }
h2 { font-size:20px; margin:0 0 4px; }
.meta { color:var(--muted); font-size:14px; margin:0 0 14px; }
video, .cover { width:100%; aspect-ratio:16/9; border-radius:10px; background:#0F172A; display:block; }
.missing { color:#B91C1C; font-size:14px; margin:8px 0 0; }
ol.ch { list-style:none; padding:0; margin:14px 0 0; display:grid; grid-template-columns:repeat(auto-fill, minmax(300px, 1fr)); gap:6px; }
ol.ch button { all:unset; box-sizing:border-box; cursor:pointer; display:flex; gap:10px; width:100%; padding:8px 10px; border-radius:8px; background:var(--chip); border:1px solid var(--line); font-size:14px; }
ol.ch button:hover, ol.ch button:focus-visible { border-color:var(--accent); }
ol.ch button[disabled] { cursor:default; }
ol.ch .at { color:var(--muted); font-variant-numeric:tabular-nums; }
footer { color:var(--muted); font-size:13px; text-align:center; }
"#;

const SCRIPT: &str = r#"
document.querySelectorAll('ol.ch button[data-t]').forEach(b => b.addEventListener('click', () => {
  const v = document.getElementById(b.dataset.v);
  if (!v) return;
  v.currentTime = parseFloat(b.dataset.t);
  v.play();
}));
"#;

/// The page.
pub fn page(title: &str, entries: &[Entry]) -> String {
    let total: f64 = entries.iter().map(|e| e.duration).sum();
    let mut o = String::with_capacity(64 * 1024);
    let _ = write!(
        o,
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{}</title>\n<style>{STYLE}</style>\n</head>\n<body>\n<main>\n<h1>{}</h1>\n<p class=\"lead\">{} videos · {} in all</p>\n<ol class=\"toc\">\n",
        esc(title),
        esc(title),
        entries.len(),
        clock(total)
    );
    for (i, e) in entries.iter().enumerate() {
        let _ = writeln!(
            o,
            "<li><a href=\"#v{n}\"><span class=\"n\">{n}</span><span class=\"t\">{}</span><span class=\"d\">{}</span></a></li>",
            esc(&e.subtitle),
            clock(e.duration),
            n = i + 1
        );
    }
    o.push_str("</ol>\n");
    for (i, e) in entries.iter().enumerate() {
        let n = i + 1;
        let cover = e.cover.as_ref().map(|png| format!("data:image/png;base64,{}", base64(png)));
        let _ = write!(o, "<section id=\"v{n}\">\n<h2>{n} · {}</h2>\n<p class=\"meta\">{} · {}</p>\n", esc(&e.subtitle), clock(e.duration), esc(&e.video));
        if e.exists {
            let poster = cover.as_ref().map_or(String::new(), |c| format!(" poster=\"{c}\""));
            let _ = writeln!(o, "<video id=\"video{n}\" controls preload=\"metadata\"{poster} src=\"{}\"></video>", esc(&e.video));
        } else {
            if let Some(c) = &cover {
                let _ = writeln!(o, "<img class=\"cover\" alt=\"\" src=\"{c}\">");
            }
            let _ = writeln!(o, "<p class=\"missing\">The video {} is not in this folder.</p>", esc(&e.video));
        }
        o.push_str("<ol class=\"ch\">\n");
        for (t, title, text) in &e.chapters {
            let target = if e.exists { format!(" data-v=\"video{n}\" data-t=\"{t:.2}\"") } else { " disabled".to_string() };
            let _ = writeln!(
                o,
                "<li><button type=\"button\" title=\"{}\"{target}><span class=\"at\">{}</span><span>{}</span></button></li>",
                esc(text),
                clock(*t),
                esc(title)
            );
        }
        o.push_str("</ol>\n</section>\n");
    }
    let _ = write!(o, "<footer>Click a chapter to play the video from that point.</footer>\n</main>\n<script>{SCRIPT}</script>\n</body>\n</html>\n");
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_as_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn the_page_lists_videos_and_chapters() {
        let e = |video: &str, exists| Entry {
            video: video.into(),
            exists,
            subtitle: "Spur <happy> path".into(),
            duration: 75.0,
            chapters: vec![(0.0, "1 · Start".into(), "A \"carrier\"".into()), (62.5, "2 · End".into(), "Done".into())],
            cover: Some(vec![1, 2, 3]),
        };
        let p = page("Analyzer & interface", &[e("a.mp4", true), e("b.mp4", false)]);
        assert!(p.contains("<title>Analyzer &amp; interface</title>"));
        assert!(p.contains("2 videos · 2:30 in all"));
        assert!(p.contains("Spur &lt;happy&gt; path"));
        assert!(p.contains("src=\"a.mp4\"") && p.contains("poster=\"data:image/png;base64,AQID\""));
        assert!(p.contains("data-v=\"video1\" data-t=\"62.50\"") && p.contains("<span class=\"at\">1:03</span>"));
        assert!(p.contains("title=\"A &quot;carrier&quot;\""));
        // A missing video: the cover, a note, chapters that don't jump.
        assert!(p.contains("The video b.mp4 is not in this folder.") && p.contains("<img class=\"cover\""));
        assert!(!p.contains("src=\"b.mp4\"") && p.contains(" disabled>"));
    }
}
