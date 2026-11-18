//! Debussy: opens a project and its window, playing through the default output device, or
//! without a project the start screen, to create or open one; projects double-clicked in the
//! Finder open in the window.
//! Command-line import and export stand in until the interface has them (#64, ADR-0057).

#![forbid(unsafe_code)]

mod args;
mod audio;
mod export_settings;
mod launcher;
mod midi_out;
mod project;
mod services;

use anyhow::{Context, Result};
use macos_platform::{documents, folders};

fn main() -> Result<()> {
    let args = args::parse(std::env::args().skip(1))?;
    let settings_path = folders::settings_path().context("HOME is not set")?;
    // Looked for once; MP3 export is offered only if it is found (ADR-0129).
    let lame = import_export::Lame::probe(&folders::lame_candidates());
    let launcher = launcher::AppLauncher::new(settings_path, folders::projects_dir(), lame.clone());
    let app = match &args.project {
        Some(path) => {
            let (mut bundle, mut session) = project::open_or_create(path, args.sample_rate)?;
            if !args.imports.is_empty() {
                project::import_files(&mut bundle, &mut session, &args.imports)?;
            }
            if let Some(path) = &args.export {
                let format = project::format_for_path(path, args.bitrate)?;
                project::export_state(
                    &bundle,
                    session.state(),
                    path,
                    format,
                    args.mono,
                    lame.as_ref(),
                )?;
                println!("exported {}", path.display());
                return Ok(());
            }
            let start = launcher.ready(bundle, session)?;
            // Absolute, to match the paths the Finder sends.
            let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            Some((start(), path))
        }
        None => None,
    };
    // Projects double-clicked in the Finder, at launch or later (ADR-0132).
    let requests = debussy_ui::OpenRequests::new();
    documents::on_open_documents(requests.sender())
        .context("listening for documents opened from the Finder")?;
    debussy_ui::run(Box::new(launcher), app, requests)
        .map_err(|e| anyhow::anyhow!("the window failed: {e}"))
}
