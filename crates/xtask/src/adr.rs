//! Architectural Decision Records: checking each record and the index against each other, and
//! creating new records from the template (`cargo xtask new-adr`).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::markdown;
use crate::repo::{self, Problem};

/// The directory holding the records, relative to the root.
const DIR: &str = "docs/adrs/";
/// The index, relative to the root.
const INDEX: &str = "docs/adrs/README.md";
/// The template new records are made from, relative to the root.
const TEMPLATE: &str = "docs/adrs/0000-template.md";
/// The heading the index table sits under.
const INDEX_HEADING: &str = "## Index";
/// Separates the number from the decision in a record's title.
const DASH: &str = " — ";

/// The header fields every record carries, in template order.
const FIELDS: &[&str] = &["Status", "Decided by", "Date", "Issue", "Architecture"];

/// The parts of a record the checks care about.
#[derive(Debug)]
struct Record {
    /// Path relative to the root.
    path: String,
    /// File name within the directory.
    file: String,
    /// The number from the file name.
    number: u32,
    /// The decision from the title, if the title is well formed.
    title: Option<String>,
    /// Header field values by name.
    fields: BTreeMap<String, (usize, String)>,
}

/// A row of the index table.
#[derive(Debug)]
struct Row {
    line: usize,
    file: String,
    title: String,
    status: String,
}

/// The number of a record file name (`0007-slug.md`), excluding the template.
fn number_of(file: &str) -> Option<u32> {
    let (digits, rest) = file.split_at_checked(4)?;
    if !digits.bytes().all(|b| b.is_ascii_digit())
        || !rest.starts_with('-')
        || !rest.ends_with(".md")
    {
        return None;
    }
    digits.parse().ok().filter(|n| *n > 0)
}

/// Replaces `[text](target)` with `text`, so linked and plain spellings compare equal.
fn plain(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let Some(close) = after.find("](") else { break };
        let Some(end) = after[close + 2..].find(')') else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push_str(&after[..close]);
        rest = &after[close + 2 + end + 1..];
    }
    out.push_str(rest);
    out.trim().to_string()
}

/// Whether a value still holds a template placeholder such as `<name>`.
fn has_placeholder(value: &str) -> bool {
    value
        .find('<')
        .is_some_and(|open| value[open..].contains('>'))
}

fn parse_record(root: &Path, path: &str) -> Result<Option<Record>> {
    let Some(file) = path.strip_prefix(DIR).filter(|f| !f.contains('/')) else {
        return Ok(None);
    };
    let Some(number) = number_of(file) else {
        return Ok(None);
    };
    let text = repo::read(root, path)?;
    let prefix = format!("# ADR-{number:04}{DASH}");
    let title = text
        .lines()
        .next()
        .and_then(|l| l.strip_prefix(&prefix))
        .map(|t| t.trim().to_string());
    let mut fields = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        for field in FIELDS {
            if let Some(value) = line.strip_prefix(&format!("- **{field}:**")) {
                fields.insert(field.to_string(), (index + 1, value.trim().to_string()));
            }
        }
    }
    Ok(Some(Record {
        path: path.to_string(),
        file: file.to_string(),
        number,
        title,
        fields,
    }))
}

fn parse_index(text: &str) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut in_index = false;
    for (index, line) in text.lines().enumerate() {
        if line.starts_with("## ") {
            in_index = line.trim() == INDEX_HEADING;
            continue;
        }
        if !in_index || !line.starts_with("| [ADR-") {
            continue;
        }
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        let link = markdown::links(cells.get(1).copied().unwrap_or_default());
        rows.push(Row {
            line: index + 1,
            file: link.first().map(|l| l.target.clone()).unwrap_or_default(),
            title: cells.get(2).copied().unwrap_or_default().to_string(),
            status: plain(cells.get(3).copied().unwrap_or_default()),
        });
    }
    rows
}

fn records(root: &Path, files: &[String]) -> Result<Vec<Record>> {
    let mut found = Vec::new();
    for path in files {
        if let Some(record) = parse_record(root, path)? {
            found.push(record);
        }
    }
    Ok(found)
}

/// Reports prompt text from the template's body (below the header fields) still in a record.
fn leftover_template_text(root: &Path, files: &[String], record: &Record) -> Result<Vec<Problem>> {
    if !files.iter().any(|f| f == TEMPLATE) {
        return Ok(vec![]);
    }
    let template = repo::read(root, TEMPLATE)?;
    let prompts: Vec<&str> = template
        .lines()
        .skip_while(|l| !l.starts_with("## "))
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    let text = repo::read(root, &record.path)?;
    Ok(text
        .lines()
        .enumerate()
        .filter(|(_, line)| prompts.contains(&line.trim()))
        .map(|(index, _)| {
            Problem::at(
                &record.path,
                index + 1,
                "still has the template's prompt text: write this section",
            )
        })
        .collect())
}

/// Reports malformed records, duplicate numbers, and disagreements between records and the index.
pub fn check(root: &Path, files: &[String]) -> Result<Vec<Problem>> {
    let mut problems = Vec::new();
    let records = records(root, files)?;

    let mut by_number: BTreeMap<u32, &Record> = BTreeMap::new();
    for record in &records {
        if let Some(other) = by_number.insert(record.number, record) {
            problems.push(Problem::in_file(
                &record.path,
                format!("number {:04} is also used by {}", record.number, other.file),
            ));
        }
        if record.title.is_none() || record.title.as_deref().is_some_and(has_placeholder) {
            problems.push(Problem::at(
                &record.path,
                1,
                format!(
                    "title must be `# ADR-{:04}{DASH}<the decision>`",
                    record.number
                ),
            ));
        }
        for field in FIELDS {
            match record.fields.get(*field) {
                None => problems.push(Problem::in_file(
                    &record.path,
                    format!("missing the `- **{field}:**` line"),
                )),
                Some((line, value)) if value.is_empty() || has_placeholder(value) => problems.push(
                    Problem::at(&record.path, *line, format!("{field} is not filled in")),
                ),
                Some(_) => {}
            }
        }
        if let Some((line, status)) = record.fields.get("Status") {
            let status = plain(status);
            if status != "Accepted" && !status.starts_with("Superseded by ADR-") {
                problems.push(Problem::at(
                    &record.path,
                    *line,
                    "Status must be `Accepted` or `Superseded by ADR-NNNN`",
                ));
            }
        }
        problems.extend(leftover_template_text(root, files, record)?);
    }

    let Some(index_text) = files
        .iter()
        .any(|f| f == INDEX)
        .then(|| repo::read(root, INDEX))
        .transpose()?
    else {
        if !records.is_empty() {
            problems.push(Problem::in_file(INDEX, "missing"));
        }
        return Ok(problems);
    };
    let rows = parse_index(&index_text);
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for row in &rows {
        if let Some(first) = seen.insert(&row.file, row.line) {
            problems.push(Problem::at(
                INDEX,
                row.line,
                format!("{} is already listed on line {first}", row.file),
            ));
        }
        let Some(record) = records.iter().find(|r| r.file == row.file) else {
            problems.push(Problem::at(
                INDEX,
                row.line,
                format!("lists {}, which is not a record in {DIR}", row.file),
            ));
            continue;
        };
        if let Some(title) = &record.title
            && *title != row.title
        {
            problems.push(Problem::at(
                INDEX,
                row.line,
                format!("title differs from {}: `{title}`", record.file),
            ));
        }
        if let Some((_, status)) = record.fields.get("Status")
            && plain(status) != row.status
        {
            problems.push(Problem::at(
                INDEX,
                row.line,
                format!("status differs from {}: `{}`", record.file, plain(status)),
            ));
        }
    }
    for record in &records {
        if !rows.iter().any(|r| r.file == record.file) {
            problems.push(Problem::in_file(
                &record.path,
                format!("not listed in the {INDEX} table"),
            ));
        }
    }
    Ok(problems)
}

/// Options for a new record.
#[derive(Debug, Default)]
pub struct NewAdr {
    /// The decision, as a declarative sentence.
    pub decision: String,
    /// Who decided; `human` means the git `user.name`.
    pub decided_by: String,
    /// The GitHub issue number, if any.
    pub issue: Option<u32>,
    /// The architecture section it supports, as `docs/architecture/<file>.md#<anchor>`.
    pub section: Option<String>,
    /// The date to record, `YYYY-MM-DD`; today when absent.
    pub date: Option<String>,
}

/// A file-name slug for a decision: lower-case words joined by `-`, at most about 50 characters.
fn file_slug(decision: &str) -> String {
    let mut slug = String::new();
    for word in decision
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        if !slug.is_empty() && slug.len() + word.len() >= 50 {
            break;
        }
        if !slug.is_empty() {
            slug.push('-');
        }
        slug.push_str(word);
    }
    slug
}

fn command_output(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("running {program}"))?;
    if !output.status.success() {
        bail!("{program} {} failed", args.join(" "));
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

/// Resolves a `docs/architecture/<file>.md#<anchor>` reference to (link from the ADR directory,
/// heading title).
fn section_link(root: &Path, section: &str) -> Result<(String, String)> {
    let Some((file, anchor)) = section.split_once('#') else {
        bail!("--section must be docs/architecture/<file>.md#<anchor>");
    };
    let Some(name) = file.strip_prefix("docs/architecture/") else {
        bail!("--section must be under docs/architecture/");
    };
    let headings = markdown::headings(&repo::read(root, file)?);
    let Some(heading) = headings.iter().find(|h| h.anchor == anchor) else {
        bail!("{file} has no heading with anchor #{anchor}");
    };
    Ok((
        format!("../architecture/{name}#{anchor}"),
        heading.title.clone(),
    ))
}

/// Creates the next record from the template and adds its index row. Returns its path.
pub fn create(root: &Path, options: &NewAdr) -> Result<String> {
    let decision = options.decision.trim();
    if decision.is_empty() || decision.contains('|') || decision.contains('\n') {
        bail!("the decision must be a non-empty single line without `|`");
    }
    let decided_by = match options.decided_by.trim() {
        "human" => command_output("git", &["config", "user.name"])
            .context("--decided-by human needs `git config user.name`")?,
        "" => bail!(
            "--decided-by is required: a name, `agent, ratified by <who>`, or `agent, unratified`"
        ),
        other => other.to_string(),
    };
    let date = match &options.date {
        Some(date) => date.clone(),
        None => command_output("date", &["+%Y-%m-%d"])?,
    };
    let section = options
        .section
        .as_deref()
        .map(|s| section_link(root, s))
        .transpose()?;

    let files = repo::files(root)?;
    let index_text = repo::read(root, INDEX)?;
    let highest_file = records(root, &files)?.iter().map(|r| r.number).max();
    let highest_row = parse_index(&index_text)
        .iter()
        .filter_map(|r| number_of(&r.file))
        .max();
    let number = highest_file.max(highest_row).unwrap_or(0) + 1;
    let file = format!("{number:04}-{}.md", file_slug(decision));
    let path = format!("{DIR}{file}");

    let template = repo::read(root, TEMPLATE)?;
    let mut body = String::new();
    for (index, line) in template.lines().enumerate() {
        let filled = if index == 0 {
            format!("# ADR-{number:04}{DASH}{decision}")
        } else if line.starts_with("- **Status:**") {
            "- **Status:** Accepted".to_string()
        } else if line.starts_with("- **Decided by:**") {
            format!("- **Decided by:** {decided_by}")
        } else if line.starts_with("- **Date:**") {
            format!("- **Date:** {date}")
        } else if line.starts_with("- **Issue:**") {
            match options.issue {
                Some(issue) => format!("- **Issue:** #{issue}"),
                None => "- **Issue:** —".to_string(),
            }
        } else if line.starts_with("- **Architecture:**") {
            match &section {
                Some((link, title)) => format!("- **Architecture:** [{title}]({link})"),
                None => line.to_string(),
            }
        } else {
            line.to_string()
        };
        body.push_str(&filled);
        body.push('\n');
    }

    let section_cell = section.map_or_else(
        || "—".to_string(),
        |(link, title)| format!("[{title}]({link})"),
    );
    let row = format!("| [ADR-{number:04}]({file}) | {decision} | Accepted | {section_cell} |");
    let index_text = insert_row(&index_text, &row)?;

    fs::write(root.join(&path), body)?;
    fs::write(root.join(INDEX), index_text)?;
    Ok(path)
}

/// Appends `row` to the end of the table under the index heading.
fn insert_row(index_text: &str, row: &str) -> Result<String> {
    let lines: Vec<&str> = index_text.lines().collect();
    let Some(heading) = lines.iter().position(|l| l.trim() == INDEX_HEADING) else {
        bail!("{INDEX} has no `{INDEX_HEADING}` heading");
    };
    let mut last_table_line = None;
    for (offset, line) in lines[heading + 1..].iter().enumerate() {
        if line.starts_with('|') {
            last_table_line = Some(heading + 1 + offset);
        } else if line.starts_with("## ") || (last_table_line.is_some() && !line.trim().is_empty())
        {
            break;
        }
    }
    let Some(last) = last_table_line else {
        bail!("{INDEX} has no table under `{INDEX_HEADING}`");
    };
    let mut out: Vec<&str> = lines[..=last].to_vec();
    out.push(row);
    out.extend_from_slice(&lines[last + 1..]);
    Ok(out.join("\n") + "\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::testing::TempTree;
    use std::error::Error;

    const TEMPLATE_TEXT: &str = "# ADR-NNNN — <decision, as a declarative sentence>\n\n\
        - **Status:** Accepted\n- **Decided by:** <name>\n- **Date:** YYYY-MM-DD\n\
        - **Issue:** #<number> (or `—`)\n\
        - **Architecture:** [<section name>](../architecture/<file>.md#<slug>)\n\n## Context\n";

    const INDEX_TEXT: &str = "# ADRs\n\n## Index\n\n| ADR | Title | Status | Architecture section |\n\
        |---|---|---|---|\n\n## Later\n";

    fn tree() -> Result<TempTree, Box<dyn Error>> {
        let tree = TempTree::new()?;
        tree.write(TEMPLATE, TEMPLATE_TEXT)?;
        tree.write(INDEX, INDEX_TEXT)?;
        tree.write(
            "docs/architecture/tools.md",
            "# Tools\n\n## Developer tooling\n",
        )?;
        Ok(tree)
    }

    fn new(decision: &str, section: Option<&str>) -> NewAdr {
        NewAdr {
            decision: decision.into(),
            decided_by: "Ada Lovelace".into(),
            section: section.map(Into::into),
            date: Some("2026-11-06".into()),
            ..NewAdr::default()
        }
    }

    #[test]
    fn test_file_slug_truncates_at_word_boundary() {
        assert_eq!(file_slug("Use iced for the UI!"), "use-iced-for-the-ui");
        let long = file_slug(&"word ".repeat(30));
        assert!(long.len() < 50 && long.ends_with("word"));
    }

    #[test]
    fn test_plain_strips_links() {
        assert_eq!(
            plain("Superseded by [ADR-0002](0002-x.md)"),
            "Superseded by ADR-0002"
        );
    }

    #[test]
    fn test_create_numbers_fills_and_indexes() -> Result<(), Box<dyn Error>> {
        let tree = tree()?;
        let first = create(
            tree.path(),
            &new(
                "Use std only",
                Some("docs/architecture/tools.md#developer-tooling"),
            ),
        )?;
        let second = create(tree.path(), &new("Second decision", None))?;
        assert_eq!(first, "docs/adrs/0001-use-std-only.md");
        assert_eq!(second, "docs/adrs/0002-second-decision.md");

        let record = repo::read(tree.path(), &first)?;
        assert!(record.starts_with("# ADR-0001 — Use std only\n"));
        assert!(record.contains("- **Decided by:** Ada Lovelace\n"));
        assert!(record.contains("- **Date:** 2026-11-06\n"));
        assert!(record.contains("- **Issue:** —\n"));
        assert!(record.contains(
            "- **Architecture:** [Developer tooling](../architecture/tools.md#developer-tooling)\n"
        ));

        let index = repo::read(tree.path(), INDEX)?;
        let first_row = index.find("| [ADR-0001]").ok_or("row 1 missing")?;
        let second_row = index.find("| [ADR-0002]").ok_or("row 2 missing")?;
        let later = index.find("## Later").ok_or("heading lost")?;
        assert!(first_row < second_row && second_row < later);

        // The second record still has the Architecture placeholder, which check reports.
        let files = repo::files(tree.path())?;
        let problems = check(tree.path(), &files)?;
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].message.contains("Architecture"));
        Ok(())
    }

    #[test]
    fn test_check_reports_leftover_template_text() -> Result<(), Box<dyn Error>> {
        let tree = tree()?;
        tree.write(
            TEMPLATE,
            &format!("{TEMPLATE_TEXT}\nWhat forced a decision.\n\n## Decision\n\nA rule.\n"),
        )?;
        let path = create(
            tree.path(),
            &new("X", Some("docs/architecture/tools.md#developer-tooling")),
        )?;
        let files = repo::files(tree.path())?;
        let problems = check(tree.path(), &files)?;
        assert_eq!(problems.len(), 2, "{problems:?}");
        tree.write(
            &path,
            &repo::read(tree.path(), &path)?
                .replace("What forced a decision.", "Real context.")
                .replace("A rule.", "Real rule."),
        )?;
        assert_eq!(check(tree.path(), &files)?, vec![]);
        Ok(())
    }

    #[test]
    fn test_create_rejects_unknown_section() -> Result<(), Box<dyn Error>> {
        let tree = tree()?;
        let result = create(
            tree.path(),
            &new("X", Some("docs/architecture/tools.md#nope")),
        );
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_check_reports_unlisted_and_mismatched() -> Result<(), Box<dyn Error>> {
        let tree = tree()?;
        create(
            tree.path(),
            &new(
                "Listed",
                Some("docs/architecture/tools.md#developer-tooling"),
            ),
        )?;
        let listed = repo::read(tree.path(), "docs/adrs/0001-listed.md")?;
        tree.write(
            "docs/adrs/0001-listed.md",
            &listed.replace(
                "- **Status:** Accepted",
                "- **Status:** Superseded by ADR-0002",
            ),
        )?;
        tree.write(
            "docs/adrs/0002-unlisted.md",
            &listed.replace("ADR-0001", "ADR-0002"),
        )?;
        let files = repo::files(tree.path())?;
        let problems = check(tree.path(), &files)?;
        let messages: Vec<&str> = problems.iter().map(|p| p.message.as_str()).collect();
        assert_eq!(problems.len(), 2, "{messages:?}");
        assert!(messages.iter().any(|m| m.contains("status differs")));
        assert!(messages.iter().any(|m| m.contains("not listed")));
        Ok(())
    }
}
