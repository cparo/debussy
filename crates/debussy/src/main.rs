//! Debussy: opens a project and its window, playing through the default output device.
//! Command-line import and export stand in until the interface has them (#64, ADR-0057).

#![forbid(unsafe_code)]

mod args;
mod audio;
mod project;
mod services;

use std::path::PathBuf;

use anyhow::{Context, Result};
use audio_engine::Engine;
use debussy_ui::App;
use engine_bridge::Bridge;
use macos_platform::MacDevices;
use project_storage::AppSettings;
use signal_graph::Config;

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
    let (mut bundle, mut session) = project::open_or_create(&args.project, args.sample_rate)?;
    if !args.imports.is_empty() {
        project::import_files(&mut bundle, &mut session, &args.imports)?;
    }
    if let Some(path) = &args.export {
        project::export_state(&bundle, session.state(), path, args.mono)?;
        println!("exported {}", path.display());
        return Ok(());
    }
    project::build_waveform_caches(&bundle, &session).context("building waveforms")?;

    let rate = bundle.sample_rate();
    let name = bundle.path().file_stem().map_or_else(
        || "Untitled".to_owned(),
        |s| s.to_string_lossy().into_owned(),
    );
    let (bridge, runner) = Bridge::new(session.state(), Config::new(rate, project::MAX_BLOCK))?;
    let output = bridge.output().context("the project has no output")?;
    let (engine, processor) = Engine::new(runner, output, rate)?;
    let settings_path = settings_path()?;
    let settings = AppSettings::load(&settings_path);
    let (audio, started) = audio::AudioOutput::start(
        Box::new(MacDevices::new()),
        rate,
        processor,
        settings.device.as_deref(),
    );
    let services = Box::new(services::AppServices::new(
        bundle,
        audio,
        settings,
        settings_path,
    ));
    let mut app = App::new(session, rate, bridge, engine, services);
    app.set_project_name(name);
    if let Err(error) = started {
        // The window still opens; another device can be chosen there.
        app.set_status(format!("No audio output: {error:#}"));
    }
    debussy_ui::run(app).map_err(|e| anyhow::anyhow!("the window failed: {e}"))
}
