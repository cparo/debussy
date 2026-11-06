//! Repository maintenance tasks, run as `cargo xtask <command>`.
//!
//! This crate uses only the standard library and `anyhow`: see
//! docs/architecture/tooling.md "Repository tasks (xtask)".

#![forbid(unsafe_code)]

mod adr;
mod arch_index;
mod budget;
mod citations;
mod crate_rules;
mod hooks;
mod json;
mod markdown;
mod metadata;
mod repo;

use std::path::Path;
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};

const USAGE: &str = "\
Usage: cargo xtask <command>

Commands:
  check                          Check docs (line budgets, docs/architecture/ citations and
                                 links, the ADR index, the architecture table of contents) and
                                 the crate tier and dependency rules
  generate-architecture-index    Regenerate the table of contents in docs/architecture/README.md
  new-adr \"<decision>\" --decided-by <who> [--section <docs/architecture/file.md#anchor>]
          [--issue <number>] [--date <YYYY-MM-DD>]
                                 Create the next ADR from the template and add it to the index.
                                 <who> is a name, `human` (git user.name),
                                 `agent, ratified by <name>`, or `agent, unratified`
  precommit                      The commit gate: check, then cargo fmt --check, then clippy
  install-hooks [--force]        Install the git pre-commit hook that runs precommit
";

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<ExitCode> {
    let root = repo::root();
    let Some((command, rest)) = args.split_first() else {
        eprint!("{USAGE}");
        return Ok(ExitCode::FAILURE);
    };
    match command.as_str() {
        "check" => {
            no_arguments(rest)?;
            Ok(exit_code(check(&root)?))
        }
        "generate-architecture-index" => {
            no_arguments(rest)?;
            let changed = arch_index::generate(&root)?;
            println!(
                "{} {}",
                arch_index::README,
                if changed {
                    "updated"
                } else {
                    "already up to date"
                }
            );
            Ok(ExitCode::SUCCESS)
        }
        "new-adr" => {
            let path = adr::create(&root, &parse_new_adr(rest)?)?;
            println!("created {path} and added it to docs/adrs/README.md");
            println!(
                "next: write Context, Decision, Alternatives and Consequences; if you gave no \
                 --section, fill in the Architecture line and the index row; add the decision's \
                 result to that architecture section"
            );
            Ok(ExitCode::SUCCESS)
        }
        "precommit" => {
            no_arguments(rest)?;
            precommit(&root)
        }
        "install-hooks" => {
            let force = match rest {
                [] => false,
                [flag] if flag == "--force" => true,
                _ => bail!("install-hooks takes only --force"),
            };
            let path = hooks::install(&root, force)?;
            println!("installed {}", path.display());
            Ok(ExitCode::SUCCESS)
        }
        "help" | "--help" | "-h" => {
            print!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        other => {
            eprint!("unknown command `{other}`\n\n{USAGE}");
            Ok(ExitCode::FAILURE)
        }
    }
}

fn no_arguments(rest: &[String]) -> Result<()> {
    if let Some(extra) = rest.first() {
        bail!("unexpected argument `{extra}`");
    }
    Ok(())
}

fn exit_code(ok: bool) -> ExitCode {
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn parse_new_adr(args: &[String]) -> Result<adr::NewAdr> {
    let mut options = adr::NewAdr::default();
    let mut decision: Option<String> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let mut value = |flag: &str| {
            iter.next()
                .cloned()
                .with_context(|| format!("{flag} needs a value"))
        };
        match arg.as_str() {
            "--decided-by" => options.decided_by = value(arg)?,
            "--section" => options.section = Some(value(arg)?),
            "--date" => options.date = Some(value(arg)?),
            "--issue" => {
                let raw = value(arg)?;
                let number = raw.trim_start_matches('#');
                options.issue = Some(
                    number
                        .parse()
                        .with_context(|| format!("--issue `{raw}` is not a number"))?,
                );
            }
            flag if flag.starts_with("--") => bail!("unknown option `{flag}`"),
            text if decision.is_none() => decision = Some(text.to_string()),
            text => bail!("unexpected argument `{text}`: quote the decision"),
        }
    }
    options.decision = decision.context("new-adr needs the decision, in quotes")?;
    Ok(options)
}

/// Runs every documentation check and the crate rules check, and prints the problems. Returns whether all passed.
fn check(root: &Path) -> Result<bool> {
    let files = repo::files(root)?;
    let mut problems = Vec::new();
    problems.extend(budget::check(root, &files)?);
    problems.extend(citations::check(root, &files)?);
    problems.extend(adr::check(root, &files)?);
    problems.extend(arch_index::check(root, &files)?);
    problems.extend(crate_rules::check(root, &files)?);
    for problem in &problems {
        eprintln!("{problem}");
    }
    if problems.is_empty() {
        println!("check passed");
    } else {
        eprintln!("check failed: {} problem(s)", problems.len());
    }
    Ok(problems.is_empty())
}

/// The commit gate: the check, then formatting, then clippy. Stops at the first failure.
fn precommit(root: &Path) -> Result<ExitCode> {
    if !check(root)? {
        return Ok(ExitCode::FAILURE);
    }
    let steps: [&[&str]; 2] = [
        &["fmt", "--all", "--check"],
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    ];
    for args in steps {
        let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
            .args(args)
            .current_dir(root)
            .status()
            .with_context(|| format!("running cargo {}", args.join(" ")))?;
        if !status.success() {
            eprintln!("precommit failed at: cargo {}", args.join(" "));
            return Ok(ExitCode::FAILURE);
        }
    }
    println!("precommit passed");
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn test_parse_new_adr_reads_flags_in_any_order() -> Result<(), Box<dyn Error>> {
        let options = parse_new_adr(&strings(&[
            "--decided-by",
            "human",
            "Use iced",
            "--issue",
            "#12",
        ]))?;
        assert_eq!(options.decision, "Use iced");
        assert_eq!(options.decided_by, "human");
        assert_eq!(options.issue, Some(12));
        Ok(())
    }

    #[test]
    fn test_parse_new_adr_rejects_unquoted_decision() {
        assert!(parse_new_adr(&strings(&["Use", "iced"])).is_err());
        assert!(parse_new_adr(&strings(&["--decided-by"])).is_err());
    }
}
