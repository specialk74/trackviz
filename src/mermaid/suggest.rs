//! The closest directive to a mistyped one.

/// Directive names, for suggestions.
const DIRECTIVES: [&str; 25] = [
    "title", "subtitle", "geometry", "carrier", "include", "on", "after", "at", "color", "for", "id", "duration", "move", "pipette", "rotate", "highlight", "operator", "needle", "status",
    "show", "hide", "wait", "caption", "pass", "divert",
];

fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut prev = row[0];
        row[0] = i;
        for j in 1..=b.len() {
            let cur = row[j];
            row[j] = (row[j] + 1).min(row[j - 1] + 1).min(prev + usize::from(a[i - 1] != b[j - 1]));
            prev = cur;
        }
    }
    row[b.len()]
}

/// The closest directive to a mistyped one.
pub(super) fn suggest(d: &str) -> Option<&'static str> {
    let head = d.split_whitespace().next().unwrap_or_default();
    DIRECTIVES
        .iter()
        .map(|&n| (distance(head, n), n))
        .filter(|&(dist, n)| dist > 0 && dist <= 2.max(n.len() / 4))
        .min()
        .map(|x| x.1)
}
