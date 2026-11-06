//! Checks that references into the documentation still resolve:
//!
//! - **Heading citations** in any text file: `docs/architecture/<file>.md "<Heading title>"`, as
//!   source comments and `deny.toml` write them. The file must exist and have that heading.
//! - **Relative links** in Markdown files: `[text](../path.md#anchor)`. The target must exist and,
//!   when an anchor is given for a Markdown target, the target must have a heading with that anchor.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;

use crate::markdown;
use crate::repo::{self, Problem};

/// Extensions of files scanned for heading citations.
const TEXT_EXTENSIONS: &[&str] = &["md", "rs", "toml", "yml", "yaml", "json", "sh"];

/// Files whose heading citations are examples of the convention rather than real citations.
const EXAMPLE_FILES: &[&str] = &["CLAUDE.md"];

const ARCHITECTURE_DIR: &str = "docs/architecture/";

/// A heading citation found in a file.
#[derive(Debug, PartialEq, Eq)]
struct Citation {
    /// The cited file, relative to the root.
    file: String,
    /// The quoted heading title, if one follows the path.
    title: Option<String>,
    /// 1-based line number.
    line: usize,
}

/// Finds every `docs/architecture/<file>.md` reference in `text`, with its quoted heading title.
fn citations(text: &str) -> Vec<Citation> {
    let mut found = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let mut rest = line;
        while let Some(start) = rest.find(ARCHITECTURE_DIR) {
            let after = &rest[start + ARCHITECTURE_DIR.len()..];
            let name_len = after
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
                .unwrap_or(after.len());
            let name = &after[..name_len];
            let tail = &after[name_len..];
            rest = tail;
            if name.is_empty() || !tail.starts_with(".md") {
                continue;
            }
            let tail = &tail[3..];
            // A link anchor (`.md#...`) is checked as a Markdown link, not as a citation.
            if tail.starts_with('#') {
                continue;
            }
            let quoted = tail.trim_start_matches([' ', '\t', '`']);
            let title = quoted
                .strip_prefix('"')
                .and_then(|q| q.find('"').map(|end| q[..end].to_string()));
            found.push(Citation {
                file: format!("{ARCHITECTURE_DIR}{name}.md"),
                title,
                line: index + 1,
            });
        }
    }
    found
}

/// Whether `path` is an integration test, whose fixtures may cite files that do not exist.
fn is_test_file(path: &str) -> bool {
    path.starts_with("tests/") || path.contains("/tests/")
}

/// The part of a Rust source file before its `#[cfg(test)]` module. Unit tests sit at the bottom
/// of each file (CLAUDE.md, "Testing"), and their fixtures may cite files that do not exist.
fn without_test_module(text: &str) -> &str {
    text.find("\n#[cfg(test)]").map_or(text, |end| &text[..end])
}

/// Reports heading citations and relative Markdown links that no longer resolve.
pub fn check(root: &Path, files: &[String]) -> Result<Vec<Problem>> {
    let mut problems = Vec::new();
    let mut headings_cache: HashMap<String, Vec<markdown::Heading>> = HashMap::new();
    let mut headings_of = |file: &str| -> Result<Vec<markdown::Heading>> {
        if let Some(found) = headings_cache.get(file) {
            return Ok(found.clone());
        }
        let found = markdown::headings(&repo::read(root, file)?);
        headings_cache.insert(file.to_string(), found.clone());
        Ok(found)
    };

    for path in files {
        let extension = path.rsplit_once('.').map(|(_, e)| e).unwrap_or_default();
        if !TEXT_EXTENSIONS.contains(&extension) || is_test_file(path) {
            continue;
        }
        let text = repo::read(root, path)?;

        if !EXAMPLE_FILES.contains(&path.as_str()) {
            let scanned = if extension == "rs" {
                without_test_module(&text)
            } else {
                &text
            };
            for citation in citations(scanned) {
                if !root.join(&citation.file).is_file() {
                    problems.push(Problem::at(
                        path,
                        citation.line,
                        format!("cites {}, which does not exist", citation.file),
                    ));
                    continue;
                }
                let Some(title) = citation.title else {
                    continue;
                };
                if !headings_of(&citation.file)?
                    .iter()
                    .any(|h| h.title == title)
                {
                    problems.push(Problem::at(
                        path,
                        citation.line,
                        format!(
                            "cites {} \"{title}\", but that file has no such heading",
                            citation.file
                        ),
                    ));
                }
            }
        }

        if extension != "md" {
            continue;
        }
        for link in markdown::links(&text) {
            // `<...>` marks a template placeholder, not a real target.
            if link.target.contains("://")
                || link.target.starts_with("mailto:")
                || link.target.contains('<')
            {
                continue;
            }
            let (target, anchor) = match link.target.split_once('#') {
                Some((t, a)) => (t, Some(a)),
                None => (link.target.as_str(), None),
            };
            let resolved = if target.is_empty() {
                Some(path.clone())
            } else {
                repo::resolve(path, target)
            };
            let Some(resolved) = resolved else {
                problems.push(Problem::at(
                    path,
                    link.line,
                    format!("links to {}, outside the repository", link.target),
                ));
                continue;
            };
            if !root.join(&resolved).exists() {
                problems.push(Problem::at(
                    path,
                    link.line,
                    format!("links to {}, which does not exist", link.target),
                ));
                continue;
            }
            let Some(anchor) = anchor else {
                continue;
            };
            if !resolved.ends_with(".md") {
                continue;
            }
            if !headings_of(&resolved)?.iter().any(|h| h.anchor == anchor) {
                problems.push(Problem::at(
                    path,
                    link.line,
                    format!(
                        "links to {}, but {resolved} has no heading with anchor #{anchor}",
                        link.target
                    ),
                ));
            }
        }
    }
    Ok(problems)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::testing::TempTree;
    use std::error::Error;

    #[test]
    fn test_citations_parses_quoted_titles() {
        let text = "// See docs/architecture/engine.md \"Real-time thread\".\n\
                    `docs/architecture/engine.md` and docs/architecture/<file>.md\n\
                    [x](../architecture/engine.md#real-time-thread)";
        let found = citations(text);
        assert_eq!(
            found,
            vec![
                Citation {
                    file: "docs/architecture/engine.md".into(),
                    title: Some("Real-time thread".into()),
                    line: 1,
                },
                Citation {
                    file: "docs/architecture/engine.md".into(),
                    title: None,
                    line: 2,
                },
            ]
        );
    }

    fn tree() -> Result<TempTree, Box<dyn Error>> {
        let tree = TempTree::new()?;
        tree.write(
            "docs/architecture/engine.md",
            "# Engine\n\n## Real-time thread\n",
        )?;
        Ok(tree)
    }

    #[test]
    fn test_check_accepts_valid_references() -> Result<(), Box<dyn Error>> {
        let tree = tree()?;
        tree.write(
            "src/lib.rs",
            "// docs/architecture/engine.md \"Real-time thread\"\n",
        )?;
        tree.write(
            "docs/adrs/0001-x.md",
            "[s](../architecture/engine.md#real-time-thread) [t](#local)\n## Local\n",
        )?;
        let files = repo::files(tree.path())?;
        assert_eq!(check(tree.path(), &files)?, vec![]);
        Ok(())
    }

    #[test]
    fn test_check_reports_missing_heading() -> Result<(), Box<dyn Error>> {
        let tree = tree()?;
        tree.write("deny.toml", "# docs/architecture/engine.md \"Old title\"\n")?;
        let files = repo::files(tree.path())?;
        let problems = check(tree.path(), &files)?;
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].path, "deny.toml");
        Ok(())
    }

    #[test]
    fn test_check_reports_broken_link_and_anchor() -> Result<(), Box<dyn Error>> {
        let tree = tree()?;
        tree.write(
            "docs/adrs/README.md",
            "[a](../../adrs/README.md) [b](../architecture/engine.md#gone) [c](../nope.md)\n",
        )?;
        let files = repo::files(tree.path())?;
        let problems = check(tree.path(), &files)?;
        assert_eq!(problems.len(), 3, "{problems:?}");
        Ok(())
    }

    #[test]
    fn test_check_skips_rust_test_modules_and_placeholders() -> Result<(), Box<dyn Error>> {
        let tree = tree()?;
        tree.write(
            "src/lib.rs",
            "// docs/architecture/engine.md \"Real-time thread\"\n\
             #[cfg(test)]\nmod tests { // docs/architecture/fixture.md \"Nope\"\n}\n",
        )?;
        tree.write("tests/it.rs", "// docs/architecture/fixture.md\n")?;
        tree.write(
            "docs/adrs/0000-template.md",
            "[s](../architecture/<file>.md#<slug>)\n",
        )?;
        let files = repo::files(tree.path())?;
        assert_eq!(check(tree.path(), &files)?, vec![]);
        Ok(())
    }

    #[test]
    fn test_check_skips_example_files() -> Result<(), Box<dyn Error>> {
        let tree = tree()?;
        tree.write("CLAUDE.md", "`docs/architecture/missing.md \"Example\"`\n")?;
        let files = repo::files(tree.path())?;
        assert_eq!(check(tree.path(), &files)?, vec![]);
        Ok(())
    }
}
