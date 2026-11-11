//! Debussy: opens a project and its window, playing through the default output device, or
//! without a project the start screen, to create or open one.
//! Command-line import and export stand in until the interface has them (#64, ADR-0057).

#![forbid(unsafe_code)]

mod args;
mod audio;
mod launcher;
mod project;
mod services;

use std::path::PathBuf;

use anyhow::{Context, Result};

/// Where the app settings live: `~/Library/Application Support/Debussy/settings.json`.
fn settings_path() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("Debussy")
        .join("settings.json"))
}

fn main() -> Result<()> {
    let args = args::parse(std::env::args().skip(1))?;
    let launcher = launcher::AppLauncher::new(settings_path()?);
    let app = match &args.project {
        Some(path) => {
            let (mut bundle, mut session) = project::open_or_create(path, args.sample_rate)?;
            if !args.imports.is_empty() {
                project::import_files(&mut bundle, &mut session, &args.imports)?;
            }
            if let Some(path) = &args.export {
                project::export_state(&bundle, session.state(), path, args.mono)?;
                println!("exported {}", path.display());
                return Ok(());
            }
            let start = launcher.ready(bundle, session)?;
            Some(start())
        }
        None => None,
    };
    debussy_ui::run(Box::new(launcher), app).map_err(|e| anyhow::anyhow!("the window failed: {e}"))
}
