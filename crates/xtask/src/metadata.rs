//! The workspace's crates and their direct dependencies, read from `cargo metadata`.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::json::{self, Value};
use crate::repo;

/// A workspace crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// The package name.
    pub name: String,
    /// The manifest, relative to the repository root.
    pub manifest: String,
    /// The library root, or the binary root if there is no library, relative to the repository
    /// root.
    pub root_source: Option<String>,
    /// Direct dependencies, of every kind.
    pub dependencies: Vec<Dependency>,
}

impl Package {
    /// The crate's directory, relative to the repository root.
    pub fn dir(&self) -> &str {
        self.manifest
            .strip_suffix("Cargo.toml")
            .map_or("", |d| d.trim_end_matches('/'))
    }
}

/// One direct dependency of a [`Package`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    /// The depended-on package's name (not any rename).
    pub name: String,
    /// The section of `Cargo.toml` it is declared in.
    pub kind: DependencyKind,
    /// Whether it comes from a registry or git, rather than a local path.
    pub third_party: bool,
}

/// The `Cargo.toml` section a dependency is declared in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyKind {
    /// `[dependencies]`.
    Normal,
    /// `[dev-dependencies]`.
    Dev,
    /// `[build-dependencies]`.
    Build,
}

/// Runs `cargo metadata` in `root` and returns the workspace's crates.
pub fn read(root: &Path) -> Result<Vec<Package>> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let output = Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
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
    parse(&text, root)
}

/// Parses `cargo metadata --no-deps` output; absolute paths are made relative to `root`.
pub fn parse(text: &str, root: &Path) -> Result<Vec<Package>> {
    let document = json::parse(text).context("parsing cargo metadata")?;
    let packages = document
        .get("packages")
        .and_then(Value::as_array)
        .context("cargo metadata has no `packages` array")?;
    packages.iter().map(|p| package(p, root)).collect()
}

fn package(value: &Value, root: &Path) -> Result<Package> {
    let name = string(value, "name")?;
    let manifest = repo::relative(root, Path::new(string(value, "manifest_path")?));
    let targets = value
        .get("targets")
        .and_then(Value::as_array)
        .unwrap_or(&[]);
    let root_target = |wanted: &[&str]| {
        targets.iter().find(|t| {
            t.get("kind")
                .and_then(Value::as_array)
                .is_some_and(|kinds| {
                    kinds
                        .iter()
                        .any(|k| k.as_str().is_some_and(|k| wanted.contains(&k)))
                })
        })
    };
    let root_source = root_target(&["lib", "proc-macro"])
        .or_else(|| root_target(&["bin"]))
        .and_then(|t| t.get("src_path"))
        .and_then(Value::as_str)
        .map(|p| repo::relative(root, Path::new(p)));
    let dependencies = value
        .get("dependencies")
        .and_then(Value::as_array)
        .unwrap_or(&[])
        .iter()
        .map(dependency)
        .collect::<Result<_>>()
        .with_context(|| format!("reading the dependencies of `{name}`"))?;
    Ok(Package {
        name: name.to_string(),
        manifest,
        root_source,
        dependencies,
    })
}

fn dependency(value: &Value) -> Result<Dependency> {
    let kind = match value.get("kind").and_then(Value::as_str) {
        None => DependencyKind::Normal,
        Some("dev") => DependencyKind::Dev,
        Some("build") => DependencyKind::Build,
        Some(other) => bail!("unknown dependency kind `{other}`"),
    };
    Ok(Dependency {
        name: string(value, "name")?.to_string(),
        kind,
        third_party: value.get("source").and_then(Value::as_str).is_some(),
    })
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
    use std::error::Error;

    const FIXTURE: &str = r#"{"packages": [
      {"name": "dsp", "manifest_path": "/r/crates/dsp/Cargo.toml",
       "targets": [{"kind": ["test"], "src_path": "/r/crates/dsp/tests/a.rs"},
                   {"kind": ["lib"], "src_path": "/r/crates/dsp/src/lib.rs"}],
       "dependencies": [
         {"name": "thiserror", "source": "registry+https://github.com/rust-lang/crates.io-index",
          "kind": null, "rename": null},
         {"name": "audio_buffers", "source": null, "kind": null, "path": "/r/crates/audio_buffers"},
         {"name": "realtime_guard", "source": null, "kind": "dev", "rename": "guard"},
         {"name": "cc", "source": "git+https://example.com/cc", "kind": "build"}]},
      {"name": "debussy", "manifest_path": "/r/crates/debussy/Cargo.toml",
       "targets": [{"kind": ["bin"], "src_path": "/r/crates/debussy/src/main.rs"}],
       "dependencies": []}
    ], "workspace_root": "/r"}"#;

    #[test]
    fn test_parse_reads_packages_and_dependencies() -> Result<(), Box<dyn Error>> {
        let packages = parse(FIXTURE, Path::new("/r"))?;
        assert_eq!(packages.len(), 2);
        let dsp = &packages[0];
        assert_eq!(dsp.manifest, "crates/dsp/Cargo.toml");
        assert_eq!(dsp.dir(), "crates/dsp");
        assert_eq!(dsp.root_source.as_deref(), Some("crates/dsp/src/lib.rs"));
        let kinds: Vec<_> = dsp
            .dependencies
            .iter()
            .map(|d| (d.name.as_str(), d.kind, d.third_party))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("thiserror", DependencyKind::Normal, true),
                ("audio_buffers", DependencyKind::Normal, false),
                ("realtime_guard", DependencyKind::Dev, false),
                ("cc", DependencyKind::Build, true),
            ]
        );
        assert_eq!(
            packages[1].root_source.as_deref(),
            Some("crates/debussy/src/main.rs")
        );
        Ok(())
    }

    #[test]
    fn test_parse_rejects_unknown_dependency_kind() {
        let text = r#"{"packages": [{"name": "a", "manifest_path": "/r/a/Cargo.toml",
            "targets": [], "dependencies": [{"name": "b", "kind": "weird"}]}]}"#;
        assert!(parse(text, Path::new("/r")).is_err());
        assert!(parse("{}", Path::new("/r")).is_err());
    }
}
