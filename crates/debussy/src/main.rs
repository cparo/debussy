//! Debussy: opens a project and its window, playing through the default output device, or
//! without a project the start screen, to create or open one.
//! Command-line import and export stand in until the interface has them (#64, ADR-0057).

#![forbid(unsafe_code)]

mod args;
mod audio;
mod launcher;
mod project;
mod services;

use anyhow::{Context, Result};
use macos_platform::folders;

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
                project::export_state(
                    &bundle,
                    session.state(),
                    path,
                    args.mono,
                    args.bitrate,
                    lame.as_ref(),
                )?;
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
