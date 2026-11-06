//! Just enough Markdown parsing for the documentation checks: ATX headings (outside fenced code
//! blocks), GitHub-style heading anchors, and inline link targets.

use std::collections::HashMap;

/// An ATX heading (`## Title`) found in a Markdown file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// Number of leading `#` characters (1–6).
    pub level: usize,
    /// The heading text, trimmed, with any closing `#` sequence removed.
    pub title: String,
    /// The anchor GitHub generates for this heading, de-duplicated within the file.
    pub anchor: String,
    /// 1-based line number.
    pub line: usize,
}

/// Returns every ATX heading in `text`, skipping fenced code blocks.
pub fn headings(text: &str) -> Vec<Heading> {
    let mut found = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut fence: Option<&str> = None;
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if let Some(marker) = fence {
            if trimmed.starts_with(marker) {
                fence = None;
            }
            continue;
        }
        if trimmed.starts_with("```") {
            fence = Some("```");
            continue;
        }
        if trimmed.starts_with("~~~") {
            fence = Some("~~~");
            continue;
        }
        let Some((level, title)) = parse_heading(line) else {
            continue;
        };
        let base = slug(&title);
        let count = seen.entry(base.clone()).or_insert(0);
        let anchor = if *count == 0 {
            base
        } else {
            format!("{base}-{count}")
        };
        *count += 1;
        found.push(Heading {
            level,
            title,
            anchor,
            line: index + 1,
        });
    }
    found
}

/// Parses one line as an ATX heading, returning its level and title.
fn parse_heading(line: &str) -> Option<(usize, String)> {
    // Up to three spaces of indentation are allowed; four makes it a code block.
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let level = rest.len() - rest.trim_start_matches('#').len();
    if !(1..=6).contains(&level) {
        return None;
    }
    let after = &rest[level..];
    if !after.is_empty() && !after.starts_with(' ') && !after.starts_with('\t') {
        return None;
    }
    let mut title = after.trim();
    // A closing sequence of `#`s is not part of the title when preceded by a space.
    let without_closing = title.trim_end_matches('#');
    if without_closing.is_empty() || without_closing.ends_with(' ') {
        title = without_closing.trim_end();
    }
    Some((level, title.to_string()))
}

/// The anchor GitHub generates for a heading title, before de-duplication: lower-cased, with
/// punctuation other than `-` and `_` removed and each space replaced by `-`.
pub fn slug(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            '-' | '_' => Some(c),
            c if c.is_alphanumeric() => Some(c),
            _ => None,
        })
        .collect()
}

/// An inline link target (`[text](target)`) found in a Markdown file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// The target as written, without any title.
    pub target: String,
    /// 1-based line number.
    pub line: usize,
}

/// Returns the targets of every inline link in `text`, skipping fenced code blocks and code spans.
pub fn links(text: &str) -> Vec<Link> {
    let mut found = Vec::new();
    let mut in_fence = false;
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let without_code = strip_code_spans(line);
        let mut rest = without_code.as_str();
        while let Some(start) = rest.find("](") {
            let after = &rest[start + 2..];
            let Some(end) = after.find(')') else {
                break;
            };
            let raw = after[..end].trim();
            let target = raw.split_whitespace().next().unwrap_or_default();
            if !target.is_empty() {
                found.push(Link {
                    target: target.to_string(),
                    line: index + 1,
                });
            }
            rest = &after[end + 1..];
        }
    }
    found
}

/// Removes inline code spans (`` `like this` ``) from a line.
pub fn strip_code_spans(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_code = false;
    for c in line.chars() {
        if c == '`' {
            in_code = !in_code;
        } else if !in_code {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slug_strips_punctuation_and_hyphenates() {
        assert_eq!(slug("Real-time thread"), "real-time-thread");
        assert_eq!(slug("Who is this for?"), "who-is-this-for");
        assert_eq!(slug("`Cargo.lock` changes"), "cargolock-changes");
        assert_eq!(slug("snake_case stays"), "snake_case-stays");
    }

    #[test]
    fn test_headings_skips_fenced_code() {
        let text = "# Title\n\n```sh\n# not a heading\n```\n\n## Section ##\n#nospace\n";
        let found = headings(text);
        let titles: Vec<&str> = found.iter().map(|h| h.title.as_str()).collect();
        assert_eq!(titles, vec!["Title", "Section"]);
        assert_eq!(found[1].level, 2);
        assert_eq!(found[1].line, 7);
    }

    #[test]
    fn test_headings_deduplicates_anchors() {
        let found = headings("## Notes\n## Notes\n## Notes\n");
        let anchors: Vec<&str> = found.iter().map(|h| h.anchor.as_str()).collect();
        assert_eq!(anchors, vec!["notes", "notes-1", "notes-2"]);
    }

    #[test]
    fn test_headings_keeps_trailing_hash_without_space() {
        let found = headings("## C#\n");
        assert_eq!(found[0].title, "C#");
    }

    #[test]
    fn test_links_ignores_code_spans_and_titles() {
        let text = "See [a](x.md#one \"title\") and `[b](not.md)`.\n[c](y.md)";
        let targets: Vec<String> = links(text).into_iter().map(|l| l.target).collect();
        assert_eq!(targets, vec!["x.md#one", "y.md"]);
    }
}
