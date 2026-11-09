//! Recovering from a lost audio device: the engine is rebuilt from the project and its
//! real-time side handed to the device layer (`docs/architecture/user-interface.md` "Files and
//! devices", ADR-0081).

use audio_engine::Engine;
use engine_bridge::Bridge;

use crate::app::App;

impl App {
    /// Rebuilds the engine after its device was lost, keeping the playhead, mono listening, the
    /// armed tracks and a recording pass in progress, and runs it on the device with `device`'s
    /// id, or with `None` on the remembered device if present, else the default output.
    pub(crate) fn recover_audio(&mut self, device: Option<&str>) {
        let config = *self.bridge.graph().config();
        let rebuilt = Bridge::new(self.session.state(), config)
            .map_err(|e| e.to_string())
            .and_then(|(bridge, runner)| {
                let output = bridge.output().ok_or("the project has no output")?;
                let (engine, processor) =
                    Engine::new(runner, output, config.sample_rate).map_err(|e| e.to_string())?;
                Ok((bridge, engine, processor))
            });
        let (bridge, engine, processor) = match rebuilt {
            Ok(rebuilt) => rebuilt,
            Err(message) => {
                self.status = Some(format!("The audio engine could not be rebuilt: {message}"));
                return;
            }
        };
        self.keep_take_info();
        engine.set_mono(self.engine.mono());
        let playhead = self.engine.playhead();
        self.bridge = bridge;
        self.engine = engine;
        // The lost processor held the track feeds; play loads new ones.
        self.loaded = None;
        let result = self.engine.locate(playhead, self.services.streams());
        self.report(result.map_err(|e| e.to_string()));
        if let Err(message) = self.services.adopt_engine(processor, device) {
            self.status = Some(message);
        }
        if !self.arming.is_empty() {
            self.sync_arming();
        }
        self.refresh_devices();
        self.inputs = self.services.input_channels();
    }
}
