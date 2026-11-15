//! `cargo xtask bundle`: assembles `Debussy.app` from the release `debussy` binary, the
//! `Info.plist` template and the licence notices, signs it ad hoc, and with `--zip` zips it for a
//! release. See docs/architecture/platform.md "App bundle" and "Releases".

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// The `Info.plist` template, relative to the repository root.
pub const TEMPLATE: &str = "crates/macos_platform/bundle/Info.plist";
/// The manifest whose `[workspace.package]` version the bundle carries; every crate inherits it.
const MANIFEST: &str = "Cargo.toml";
/// The table in [`MANIFEST`] holding the version.
const VERSION_TABLE: &str = "[workspace.package]";
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

    let version = version(root)?;
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
    // Resources go in before signing: the signature seals them.
    crate::notices::write(root, &app.join("Contents/Resources"))?;

    run(
        Command::new("codesign")
            .args(["--force", "--sign", "-"])
            .arg(&app),
        "codesign",
    )?;
    Ok(app)
}

/// Zips a built `app` beside it as `Debussy-<version>-macos-arm64.zip` with `ditto`, which keeps
/// the signature and extended attributes. Returns the zip's path. Fails on a host that is not
/// Apple silicon, since the name promises arm64.
pub fn zip(root: &Path, app: &Path) -> Result<PathBuf> {
    if std::env::consts::ARCH != "aarch64" {
        bail!(
            "the release zip is for Apple silicon; this host is {}",
            std::env::consts::ARCH
        );
    }
    let dir = app.parent().context("the app has no parent directory")?;
    let zip = dir.join(zip_name(&version(root)?));
    if zip.exists() {
        fs::remove_file(&zip).with_context(|| format!("removing {}", zip.display()))?;
    }
    run(
        Command::new("ditto")
            .args(["-c", "-k", "--sequesterRsrc", "--keepParent"])
            .arg(app)
            .arg(&zip),
        "ditto",
    )?;
    Ok(zip)
}

/// The release zip's file name for `version`.
fn zip_name(version: &str) -> String {
    format!("Debussy-{version}-macos-arm64.zip")
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

/// The workspace version, which the bundle and every crate carry.
pub fn version(root: &Path) -> Result<String> {
    let manifest = fs::read_to_string(root.join(MANIFEST)).context(MANIFEST)?;
    table_version(&manifest, VERSION_TABLE)
}

/// The `version` of the table headed `table` (such as `[workspace.package]`) in a manifest's text.
fn table_version(manifest: &str, table: &str) -> Result<String> {
    let mut in_table = false;
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            in_table = line == table;
        } else if in_table
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
    bail!("{MANIFEST} has no {table} version")
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
    fn test_table_version_reads_the_named_table() -> Result<(), Box<dyn Error>> {
        let manifest = "[workspace]\nmembers = [\"crates/*\"]\n\n\
                        [workspace.package]\nversion = \"1.2.3\"\nedition = \"2024\"\n\n\
                        [workspace.dependencies]\nversion = \"9\"\n";
        assert_eq!(table_version(manifest, VERSION_TABLE)?, "1.2.3");
        Ok(())
    }

    #[test]
    fn test_table_version_rejects_missing_or_odd_versions() {
        let t = VERSION_TABLE;
        assert!(table_version("[workspace.package]\nedition = \"2024\"\n", t).is_err());
        assert!(table_version("[package]\nversion = \"1.0.0\"\n", t).is_err());
        assert!(table_version("[workspace.package]\nversion = \"1.0<x>\"\n", t).is_err());
    }

    #[test]
    fn test_version_reads_the_repository_manifest() -> Result<(), Box<dyn Error>> {
        let version = version(&crate::repo::root())?;
        assert_eq!(version.split('.').count(), 3, "{version}");
        Ok(())
    }

    #[test]
    fn test_zip_name_carries_version_and_platform() {
        assert_eq!(zip_name("0.1.0"), "Debussy-0.1.0-macos-arm64.zip");
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
