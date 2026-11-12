//! `trackviz sort`: rewrites a scene with its steps in time order, so that the
//! Mermaid diagram (which follows the file) shows the messages in the order of
//! the video. Each step moves with the directives written above it (`on`,
//! `after`, `id`, `color`, `for`, comments); where a moved step was tied to the
//! line before it (no timing, `after <seconds>`, `after <seconds> pipette ...`),
//! it is tied explicitly to the same step with an `id`. The result is checked:
//! the scene must give the same timeline, otherwise the file is left as it is.

use crate::scene::{Action, Anchor, Scene, StartMode, Step, Timeline};
use std::collections::{HashMap, HashSet};

/// A step of the scene's own file with the lines that go with it.
struct Block {
    /// Index of the step in `Scene::steps`.
    step: usize,
    /// The lines, the step's own one at `own`.
    lines: Vec<String>,
    own: usize,
    /// When it starts in the video.
    t0: f64,
}

/// The directives that belong to the next step.
fn is_prefix(line: &str) -> bool {
    let l = line.trim();
    if l.is_empty() {
        return true;
    }
    let Some(d) = l.strip_prefix("%%") else { return false };
    let d = d.trim();
    let word = d.split_whitespace().next().unwrap_or_default();
    d.starts_with('#') || ["on", "after", "at", "color", "for", "id", "duration"].contains(&word)
}

/// What a `when` on an action refers to ("pipette sp1"), as the timeline sees it.
fn keys(st: &Step) -> Vec<String> {
    let mut v: Vec<String> = st.id.iter().map(|id| format!("step {id}")).collect();
    match &st.action {
        Action::Pipette { device, .. } => v.push(format!("pipette {device}")),
        Action::Rotate { device } => v.push(format!("rotate {device}")),
        Action::Needle { device, .. } => v.push(format!("needle {device}")),
        _ => {}
    }
    v
}

fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

/// The antenna written for a device (its own name if it has none).
fn antenna_of(sc: &Scene, device: &str) -> String {
    sc.devices.iter().find(|d| d.kind == crate::scene::DeviceKind::Antenna && d.part_of.as_deref() == Some(device)).map_or_else(|| device.to_string(), |d| d.id.clone())
}

/// A readable name for a step that needs an id: what it does and its carrier
/// (for a message without `SampleId=`, the carrier whose sample its text names).
fn name_for(sc: &Scene, st: &Step) -> String {
    let base = match &st.action {
        Action::Message { text, carrier, .. } => {
            let first = text.split(|c: char| c.is_whitespace() || c == '<').find(|w| !w.is_empty()).unwrap_or_default();
            let named = sc.carriers.iter().find(|c| !c.ghost && !c.sample_id.is_empty() && text.contains(&c.sample_id)).map(|c| c.id.clone());
            let mut n = slug(first);
            if let Some(c) = carrier.clone().or(named) {
                n.push('-');
                n.push_str(&slug(&c));
            }
            n
        }
        Action::Visibility { carrier, show } => format!("{}-{}", if *show { "show" } else { "hide" }, slug(carrier)),
        Action::Pipette { device, .. } => format!("pipette-{}", slug(&antenna_of(sc, device))),
        Action::Needle { device, .. } => format!("needle-{}", slug(&antenna_of(sc, device))),
        Action::Rotate { device } => format!("rotate-{}", slug(device)),
        Action::Pass { device, divert, carrier, .. } => {
            format!("{}-{}{}", if *divert { "divert" } else { "pass" }, slug(device), carrier.as_deref().map_or(String::new(), |c| format!("-{}", slug(c))))
        }
        Action::Move { carrier, .. } => format!("move-{}", slug(carrier)),
        Action::Caption { .. } => "caption".to_string(),
        Action::Note { .. } => "note".to_string(),
        _ => "step".to_string(),
    };
    if base.is_empty() || base.starts_with(|c: char| c.is_ascii_digit()) { format!("step-{base}") } else { base }
}

/// The text of the scene with its steps in time order, and how many blocks moved
/// and how many anchors were written; `None` when it is already in order.
pub fn sorted(path: &str, text: &str, sc: &Scene, tl: &Timeline) -> Result<Option<(String, usize, usize)>, String> {
    let lines: Vec<&str> = text.lines().collect();
    // The steps written in this file, by line.
    let own: HashMap<usize, usize> = sc.steps.iter().enumerate().filter(|(_, s)| s.at.0 == path).map(|(i, s)| (s.at.1, i)).collect();
    if own.is_empty() {
        return Ok(None);
    }
    let mut t0_of: HashMap<usize, f64> = HashMap::new();
    for e in &tl.events {
        if let Some((f, l)) = &e.at {
            if f == path {
                let t = t0_of.entry(*l).or_insert(e.t0);
                *t = t.min(e.t0);
            }
        }
    }
    let first = *own.keys().min().unwrap();
    // The header: everything before the directives of the first step.
    let mut start = first - 1;
    while start > 0 && is_prefix(lines[start - 1]) {
        start -= 1;
    }
    let mut blocks: Vec<Block> = Vec::new();
    let mut pending: Vec<String> = Vec::new();
    let mut end = lines.len();
    for (i, l) in lines.iter().enumerate().skip(start) {
        let no = i + 1;
        if let Some(&si) = own.get(&no) {
            pending.push(l.to_string());
            let own_at = pending.len() - 1;
            let t0 = t0_of.get(&no).copied().unwrap_or_else(|| blocks.last().map_or(0.0, |b| b.t0));
            blocks.push(Block { step: si, lines: std::mem::take(&mut pending), own: own_at, t0 });
        } else if is_prefix(l) {
            pending.push(l.to_string());
        } else if l.trim_start().starts_with("Note") && blocks.last().is_some_and(|b| matches!(sc.steps[b.step].action, Action::Pipette { .. })) && pending.iter().all(|p| p.trim().is_empty()) {
            // The text of the pipette's note: it stays with the pipette.
            let b = blocks.last_mut().unwrap();
            b.lines.append(&mut pending);
            b.lines.push(l.to_string());
        } else if no > *own.keys().max().unwrap() {
            end = i - pending.len();
            break;
        } else {
            return Err(format!("{path} line {no}: '{}' is between the steps: move it above the first one, then sort", l.trim()));
        }
    }
    let tail: Vec<String> = if end < lines.len() {
        lines[end..].iter().map(|l| l.to_string()).collect()
    } else {
        std::mem::take(&mut pending)
    };

    let old: Vec<usize> = blocks.iter().map(|b| b.step).collect();
    let mut order: Vec<usize> = (0..blocks.len()).collect();
    order.sort_by(|&a, &b| blocks[a].t0.total_cmp(&blocks[b].t0).then(a.cmp(&b)));
    if order.iter().enumerate().all(|(i, &k)| i == k) {
        return Ok(None);
    }
    // The fewest blocks to move: all but the longest run already in order.
    let mut tails: Vec<usize> = Vec::new();
    for &k in &order {
        match tails.binary_search(&k) {
            Ok(_) => {}
            Err(p) if p == tails.len() => tails.push(k),
            Err(p) => tails[p] = k,
        }
    }
    let moved = order.len() - tails.len();

    // Ids already written, and the ones given now.
    let mut taken: HashSet<String> = sc.steps.iter().filter_map(|s| s.id.clone()).collect();
    let mut given: HashMap<usize, String> = HashMap::new();
    let mut id_of = |si: usize, given: &mut HashMap<usize, String>| -> Result<String, String> {
        if let Some(id) = &sc.steps[si].id {
            return Ok(id.clone());
        }
        if let Some(id) = given.get(&si) {
            return Ok(id.clone());
        }
        if sc.steps[si].at.0 != path {
            return Err(format!("{}: the step written in another file must have an id (%% id) to be waited for after sorting", sc.steps[si].loc));
        }
        let base = name_for(sc, &sc.steps[si]);
        let mut id = base.clone();
        let mut n = 2;
        while taken.contains(&id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        taken.insert(id.clone());
        given.insert(si, id.clone());
        Ok(id)
    };

    // The step before each one, in the file as it is and as it will be.
    let before_old = |si: usize| si.checked_sub(1);
    let pos_new: HashMap<usize, usize> = order.iter().enumerate().map(|(p, &b)| (blocks[b].step, p)).collect();
    let first_own = old[0];
    let before_new = |si: usize| -> Option<usize> {
        let p = pos_new[&si];
        if p == 0 { first_own.checked_sub(1) } else { Some(blocks[order[p - 1]].step) }
    };
    // The steps in the new order (those of other files first, as they are).
    let new_seq: Vec<usize> = (0..first_own).chain(order.iter().map(|&b| blocks[b].step)).collect();

    // What each moved step must now say about when it starts.
    let mut timing: HashMap<usize, String> = HashMap::new();
    for b in &blocks {
        let si = b.step;
        let st = &sc.steps[si];
        let delay = st.delay;
        let secs = |d: f64| {
            let s = format!("{d:.3}");
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        };
        match &st.start {
            StartMode::After => {
                if before_old(si) == before_new(si) {
                    continue;
                }
                let line = match before_old(si) {
                    None => format!("at {}", secs(delay)),
                    Some(prev) => format!("after {} {}", secs(delay), id_of(prev, &mut given)?),
                };
                timing.insert(si, line);
            }
            StartMode::When(Anchor::Event { key }) if !key.starts_with("step ") => {
                let latest = |seq: &[usize]| seq.iter().take_while(|&&k| k != si).filter(|&&k| keys(&sc.steps[k]).contains(key)).last().copied();
                let old_ref = latest(&(0..sc.steps.len()).collect::<Vec<_>>());
                if old_ref == latest(&new_seq) {
                    continue;
                }
                let r = old_ref.ok_or_else(|| format!("{}: no '{key}' before this step", st.loc))?;
                timing.insert(si, format!("after {} {}", secs(delay), id_of(r, &mut given)?));
            }
            _ => {}
        }
    }
    let anchors = timing.len();

    // The blocks rewritten: an id above the step that is waited for, the new
    // timing in place of the old one (or above the step).
    let indent = |l: &str| l[..l.len() - l.trim_start().len()].to_string();
    let mut out: Vec<String> = lines[..start].iter().map(|l| l.to_string()).collect();
    for &bi in &order {
        let b = &blocks[bi];
        let ind = indent(&b.lines[b.own]);
        let mut ls: Vec<String> = Vec::new();
        for (k, l) in b.lines.iter().enumerate() {
            let d = l.trim().strip_prefix("%%").map(str::trim);
            let is_timing = d.is_some_and(|d| {
                let w = d.split_whitespace().next().unwrap_or_default();
                w == "after" || w == "at"
            });
            if is_timing && timing.contains_key(&b.step) {
                continue;
            }
            if k == b.own {
                if let Some(t) = timing.get(&b.step) {
                    ls.push(format!("{ind}%% {t}"));
                }
                if let Some(id) = given.get(&b.step) {
                    ls.push(format!("{ind}%% id {id}"));
                }
            }
            ls.push(l.clone());
        }
        out.extend(ls);
    }
    out.extend(tail);
    let mut s = out.join("\n");
    if text.ends_with('\n') {
        s.push('\n');
    }
    Ok(Some((s, moved, anchors)))
}

/// The timeline in a form that doesn't depend on the order of the file.
pub fn same_timeline(a: &Timeline, b: &Timeline) -> Result<(), String> {
    let norm = |t: &Timeline| {
        let mut l = t.log.clone();
        l.sort();
        l
    };
    if (a.duration - b.duration).abs() > 1e-9 {
        return Err(format!("the length changes: {:.2}s -> {:.2}s", a.duration, b.duration));
    }
    let (x, y) = (norm(a), norm(b));
    if let Some((p, q)) = x.iter().zip(&y).find(|(p, q)| p != q) {
        return Err(format!("the timeline changes: '{}' becomes '{}'", p.trim(), q.trim()));
    }
    if x.len() != y.len() {
        return Err("the timeline changes: a different number of steps".into());
    }
    Ok(())
}
