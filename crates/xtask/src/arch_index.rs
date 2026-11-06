//! Generates the table of contents in `docs/architecture/README.md` from the section files beside
//! it. Only the text between the two markers is generated; everything else in the README is
//! hand-written and left alone.

use std::fs;
use std::path::Path;

use anyhow::{Result, bail};

use crate::markdown;
use crate::repo::{self, Problem};

/// The architecture entry point, relative to the root.
pub const README: &str = "docs/architecture/README.md";

/// Marks the start of the generated region.
pub const BEGIN: &str =
    "<!-- BEGIN GENERATED INDEX: run `cargo xtask generate-architecture-index` -->";

/// Marks the end of the generated region.
pub const END: &str = "<!-- END GENERATED INDEX -->";

/// Deepest heading level listed in the index.
const MAX_LEVEL: usize = 3;

/// Builds the generated region's body: one entry per section file, with its headings nested.
fn render(root: &Path, files: &[String]) -> Result<String> {
    let mut out = String::new();
    for path in files {
        let Some(name) = path.strip_prefix("docs/architecture/") else {
            continue;
        };
        if name == "README.md" || name.contains('/') || !name.ends_with(".md") {
            continue;
        }
        let headings = markdown::headings(&repo::read(root, path)?);
        let title = headings
            .iter()
            .find(|h| h.level == 1)
            .map_or(name, |h| h.title.as_str());
        out.push_str(&format!("- [{title}]({name})\n"));
        for heading in headings
            .iter()
            .filter(|h| (2..=MAX_LEVEL).contains(&h.level))
        {
            let indent = "  ".repeat(heading.level - 1);
            out.push_str(&format!(
                "{indent}- [{}]({name}#{})\n",
                heading.title, heading.anchor
            ));
        }
    }
    if out.is_empty() {
        out.push_str("_No section files yet._\n");
    }
    Ok(out)
}

/// Returns the README with its generated region replaced by `body`.
fn splice(readme: &str, body: &str) -> Result<String> {
    let Some(begin) = readme.find(BEGIN) else {
        bail!("{README} has no `{BEGIN}` marker; add it, then {END}, where the index belongs");
    };
    let after_begin = begin + BEGIN.len();
    let Some(end_offset) = readme[after_begin..].find(END) else {
        bail!("{README} has no `{END}` marker after `{BEGIN}`");
    };
    let end = after_begin + end_offset;
    Ok(format!(
        "{}\n\n{body}\n{}",
        &readme[..after_begin],
        &readme[end..]
    ))
}

/// The README as it should be, regenerated from the section files.
fn regenerated(root: &Path, files: &[String]) -> Result<(String, String)> {
    let current = repo::read(root, README)?;
    let wanted = splice(&current, &render(root, files)?)?;
    Ok((current, wanted))
}

/// Rewrites the generated region of the README. Returns whether the file changed.
pub fn generate(root: &Path) -> Result<bool> {
    let files = repo::files(root)?;
    let (current, wanted) = regenerated(root, &files)?;
    if current == wanted {
        return Ok(false);
    }
    fs::write(root.join(README), wanted)?;
    Ok(true)
}

/// Reports a README whose generated region is missing or out of date.
pub fn check(root: &Path, files: &[String]) -> Result<Vec<Problem>> {
    if !files.iter().any(|f| f == README) {
        return Ok(vec![Problem::in_file(README, "missing")]);
    }
    match regenerated(root, files) {
        Ok((current, wanted)) if current == wanted => Ok(vec![]),
        Ok(_) => Ok(vec![Problem::in_file(
            README,
            "table of contents is out of date: run `cargo xtask generate-architecture-index`",
        )]),
        Err(error) => Ok(vec![Problem::in_file(README, error.to_string())]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::testing::TempTree;
    use std::error::Error;

    fn tree() -> Result<TempTree, Box<dyn Error>> {
        let tree = TempTree::new()?;
        tree.write(
            README,
            &format!("# Architecture\n\nPrinciples stay.\n\n## Contents\n\n{BEGIN}\nstale\n{END}\n\nTail stays.\n"),
        )?;
        tree.write(
            "docs/architecture/engine.md",
            "# Audio engine\n\n## Real-time thread\n\n### Handoff\n\n#### Too deep\n",
        )?;
        Ok(tree)
    }

    #[test]
    fn test_generate_replaces_only_marked_region() -> Result<(), Box<dyn Error>> {
        let tree = tree()?;
        assert!(generate(tree.path())?);
        let readme = repo::read(tree.path(), README)?;
        assert!(readme.contains("Principles stay."));
        assert!(readme.contains("Tail stays."));
        assert!(!readme.contains("stale"));
        assert!(readme.contains("- [Audio engine](engine.md)\n"));
        assert!(readme.contains("  - [Real-time thread](engine.md#real-time-thread)\n"));
        assert!(readme.contains("    - [Handoff](engine.md#handoff)\n"));
        assert!(!readme.contains("Too deep"));
        Ok(())
    }

    #[test]
    fn test_generate_is_idempotent_and_check_passes() -> Result<(), Box<dyn Error>> {
        let tree = tree()?;
        let files = repo::files(tree.path())?;
        assert_eq!(check(tree.path(), &files)?.len(), 1);
        generate(tree.path())?;
        assert!(!generate(tree.path())?);
        assert_eq!(check(tree.path(), &files)?, vec![]);
        Ok(())
    }

    #[test]
    fn test_check_reports_missing_markers() -> Result<(), Box<dyn Error>> {
        let tree = TempTree::new()?;
        tree.write(README, "# Architecture\n")?;
        let files = repo::files(tree.path())?;
        let problems = check(tree.path(), &files)?;
        assert_eq!(problems.len(), 1);
        assert!(problems[0].message.contains("marker"));
        Ok(())
    }
}
