//! Line budgets for each kind of document. When a file reaches its budget, split it by topic; do
//! not raise the limit to fit more in (CLAUDE.md, "Documentation Boundaries").

use std::path::Path;

use anyhow::Result;

use crate::repo::{self, Problem};

/// One kind of document and its line budget.
struct Budget {
    /// What the kind is, for messages.
    kind: &'static str,
    /// Whether a root-relative path is of this kind.
    matches: fn(&str) -> bool,
    /// Maximum number of lines.
    max_lines: usize,
}

/// The budget table. The first matching row applies; files matching no row are unbudgeted.
const BUDGETS: &[Budget] = &[
    Budget {
        kind: "agent instructions (CLAUDE.md)",
        matches: |p| p == "CLAUDE.md",
        max_lines: 220,
    },
    Budget {
        kind: "project README",
        matches: |p| p == "README.md",
        max_lines: 100,
    },
    Budget {
        kind: "glossary",
        matches: |p| p == "GLOSSARY.md",
        max_lines: 300,
    },
    Budget {
        kind: "architecture entry point",
        matches: |p| p == "docs/architecture/README.md",
        max_lines: 200,
    },
    Budget {
        kind: "architecture section",
        matches: |p| in_dir(p, "docs/architecture/"),
        max_lines: 300,
    },
    Budget {
        kind: "ADR index",
        matches: |p| p == "docs/adrs/README.md",
        max_lines: 400,
    },
    Budget {
        kind: "ADR",
        matches: |p| in_dir(p, "docs/adrs/"),
        max_lines: 150,
    },
    Budget {
        kind: "product brief",
        matches: |p| in_dir(p, "docs/product/"),
        max_lines: 200,
    },
    Budget {
        kind: "user guide",
        matches: |p| in_dir(p, "docs/user/"),
        max_lines: 300,
    },
    Budget {
        kind: "agent guide",
        matches: |p| in_dir(p, "docs/agents/"),
        max_lines: 150,
    },
    Budget {
        kind: "skill",
        matches: |p| p.starts_with(".claude/skills/") && p.ends_with(".md"),
        max_lines: 120,
    },
];

/// Whether `path` is a Markdown file directly inside `dir` (which ends in `/`).
fn in_dir(path: &str, dir: &str) -> bool {
    path.strip_prefix(dir)
        .is_some_and(|rest| !rest.contains('/') && rest.ends_with(".md"))
}

/// The budget row that applies to `path`, if any.
fn budget_for(path: &str) -> Option<&'static Budget> {
    BUDGETS.iter().find(|b| (b.matches)(path))
}

/// Reports every file in `files` that is over its line budget.
pub fn check(root: &Path, files: &[String]) -> Result<Vec<Problem>> {
    let mut problems = Vec::new();
    for path in files {
        let Some(budget) = budget_for(path) else {
            continue;
        };
        let lines = repo::read(root, path)?.lines().count();
        if lines > budget.max_lines {
            problems.push(Problem::in_file(
                path,
                format!(
                    "{lines} lines is over the {} budget of {}: split it by topic \
                     (do not raise the limit)",
                    budget.kind, budget.max_lines
                ),
            ));
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
    fn test_budget_for_picks_specific_row_first() {
        let kind = |p| budget_for(p).map(|b| b.kind);
        assert_eq!(
            kind("docs/architecture/README.md"),
            Some("architecture entry point")
        );
        assert_eq!(
            kind("docs/architecture/engine.md"),
            Some("architecture section")
        );
        assert_eq!(kind("docs/adrs/0001-x.md"), Some("ADR"));
        assert_eq!(kind("docs/architecture/sub/x.md"), None);
        assert_eq!(kind("src/main.rs"), None);
    }

    #[test]
    fn test_check_reports_file_over_budget() -> Result<(), Box<dyn Error>> {
        let tree = TempTree::new()?;
        tree.write("docs/adrs/0001-long.md", &"line\n".repeat(151))?;
        tree.write("docs/adrs/0002-short.md", &"line\n".repeat(150))?;
        let files = repo::files(tree.path())?;
        let problems = check(tree.path(), &files)?;
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].path, "docs/adrs/0001-long.md");
        Ok(())
    }
}
