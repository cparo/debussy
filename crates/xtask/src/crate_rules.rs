//! The crate tiers and dependency rules of `docs/architecture/crates.md` "Dependency rules",
//! checked against `cargo metadata`. A new crate gets a row in [`CRATES`] when it is created.

use std::path::Path;

use anyhow::Result;

use crate::metadata::{self, DependencyKind, Package};
use crate::repo::Problem;

/// The kind of a workspace crate, as in the crate map of `docs/architecture/crates.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    RealtimeCore,
    Core,
    Io,
    Platform,
    Ui,
    Binary,
    TestSupport,
    Tooling,
}

impl Kind {
    /// The position in the tier order, earliest first; `None` for kinds outside it.
    fn tier(self) -> Option<u8> {
        match self {
            Kind::RealtimeCore => Some(0),
            Kind::Core => Some(1),
            Kind::Io => Some(2),
            Kind::Platform | Kind::Ui => Some(3),
            Kind::Binary => Some(4),
            Kind::TestSupport | Kind::Tooling => None,
        }
    }

    fn describe(self) -> &'static str {
        match self {
            Kind::RealtimeCore => "real-time core",
            Kind::Core => "core",
            Kind::Io => "I/O",
            Kind::Platform => "platform",
            Kind::Ui => "UI",
            Kind::Binary => "binary",
            Kind::TestSupport => "test support",
            Kind::Tooling => "tooling",
        }
    }
}

/// Every workspace crate and its kind, as in the crate map.
const CRATES: &[(&str, Kind)] = &[
    ("audio_buffers", Kind::RealtimeCore),
    ("dsp", Kind::RealtimeCore),
    ("modulation", Kind::RealtimeCore),
    ("audio_effects", Kind::RealtimeCore),
    ("signal_graph", Kind::RealtimeCore),
    ("audio_engine", Kind::RealtimeCore),
    ("audio_analysis", Kind::Core),
    ("engine_bridge", Kind::Core),
    ("project_model", Kind::Core),
    ("wav_files", Kind::Core),
    ("platform_interfaces", Kind::Core),
    ("disk_streaming", Kind::Io),
    ("project_storage", Kind::Io),
    ("import_export", Kind::Io),
    ("macos_platform", Kind::Platform),
    ("debussy_ui", Kind::Ui),
    ("debussy", Kind::Binary),
    ("realtime_guard", Kind::TestSupport),
    ("xtask", Kind::Tooling),
];

/// The only crate that may hold platform-specific code.
const PLATFORM_CRATE: &str = "macos_platform";
/// The only crate that may depend on [`PLATFORM_CRATE`].
const BINARY_CRATE: &str = "debussy";
/// The UI crate, which must not depend on I/O or platform crates.
const UI_CRATE: &str = "debussy_ui";
/// The test-support crate, usable only as a dev-dependency.
const GUARD_CRATE: &str = "realtime_guard";
/// Crates allowed to contain `unsafe` code.
const UNSAFE_ALLOWED: &[&str] = &[PLATFORM_CRATE, GUARD_CRATE];
/// The one third-party crate a real-time core crate may depend on.
const REALTIME_THIRD_PARTY: &str = "thiserror";
/// Name prefixes of third-party crates that only [`PLATFORM_CRATE`] may depend on.
const PLATFORM_ONLY_PREFIXES: &[&str] = &["cpal", "objc2"];
/// The attribute every crate outside [`UNSAFE_ALLOWED`] and [`SCOPED_UNSAFE`] must carry in its
/// root.
const FORBID_UNSAFE: &str = "#![forbid(unsafe_code)]";
/// The attribute a crate in [`SCOPED_UNSAFE`] carries in its root instead.
const DENY_UNSAFE: &str = "#![deny(unsafe_code)]";
/// The attribute that opts a module back in to `unsafe`.
const ALLOW_UNSAFE: &str = "#![allow(unsafe_code)]";
/// Crates allowed `unsafe` in one module only, and that module's file (ADR-0028).
const SCOPED_UNSAFE: &[(&str, &str)] = &[("audio_buffers", "src/queue.rs")];

fn kind_of(name: &str) -> Option<Kind> {
    CRATES.iter().find(|(n, _)| *n == name).map(|(_, k)| *k)
}

/// Reports every breach of the crate rules in the workspace at `root`.
pub fn check(root: &Path, files: &[String]) -> Result<Vec<Problem>> {
    let packages = metadata::read(root)?;
    check_packages(&packages, files, &|rel| crate::repo::read(root, rel))
}

/// Reports every breach of the crate rules by `packages`. `files` lists the repository's files
/// and `read` returns one of them.
fn check_packages(
    packages: &[Package],
    files: &[String],
    read: &dyn Fn(&str) -> Result<String>,
) -> Result<Vec<Problem>> {
    let mut problems = Vec::new();
    let mut problems_scoped = Vec::new();
    for package in packages {
        let mut report = |message: String| {
            problems.push(Problem::in_file(&package.manifest, message));
        };
        let Some(kind) = kind_of(&package.name) else {
            report(format!(
                "crate `{}` is not in the crate table in crates/xtask/src/crate_rules.rs: \
                 add it with its kind from docs/architecture/crates.md \"Crate map\"",
                package.name
            ));
            continue;
        };

        if kind == Kind::RealtimeCore {
            let clippy = format!("{}/clippy.toml", package.dir());
            if !files.contains(&clippy) {
                report(format!(
                    "real-time core crate has no {clippy}: see docs/architecture/engine.md \
                     \"Real-time thread\""
                ));
            }
        }

        let scoped = SCOPED_UNSAFE.iter().find(|(name, _)| *name == package.name);
        if !UNSAFE_ALLOWED.contains(&package.name.as_str()) {
            // A scoped crate may also forbid `unsafe` outright, which is stricter.
            let accepted: &[&str] = if scoped.is_some() {
                &[DENY_UNSAFE, FORBID_UNSAFE]
            } else {
                &[FORBID_UNSAFE]
            };
            let required = accepted[0];
            let has_attribute = match &package.root_source {
                Some(source) => read(source)?.lines().any(|l| accepted.contains(&l.trim())),
                None => false,
            };
            if !has_attribute {
                report(format!(
                    "crate root must contain `{required}`: only {} may contain unsafe code \
                     (docs/architecture/crates.md \"Dependency rules\")",
                    UNSAFE_ALLOWED.join(" and ")
                ));
            }
        }
        if let Some((_, module)) = scoped {
            let allowed = format!("{}/{module}", package.dir());
            let prefix = format!("{}/", package.dir());
            for file in files {
                if file.starts_with(&prefix) && file.ends_with(".rs") && *file != allowed {
                    let opts_in = read(file)?.lines().any(|l| l.trim() == ALLOW_UNSAFE);
                    if opts_in {
                        problems_scoped.push(Problem::in_file(
                            file,
                            format!("only {allowed} may allow unsafe code in this crate"),
                        ));
                    }
                }
            }
        }

        for dep in &package.dependencies {
            if dep.third_party {
                if kind == Kind::RealtimeCore && dep.name != REALTIME_THIRD_PARTY {
                    report(format!(
                        "real-time core crate depends on third-party `{}`: only \
                         `{REALTIME_THIRD_PARTY}` is allowed",
                        dep.name
                    ));
                }
                let platform_only = PLATFORM_ONLY_PREFIXES
                    .iter()
                    .any(|p| dep.name.starts_with(p));
                if platform_only && package.name != PLATFORM_CRATE {
                    report(format!(
                        "depends on `{}`: only `{PLATFORM_CRATE}` may depend on platform crates",
                        dep.name
                    ));
                }
                continue;
            }
            let Some(dep_kind) = kind_of(&dep.name) else {
                // Reported as an unlisted crate when that crate itself is checked.
                continue;
            };
            if dep.name == GUARD_CRATE {
                if dep.kind != DependencyKind::Dev {
                    report(format!(
                        "`{GUARD_CRATE}` may be used only in [dev-dependencies]"
                    ));
                }
                continue;
            }
            if let Some(tier) = kind.tier() {
                match dep_kind.tier() {
                    Some(dep_tier) if dep_tier <= tier => {}
                    _ => report(format!(
                        "{} crate depends on {} crate `{}`: a crate may depend only on its own \
                         tier or an earlier one",
                        kind.describe(),
                        dep_kind.describe(),
                        dep.name
                    )),
                }
            }
            if dep.name == PLATFORM_CRATE && package.name != BINARY_CRATE {
                report(format!(
                    "depends on `{PLATFORM_CRATE}`: only `{BINARY_CRATE}` may"
                ));
            }
            if package.name == UI_CRATE && matches!(dep_kind, Kind::Io | Kind::Platform) {
                report(format!(
                    "`{UI_CRATE}` depends on {} crate `{}`: it must reach I/O and platform \
                     crates through interfaces `{BINARY_CRATE}` supplies",
                    dep_kind.describe(),
                    dep.name
                ));
            }
        }
    }
    problems.extend(problems_scoped);
    Ok(problems)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::Dependency;
    use std::error::Error;

    fn dep(name: &str, kind: DependencyKind, third_party: bool) -> Dependency {
        Dependency {
            name: name.into(),
            kind,
            third_party,
        }
    }

    fn workspace(name: &str, kind: DependencyKind) -> Dependency {
        dep(name, kind, false)
    }

    fn registry(name: &str) -> Dependency {
        dep(name, DependencyKind::Normal, true)
    }

    fn package(name: &str, dependencies: Vec<Dependency>) -> Package {
        Package {
            name: name.into(),
            manifest: format!("crates/{name}/Cargo.toml"),
            root_source: Some(format!("crates/{name}/src/lib.rs")),
            dependencies,
        }
    }

    /// Checks `packages` in a repository where every real-time core crate has a `clippy.toml`
    /// and every crate root forbids `unsafe`; returns the problem messages.
    fn messages(packages: &[Package]) -> Result<Vec<String>, Box<dyn Error>> {
        let files: Vec<String> = packages
            .iter()
            .map(|p| format!("{}/clippy.toml", p.dir()))
            .collect();
        let read = |_: &str| Ok(format!("//! Docs.\n\n{FORBID_UNSAFE}\n"));
        Ok(check_packages(packages, &files, &read)?
            .into_iter()
            .map(|p| p.message)
            .collect())
    }

    fn assert_one(packages: &[Package], needle: &str) -> Result<(), Box<dyn Error>> {
        let found = messages(packages)?;
        if found.len() == 1 && found[0].contains(needle) {
            Ok(())
        } else {
            Err(format!("expected one problem containing {needle:?}, got {found:?}").into())
        }
    }

    #[test]
    fn test_check_packages_accepts_valid_workspace() -> Result<(), Box<dyn Error>> {
        let found = messages(&[
            package(
                "dsp",
                vec![
                    registry("thiserror"),
                    workspace("audio_buffers", DependencyKind::Normal),
                ],
            ),
            package(
                "audio_buffers",
                vec![workspace(GUARD_CRATE, DependencyKind::Dev)],
            ),
            package("wav_files", vec![workspace("dsp", DependencyKind::Normal)]),
            package(
                PLATFORM_CRATE,
                vec![registry("cpal"), registry("objc2-audio-toolbox")],
            ),
            package(
                BINARY_CRATE,
                vec![
                    workspace(PLATFORM_CRATE, DependencyKind::Normal),
                    workspace(UI_CRATE, DependencyKind::Normal),
                ],
            ),
            package(
                UI_CRATE,
                vec![
                    workspace("project_model", DependencyKind::Normal),
                    registry("iced"),
                ],
            ),
            package("xtask", vec![registry("anyhow")]),
        ])?;
        assert_eq!(found, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn test_check_packages_reports_later_tier_dependency() -> Result<(), Box<dyn Error>> {
        assert_one(
            &[package(
                "dsp",
                vec![workspace("wav_files", DependencyKind::Normal)],
            )],
            "real-time core crate depends on core crate `wav_files`",
        )?;
        // Dev-dependencies follow the tier order too.
        assert_one(
            &[package(
                "project_model",
                vec![workspace("project_storage", DependencyKind::Dev)],
            )],
            "depends on I/O crate `project_storage`",
        )
    }

    #[test]
    fn test_check_packages_reports_dependency_on_tooling() -> Result<(), Box<dyn Error>> {
        assert_one(
            &[package(
                BINARY_CRATE,
                vec![workspace("xtask", DependencyKind::Build)],
            )],
            "depends on tooling crate `xtask`",
        )
    }

    #[test]
    fn test_check_packages_reports_realtime_third_party() -> Result<(), Box<dyn Error>> {
        assert_one(
            &[package(
                "dsp",
                vec![registry("thiserror"), registry("rustfft")],
            )],
            "third-party `rustfft`",
        )
    }

    #[test]
    fn test_check_packages_reports_platform_crate_elsewhere() -> Result<(), Box<dyn Error>> {
        assert_one(
            &[package("import_export", vec![registry("cpal")])],
            "`cpal`",
        )?;
        assert_one(
            &[package(UI_CRATE, vec![registry("objc2-foundation")])],
            "`objc2-foundation`",
        )
    }

    #[test]
    fn test_check_packages_reports_guard_outside_dev() -> Result<(), Box<dyn Error>> {
        assert_one(
            &[package(
                "dsp",
                vec![workspace(GUARD_CRATE, DependencyKind::Normal)],
            )],
            "only in [dev-dependencies]",
        )?;
        assert_one(
            &[package(
                "dsp",
                vec![workspace(GUARD_CRATE, DependencyKind::Build)],
            )],
            "only in [dev-dependencies]",
        )
    }

    #[test]
    fn test_check_packages_reports_unlisted_crate() -> Result<(), Box<dyn Error>> {
        assert_one(
            &[package("mystery", vec![])],
            "`mystery` is not in the crate table",
        )
    }

    #[test]
    fn test_check_packages_reports_platform_and_ui_rules() -> Result<(), Box<dyn Error>> {
        // Platform and UI share a tier, so only the two specific rules catch this edge.
        let found = messages(&[package(
            UI_CRATE,
            vec![workspace(PLATFORM_CRATE, DependencyKind::Normal)],
        )])?;
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found[0].contains("only `debussy` may"), "{found:?}");
        assert!(
            found[1].contains("`debussy_ui` depends on platform crate"),
            "{found:?}"
        );
        assert_one(
            &[package(
                UI_CRATE,
                vec![workspace("project_storage", DependencyKind::Normal)],
            )],
            "`debussy_ui` depends on I/O crate `project_storage`",
        )
    }

    #[test]
    fn test_check_packages_reports_missing_clippy_toml() -> Result<(), Box<dyn Error>> {
        let packages = [package("dsp", vec![]), package("wav_files", vec![])];
        let read = |_: &str| Ok(FORBID_UNSAFE.to_string());
        let found = check_packages(&packages, &[], &read)?;
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("crates/dsp/clippy.toml"));
        Ok(())
    }

    #[test]
    fn test_check_packages_reports_missing_forbid_unsafe() -> Result<(), Box<dyn Error>> {
        let packages = [
            package("wav_files", vec![]),
            package(GUARD_CRATE, vec![]),
            package(PLATFORM_CRATE, vec![]),
        ];
        let read = |_: &str| {
            Ok("#![deny(unsafe_code)]\n// #![forbid(unsafe_code)] in a comment\n".to_string())
        };
        let found = check_packages(&packages, &[], &read)?;
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].path, "crates/wav_files/Cargo.toml");
        Ok(())
    }

    #[test]
    fn test_check_packages_scoped_unsafe_needs_deny_in_root() -> Result<(), Box<dyn Error>> {
        let packages = [package("audio_buffers", vec![])];
        let files = vec!["crates/audio_buffers/clippy.toml".to_string()];
        let neither = |_: &str| Ok("//! No attribute.\n".to_string());
        let found = check_packages(&packages, &files, &neither)?;
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains(DENY_UNSAFE));
        for attribute in [DENY_UNSAFE, FORBID_UNSAFE] {
            let read = |_: &str| Ok(attribute.to_string());
            assert_eq!(check_packages(&packages, &files, &read)?, vec![]);
        }
        Ok(())
    }

    #[test]
    fn test_check_packages_scoped_unsafe_only_in_its_module() -> Result<(), Box<dyn Error>> {
        let packages = [package("audio_buffers", vec![])];
        let files: Vec<String> = ["clippy.toml", "src/lib.rs", "src/queue.rs", "src/pool.rs"]
            .iter()
            .map(|f| format!("crates/audio_buffers/{f}"))
            .collect();
        let read = |path: &str| {
            Ok(match path {
                "crates/audio_buffers/src/lib.rs" => DENY_UNSAFE.to_string(),
                _ => format!("//! A module.\n{ALLOW_UNSAFE}\n"),
            })
        };
        let found = check_packages(&packages, &files, &read)?;
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].path, "crates/audio_buffers/src/pool.rs");
        Ok(())
    }

    #[test]
    fn test_crates_table_has_no_duplicates() {
        for (i, (name, _)) in CRATES.iter().enumerate() {
            assert!(
                !CRATES[i + 1..].iter().any(|(n, _)| n == name),
                "{name} listed twice"
            );
        }
    }
}
