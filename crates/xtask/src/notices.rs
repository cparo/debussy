//! The third-party licence notices shipped inside `Debussy.app`: the licence files of every crate
//! linked into the `debussy` binary for Apple silicon, copied verbatim, or, for a crate that ships
//! none, the standard text of a licence its expression offers. See
//! docs/architecture/platform.md "Releases".

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::json::{self, Value};

/// The package whose shipped dependencies the notices cover.
const ROOT_PACKAGE: &str = "debussy";
/// The target the release is built for.
const TARGET: &str = "aarch64-apple-darwin";
/// The notices file's name inside `Contents/Resources`.
pub const NOTICES_FILE: &str = "THIRD-PARTY-NOTICES.txt";
/// Debussy's own licence files, at the repository root, copied beside the notices.
pub const OWN_LICENSES: &[&str] = &["LICENSE-MIT", "LICENSE-APACHE"];
/// File-name prefixes (compared case-insensitively) that mark a licence file.
const LICENSE_PREFIXES: &[&str] = &["license", "licence", "copying", "notice", "copyright"];
/// Crates that ship no licence file and need no licence text, by name, with the reason. Such a
/// crate gets a header and the reason in the notices, but no text.
const OVERRIDES: &[(&str, &str)] = &[(
    "hexf-parse",
    "CC0-1.0: a public-domain dedication, which requires no notice",
)];
/// The line of [`OWN_LICENSES`]' MIT text that names the copyright holders.
const MIT_COPYRIGHT_PREFIX: &str = "Copyright (c)";

/// One third-party crate whose licence the release must carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// The crate name.
    pub name: String,
    /// The crate version.
    pub version: String,
    /// The SPDX licence expression from its manifest, if it declares one.
    pub license: Option<String>,
    /// The manifest's authors, without e-mail addresses.
    pub authors: Vec<String>,
    /// The crate's source directory.
    pub dir: PathBuf,
    /// The manifest's `license-file`, if set.
    pub license_file: Option<PathBuf>,
}

/// The standard licence texts used for a crate that ships no licence file.
#[derive(Debug, Clone)]
pub struct StandardTexts {
    /// The MIT licence, with its copyright line (starting [`MIT_COPYRIGHT_PREFIX`]) to replace.
    pub mit: String,
    /// The Apache License 2.0, which names no copyright holder.
    pub apache: String,
}

impl StandardTexts {
    /// Debussy's own licence files at `root`, which are the standard texts.
    fn read(root: &Path) -> Result<Self> {
        let read =
            |name: &str| fs::read_to_string(root.join(name)).with_context(|| name.to_string());
        Ok(Self {
            mit: read("LICENSE-MIT")?,
            apache: read("LICENSE-APACHE")?,
        })
    }

    /// The text of a licence `expression` offers, for a crate by `authors`: MIT if offered, else
    /// Apache-2.0. `None` if it offers neither or requires more than one licence (`AND`, `WITH`).
    fn for_expression(&self, expression: &str, authors: &[String]) -> Result<Option<String>> {
        let tokens: Vec<&str> = expression
            .split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | '/'))
            .filter(|t| !t.is_empty())
            .collect();
        if tokens.iter().any(|t| matches!(*t, "AND" | "WITH")) {
            return Ok(None);
        }
        if tokens.contains(&"MIT") {
            let holders = if authors.is_empty() {
                "the authors".to_string()
            } else {
                authors.join(", ")
            };
            let mut found = false;
            let text = self
                .mit
                .lines()
                .map(|line| {
                    if line.starts_with(MIT_COPYRIGHT_PREFIX) {
                        found = true;
                        format!("{MIT_COPYRIGHT_PREFIX} {holders}")
                    } else {
                        line.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            if !found {
                bail!("the MIT text has no `{MIT_COPYRIGHT_PREFIX}` line");
            }
            return Ok(Some(text));
        }
        Ok(tokens.contains(&"Apache-2.0").then(|| self.apache.clone()))
    }
}

/// Runs `cargo metadata` in `root` and writes the notices and Debussy's own licence files into
/// `resources`.
pub fn write(root: &Path, resources: &Path) -> Result<()> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let output = Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--filter-platform",
            TARGET,
            "--offline",
        ])
        .current_dir(root)
        .output()
        .context("running cargo metadata")?;
    if !output.status.success() {
        bail!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let text = String::from_utf8(output.stdout).context("cargo metadata output is not UTF-8")?;
    let notices = render(&collect(&text)?, &StandardTexts::read(root)?)?;
    fs::create_dir_all(resources).with_context(|| format!("creating {}", resources.display()))?;
    fs::write(resources.join(NOTICES_FILE), notices).context("writing the notices")?;
    for name in OWN_LICENSES {
        fs::copy(root.join(name), resources.join(name))
            .with_context(|| format!("copying {name}"))?;
    }
    Ok(())
}

/// The third-party crates `debussy` links, from `cargo metadata` output (with dependencies):
/// those reached from it along normal dependency edges only, leaving out build and dev
/// dependencies and workspace members. Sorted by name, then version.
pub fn collect(text: &str) -> Result<Vec<Notice>> {
    let document = json::parse(text).context("parsing cargo metadata")?;
    let members: BTreeSet<&str> = array(&document, "workspace_members")?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let packages: HashMap<&str, &Value> = array(&document, "packages")?
        .iter()
        .map(|p| Ok((string(p, "id")?, p)))
        .collect::<Result<_>>()?;
    let resolve = document
        .get("resolve")
        .context("cargo metadata has no `resolve`: run it with dependencies")?;
    let nodes: HashMap<&str, &Value> = array(resolve, "nodes")?
        .iter()
        .map(|n| Ok((string(n, "id")?, n)))
        .collect::<Result<_>>()?;

    let root = members
        .iter()
        .copied()
        .find(|id| {
            packages
                .get(id)
                .and_then(|p| p.get("name"))
                .and_then(Value::as_str)
                == Some(ROOT_PACKAGE)
        })
        .with_context(|| format!("no workspace member `{ROOT_PACKAGE}`"))?;

    let mut seen = BTreeSet::from([root]);
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let node = nodes
            .get(id)
            .with_context(|| format!("no resolve node for {id}"))?;
        for dep in node.get("deps").and_then(Value::as_array).unwrap_or(&[]) {
            let normal = dep
                .get("dep_kinds")
                .and_then(Value::as_array)
                .unwrap_or(&[])
                .iter()
                .any(|k| matches!(k.get("kind"), Some(Value::Null) | None));
            let pkg = string(dep, "pkg")?;
            if normal && seen.insert(pkg) {
                stack.push(pkg);
            }
        }
    }

    let mut notices = seen
        .into_iter()
        .filter(|id| !members.contains(id))
        .map(|id| {
            let package = packages
                .get(id)
                .with_context(|| format!("no package for {id}"))?;
            let manifest = Path::new(string(package, "manifest_path")?);
            let dir = manifest.parent().unwrap_or(Path::new("")).to_path_buf();
            let optional = |key| package.get(key).and_then(Value::as_str);
            Ok(Notice {
                name: string(package, "name")?.to_string(),
                version: string(package, "version")?.to_string(),
                license: optional("license").map(str::to_string),
                authors: package
                    .get("authors")
                    .and_then(Value::as_array)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|a| a.split(" <").next().unwrap_or(a).trim().to_string())
                    .collect(),
                license_file: optional("license_file").map(|f| dir.join(f)),
                dir,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    notices.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
    Ok(notices)
}

/// The notices text: per crate, a header line and the verbatim contents of its licence files, or
/// if it has none, the standard text of a licence its expression offers. Fails, naming them, if
/// any crate not in [`OVERRIDES`] has neither.
pub fn render(notices: &[Notice], standard: &StandardTexts) -> Result<String> {
    let mut out = String::from(
        "Debussy includes the following third-party software. Each crate's licence files are \
         reproduced below as the crate ships them.\n",
    );
    let mut missing = Vec::new();
    for notice in notices {
        let files = license_files(notice)?;
        let overridden = OVERRIDES.iter().find(|(name, _)| *name == notice.name);
        out.push_str(&format!(
            "\n{}\n{} {} — {}\n",
            "=".repeat(78),
            notice.name,
            notice.version,
            notice.license.as_deref().unwrap_or("no licence expression")
        ));
        if let Some((_, reason)) = overridden {
            out.push_str(&format!("({reason})\n"));
        }
        if files.is_empty() && overridden.is_none() {
            let text = match &notice.license {
                Some(expression) => standard.for_expression(expression, &notice.authors)?,
                None => None,
            };
            match text {
                Some(text) => out.push_str(&format!(
                    "\n--- standard licence text (the crate ships no licence file) ---\n\n{}\n",
                    text.trim_end()
                )),
                None => missing.push(format!("{} {}", notice.name, notice.version)),
            }
        }
        for file in files {
            let text =
                fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
            let name = file.file_name().unwrap_or_default().to_string_lossy();
            out.push_str(&format!("\n--- {name} ---\n\n{}\n", text.trim_end()));
        }
    }
    if !missing.is_empty() {
        bail!(
            "no licence file or standard licence text for: {}. Add each to OVERRIDES in \
             crates/xtask/src/notices.rs only if its licence needs no notice",
            missing.join(", ")
        );
    }
    Ok(out)
}

/// A crate's licence files: the files in its directory whose names start with a licence prefix,
/// plus its `license-file`, sorted and without duplicates.
fn license_files(notice: &Notice) -> Result<Vec<PathBuf>> {
    let mut files = BTreeSet::new();
    let entries =
        fs::read_dir(&notice.dir).with_context(|| format!("listing {}", notice.dir.display()))?;
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if entry.file_type()?.is_file() && LICENSE_PREFIXES.iter().any(|p| name.starts_with(p)) {
            files.insert(entry.path());
        }
    }
    if let Some(file) = &notice.license_file {
        files.insert(file.clone());
    }
    Ok(files.into_iter().collect())
}

fn array<'a>(value: &'a Value, key: &str) -> Result<&'a [Value]> {
    value
        .get(key)
        .and_then(Value::as_array)
        .with_context(|| format!("cargo metadata has no `{key}` array"))
}

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("missing string `{key}`"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::testing::TempTree;
    use std::error::Error;

    /// `debussy` depends on workspace `dsp` and on `a`; `a` on `b`. `dsp` has build-only `cc`,
    /// whose normal dependency `shlex` must not ship either; `debussy` has dev-only `tempfile`.
    const FIXTURE: &str = r#"{
      "workspace_members": ["path+file:///r/crates/debussy#0.1.0", "path+file:///r/crates/dsp#0.1.0"],
      "packages": [
        {"name": "debussy", "version": "0.1.0", "id": "path+file:///r/crates/debussy#0.1.0",
         "license": "MIT OR Apache-2.0", "license_file": null, "manifest_path": "/r/crates/debussy/Cargo.toml"},
        {"name": "dsp", "version": "0.1.0", "id": "path+file:///r/crates/dsp#0.1.0",
         "license": "MIT OR Apache-2.0", "license_file": null, "manifest_path": "/r/crates/dsp/Cargo.toml"},
        {"name": "b", "version": "2.0.0", "id": "reg#b@2.0.0", "license": null,
         "license_file": "LICENSE.txt", "manifest_path": "/reg/b-2.0.0/Cargo.toml"},
        {"name": "a", "version": "1.0.0", "id": "reg#a@1.0.0", "license": "MIT",
         "license_file": null, "manifest_path": "/reg/a-1.0.0/Cargo.toml"},
        {"name": "cc", "version": "1.0.0", "id": "reg#cc@1.0.0", "license": "MIT",
         "license_file": null, "manifest_path": "/reg/cc-1.0.0/Cargo.toml"},
        {"name": "shlex", "version": "1.0.0", "id": "reg#shlex@1.0.0", "license": "MIT",
         "license_file": null, "manifest_path": "/reg/shlex-1.0.0/Cargo.toml"},
        {"name": "tempfile", "version": "3.0.0", "id": "reg#tempfile@3.0.0", "license": "MIT",
         "license_file": null, "manifest_path": "/reg/tempfile-3.0.0/Cargo.toml"}
      ],
      "resolve": {"root": null, "nodes": [
        {"id": "path+file:///r/crates/debussy#0.1.0", "deps": [
          {"name": "dsp", "pkg": "path+file:///r/crates/dsp#0.1.0", "dep_kinds": [{"kind": null, "target": null}]},
          {"name": "a", "pkg": "reg#a@1.0.0", "dep_kinds": [{"kind": null, "target": null}]},
          {"name": "tempfile", "pkg": "reg#tempfile@3.0.0", "dep_kinds": [{"kind": "dev", "target": null}]}]},
        {"id": "path+file:///r/crates/dsp#0.1.0", "deps": [
          {"name": "cc", "pkg": "reg#cc@1.0.0", "dep_kinds": [{"kind": "build", "target": null}]}]},
        {"id": "reg#a@1.0.0", "deps": [
          {"name": "b", "pkg": "reg#b@2.0.0", "dep_kinds": [{"kind": "build", "target": null}, {"kind": null, "target": null}]}]},
        {"id": "reg#b@2.0.0", "deps": []},
        {"id": "reg#cc@1.0.0", "deps": [
          {"name": "shlex", "pkg": "reg#shlex@1.0.0", "dep_kinds": [{"kind": null, "target": null}]}]},
        {"id": "reg#shlex@1.0.0", "deps": []},
        {"id": "reg#tempfile@3.0.0", "deps": []}
      ]}
    }"#;

    #[test]
    fn test_collect_follows_normal_edges_only() -> Result<(), Box<dyn Error>> {
        let notices = collect(FIXTURE)?;
        let names: Vec<_> = notices.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["a", "b"]);
        assert_eq!(notices[0].dir, Path::new("/reg/a-1.0.0"));
        assert_eq!(notices[0].license.as_deref(), Some("MIT"));
        assert_eq!(
            notices[1].license_file.as_deref(),
            Some(Path::new("/reg/b-2.0.0/LICENSE.txt"))
        );
        Ok(())
    }

    #[test]
    fn test_collect_needs_resolve_and_the_root_package() {
        assert!(collect(r#"{"workspace_members": [], "packages": []}"#).is_err());
        let no_root = r#"{"workspace_members": [], "packages": [], "resolve": {"nodes": []}}"#;
        assert!(collect(no_root).is_err());
    }

    fn notice(tree: &TempTree, name: &str, license_file: Option<&str>) -> Notice {
        let dir = tree.path().join(name);
        Notice {
            name: name.into(),
            version: "1.0.0".into(),
            license: Some("MIT OR Apache-2.0".into()),
            authors: vec!["Ada".into()],
            license_file: license_file.map(|f| dir.join(f)),
            dir,
        }
    }

    fn standard() -> StandardTexts {
        StandardTexts {
            mit: "MIT License\n\nCopyright (c) 2026 Debussy\n\nPermission is granted.\n".into(),
            apache: "Apache License\n".into(),
        }
    }

    #[test]
    fn test_render_copies_licence_files_verbatim() -> Result<(), Box<dyn Error>> {
        let tree = TempTree::new()?;
        tree.write("a/LICENSE-MIT", "MIT text\n")?;
        tree.write("a/license-apache.md", "Apache text\n")?;
        tree.write("a/README.md", "not a licence\n")?;
        tree.write("b/Cargo.toml", "")?;
        tree.write("b/legal/TERMS", "custom terms\n")?;
        let text = render(
            &[
                notice(&tree, "a", None),
                notice(&tree, "b", Some("legal/TERMS")),
            ],
            &standard(),
        )?;
        assert!(text.contains("a 1.0.0 — MIT OR Apache-2.0"));
        assert!(text.contains("--- LICENSE-MIT ---\n\nMIT text\n"));
        assert!(text.contains("Apache text"));
        assert!(!text.contains("not a licence"));
        assert!(text.contains("--- TERMS ---\n\ncustom terms\n"));
        Ok(())
    }

    #[test]
    fn test_render_falls_back_to_a_standard_text() -> Result<(), Box<dyn Error>> {
        let tree = TempTree::new()?;
        tree.write("bare/Cargo.toml", "")?;
        let text = render(&[notice(&tree, "bare", None)], &standard())?;
        assert!(text.contains("ships no licence file"), "{text}");
        assert!(
            text.contains("Copyright (c) Ada\n\nPermission is granted."),
            "{text}"
        );
        assert!(!text.contains("Debussy\n\nPermission"), "{text}");
        Ok(())
    }

    #[test]
    fn test_for_expression_picks_mit_then_apache() -> Result<(), Box<dyn Error>> {
        let texts = standard();
        let authors = ["Ada".to_string(), "Grace".to_string()];
        let pick = |e| texts.for_expression(e, &authors);
        assert!(pick("Zlib OR Apache-2.0 OR MIT")?.is_some_and(|t| t.contains("(c) Ada, Grace")));
        assert!(pick("MIT/Apache-2.0")?.is_some_and(|t| t.starts_with("MIT")));
        assert_eq!(pick("Apache-2.0")?.as_deref(), Some("Apache License\n"));
        assert_eq!(pick("(MIT OR Apache-2.0) AND Unicode-3.0")?, None);
        assert_eq!(pick("Apache-2.0 WITH LLVM-exception")?, None);
        assert_eq!(pick("BSD-3-Clause")?, None);
        Ok(())
    }

    #[test]
    fn test_render_fails_on_a_crate_without_any_licence_text() -> Result<(), Box<dyn Error>> {
        let tree = TempTree::new()?;
        tree.write("a/LICENSE", "text\n")?;
        tree.write("bare/Cargo.toml", "")?;
        let mut bare = notice(&tree, "bare", None);
        bare.license = Some("BSD-3-Clause".into());
        let result = render(&[notice(&tree, "a", None), bare], &standard());
        let message = format!("{:#}", result.err().ok_or("render accepted `bare`")?);
        assert!(message.contains("bare 1.0.0"), "{message}");
        assert!(!message.contains("a 1.0.0"), "{message}");
        Ok(())
    }
}
