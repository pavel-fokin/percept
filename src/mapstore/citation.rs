//! Where a citation's excerpt sits in the working tree now - shared by
//! `start`'s Attention block and the review server's source entries, so
//! a stale range is never read back from the stored event, only ever
//! recomputed from the tree.

/// Each line's trailing whitespace stripped, then leading and trailing
/// blank lines trimmed - the one normalisation both sides of a
/// "changed since recorded" comparison go through, so a citation whose
/// stored excerpt padded its range with context still matches.
fn normalize(text: &str) -> String {
    let lines: Vec<&str> = text.lines().map(|line| line.trim_end()).collect();
    let start = lines.iter().position(|line| !line.is_empty()).unwrap_or(lines.len());
    let end = lines.iter().rposition(|line| !line.is_empty()).map_or(start, |i| i + 1);
    lines[start..end].join("\n")
}

/// The 1-based inclusive line range where `excerpt` sits in `text` now,
/// or `None` when it does not: `excerpt` goes through `normalize`, then
/// its lines are matched whole against a contiguous run of `text`'s
/// lines - each trailing-whitespace-stripped but otherwise kept as
/// given, first match wins. Line numbers count lines of `text` as
/// given, so its leading blank lines are kept, unlike `excerpt`'s:
/// dropping them would shift every number. An excerpt that normalises
/// to nothing is never found.
pub fn locate(excerpt: &str, text: &str) -> Option<(u32, u32)> {
    let excerpt = normalize(excerpt);
    if excerpt.is_empty() {
        return None;
    }
    let excerpt: Vec<&str> = excerpt.split('\n').collect();
    let lines: Vec<&str> = text.lines().map(|line| line.trim_end()).collect();
    if excerpt.len() > lines.len() {
        return None;
    }
    (0..=lines.len() - excerpt.len())
        .find(|&start| lines[start..start + excerpt.len()] == excerpt[..])
        .map(|start| (start as u32 + 1, (start + excerpt.len()) as u32))
}

#[cfg(test)]
mod tests;
