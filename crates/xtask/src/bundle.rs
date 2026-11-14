//! `cargo xtask bundle`: assembles `Debussy.app` from the release `debussy` binary and the
//! `Info.plist` template, and signs it ad hoc. See docs/architecture/platform.md "App bundle".

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// The `Info.plist` template, relative to the repository root.
pub const TEMPLATE: &str = "crates/macos_platform/bundle/Info.plist";
/// The manifest whose version the bundle carries.
const MANIFEST: &str = "crates/debussy/Cargo.toml";
/// The placeholder in the template replaced by the version.
const VERSION_PLACEHOLDER: &str = "@VERSION@";

/// Builds `debussy` in release mode and writes `target/release/Debussy.app`. Returns its path.
pub fn build(root: &Path) -> Result<PathBuf> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    run(
        Command::new(cargo)
            .args(["build", "--release", "-p", "debussy"])
            .current_dir(root),
        "cargo build --release -p debussy",
    )?;

    let version = package_version(&fs::read_to_string(root.join(MANIFEST)).context(MANIFEST)?)?;
    let template = fs::read_to_string(root.join(TEMPLATE)).context(TEMPLATE)?;
    let plist = info_plist(&template, &version)?;

    let app = root.join("target/release/Debussy.app");
    if app.exists() {
        fs::remove_dir_all(&app).with_context(|| format!("removing {}", app.display()))?;
    }
    let macos = app.join("Contents/MacOS");
    fs::create_dir_all(&macos).with_context(|| format!("creating {}", macos.display()))?;
    fs::write(app.join("Contents/Info.plist"), plist).context("writing Info.plist")?;
    fs::copy(root.join("target/release/debussy"), macos.join("debussy"))
        .context("copying the debussy binary")?;

    run(
        Command::new("codesign")
            .args(["--force", "--sign", "-"])
            .arg(&app),
        "codesign",
    )?;
    Ok(app)
}

/// Runs a command, failing if it does not succeed.
fn run(command: &mut Command, what: &str) -> Result<()> {
    let status = command
        .status()
        .with_context(|| format!("running {what}"))?;
    if !status.success() {
        bail!("{what} failed");
    }
    Ok(())
}

/// The `version` of the `[package]` table in a manifest's text.
fn package_version(manifest: &str) -> Result<String> {
    let mut in_package = false;
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            in_package = line == "[package]";
        } else if in_package
            && let Some(rest) = line.strip_prefix("version")
            && let Some(value) = rest.trim_start().strip_prefix('=')
        {
            let value = value.trim().trim_matches('"');
            if !value.is_empty() && value.chars().all(|c| c.is_ascii_digit() || c == '.') {
                return Ok(value.to_string());
            }
            bail!("{MANIFEST}: version `{value}` is not dotted numbers");
        }
    }
    bail!("{MANIFEST} has no [package] version")
}

/// The template with its version placeholder replaced.
fn info_plist(template: &str, version: &str) -> Result<String> {
    if !template.contains(VERSION_PLACEHOLDER) {
        bail!("{TEMPLATE} has no {VERSION_PLACEHOLDER}");
    }
    Ok(template.replace(VERSION_PLACEHOLDER, version))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn test_package_version_reads_the_package_table() -> Result<(), Box<dyn Error>> {
        let manifest = "[package]\nname = \"debussy\"\nversion = \"1.2.3\"\n\n\
                        [dependencies]\nversion = \"9\"\n";
        assert_eq!(package_version(manifest)?, "1.2.3");
        Ok(())
    }

    #[test]
    fn test_package_version_rejects_missing_or_odd_versions() {
        assert!(package_version("[package]\nname = \"x\"\n").is_err());
        assert!(package_version("[package]\nversion.workspace = true\n").is_err());
        assert!(package_version("[package]\nversion = \"1.0<x>\"\n").is_err());
    }

    #[test]
    fn test_info_plist_substitutes_the_version() -> Result<(), Box<dyn Error>> {
        let plist = info_plist("<string>@VERSION@</string>", "0.4.0")?;
        assert_eq!(plist, "<string>0.4.0</string>");
        assert!(info_plist("<string/>", "0.4.0").is_err());
        Ok(())
    }

    #[test]
    fn test_template_declares_the_project_package_type() -> Result<(), Box<dyn Error>> {
        let template = fs::read_to_string(crate::repo::root().join(TEMPLATE))?;
        let plist = info_plist(&template, "1.0.0")?;
        assert!(!plist.contains(VERSION_PLACEHOLDER));
        for needle in [
            "<string>io.github.cparo.debussy</string>",
            "<string>io.github.cparo.debussy.project</string>",
            "<string>com.apple.package</string>",
            "<string>debussy</string>",
            "UTExportedTypeDeclarations",
            "CFBundleDocumentTypes",
            "LSTypeIsPackage",
            "NSMicrophoneUsageDescription",
        ] {
            assert!(plist.contains(needle), "Info.plist lacks {needle}");
        }
        Ok(())
    }
}
