//! Debussy: opens a project and its window, playing through the default output device.
//! Command-line import and export stand in until the interface has them (#64, ADR-0057).

#![forbid(unsafe_code)]

mod args;
mod audio;
mod project;
mod services;

use anyhow::{Context, Result};
use audio_engine::Engine;
use debussy_ui::App;
use engine_bridge::Bridge;
use macos_platform::MacDevices;
use signal_graph::Config;

fn main() -> Result<()> {
    let args = args::parse(std::env::args().skip(1))?;
    let (mut bundle, mut session) = project::open_or_create(&args.project, args.sample_rate)?;
    if !args.imports.is_empty() {
        project::import_files(&mut bundle, &mut session, &args.imports)?;
    }
    if let Some(path) = &args.export {
        project::export_mix(&bundle, &session, path, args.mono)?;
        println!("exported {}", path.display());
        return Ok(());
    }
    project::build_waveform_caches(&bundle, &session).context("building waveforms")?;

    let rate = bundle.sample_rate();
    let (bridge, runner) = Bridge::new(session.state(), Config::new(rate, project::MAX_BLOCK))?;
    let output = bridge.output().context("the project has no output")?;
    let (engine, processor) = Engine::new(runner, output, rate)?;
    let _stream = audio::start(&MacDevices::new(), rate, processor)?;
    let services = Box::new(services::AppServices::new(bundle));
    debussy_ui::run(App::new(session, rate, bridge, engine, services))
        .map_err(|e| anyhow::anyhow!("the window failed: {e}"))
}
