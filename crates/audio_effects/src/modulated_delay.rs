//! The modulated delay the chorus and flanger share (`docs/architecture/modulation.md`
//! "Modulated delay"): [`ModulatedDelay`], a node holding the LFO, two lines, `spread` and `mix`,
//! around a [`Sweep`] that says how one kind reads and writes the lines each frame.

use audio_buffers::Layout;
use dsp::TapDelay;
use modulation::{Lfo, Shape, sync};
use signal_graph::{Inputs, Node, Outputs, Parameter, ParameterInfo, Port, Ramp, Scale};

use crate::error::Error;

/// The LFO shapes a modulated delay offers, by index, with their names. The order is saved
/// format.
const SHAPES: [(&str, Shape); 2] = [("sine", Shape::Sine), ("triangle", Shape::Triangle)];

/// The seed of every modulated delay's LFO: its shapes are not random, so any fixed value does.
const SEED: u64 = 0x6368_6f72_7573;

/// The LFO parameters, which come first, in declaration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
enum LfoControl {
    Sync,
    Rate,
    Note,
    Feel,
    Shape,
}

/// How many LFO parameters there are.
const LFO_CONTROLS: usize = 5;

/// What one kind of modulated delay adds to the shared node: its own parameters, declared between
/// the LFO's and `spread` and `mix`, and how it reads and writes the lines each frame.
pub(crate) trait Sweep: Send + Sized {
    /// The LFO's defaults.
    const LFO_DEFAULTS: LfoDefaults;
    /// The longest delay the lines must hold, in milliseconds.
    const MAX_MS: f32;
    /// How many parameters [`Sweep::infos`] gives.
    const CONTROLS: usize;

    /// The kind's own parameters, in declaration order.
    fn infos() -> Vec<ParameterInfo>;

    /// The kind's state at `sample_rate`, before any values are set.
    fn new(sample_rate: u32) -> Self;

    /// Takes the kind's own parameter values, by its own index, once per block.
    fn set(&mut self, value: &dyn Fn(usize) -> f32);

    /// Every ramp the kind holds, so the node can start them at their targets.
    fn ramps(&mut self) -> impl Iterator<Item = &mut Ramp>;

    /// One frame: reads and writes `lines` for `input`, and gives each channel's wet signal. The
    /// right channel reads `lfo` `spread` cycles from the left.
    fn frame(
        &mut self,
        lines: &mut [TapDelay; 2],
        lfo: &Lfo,
        spread: f64,
        input: [f32; 2],
    ) -> [f32; 2];
}

/// A kind's LFO defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LfoDefaults {
    /// `rate`, in hertz.
    pub(crate) rate: f32,
    /// `note`, an index into [`sync::LFO_NOTES`].
    pub(crate) note: f32,
    /// `shape`, an index into [`SHAPES`].
    pub(crate) shape: f32,
}

/// A choice parameter `name` over `options`, at index `default`.
fn choice<'a>(
    name: &str,
    options: impl ExactSizeIterator<Item = &'a str>,
    default: f32,
) -> ParameterInfo {
    #[allow(clippy::cast_precision_loss)] // a handful of options
    let max = (options.len().max(1) - 1) as f32;
    ParameterInfo::new(name, 0.0, max, default)
        .with_scale(Scale::Choice(options.map(str::to_owned).collect()))
}

/// Every parameter of kind `S`, in declaration order: the LFO's in group `lfo`, then the kind's
/// own, then `spread` and `mix`.
fn infos<S: Sweep>() -> Vec<ParameterInfo> {
    let lfo = S::LFO_DEFAULTS;
    let lfo_infos = [
        ParameterInfo::new("sync", 0.0, 1.0, 0.0).with_scale(Scale::Toggle),
        ParameterInfo::new("rate", 0.05, 5.0, lfo.rate)
            .with_unit("Hz")
            .with_scale(Scale::Logarithmic)
            .shown_while("sync", false),
        choice("note", sync::LFO_NOTES.into_iter(), lfo.note).shown_while("sync", true),
        choice("feel", sync::FEELS.into_iter(), 0.0).shown_while("sync", true),
        choice("shape", SHAPES.iter().map(|&(name, _)| name), lfo.shape),
    ];
    let mut infos: Vec<ParameterInfo> = lfo_infos
        .into_iter()
        .map(|info| info.with_group("lfo"))
        .collect();
    infos.extend(S::infos());
    infos.push(
        ParameterInfo::new("spread", 0.0, 180.0, 90.0)
            .with_unit("°")
            .with_group("stereo"),
    );
    infos.push(
        ParameterInfo::new("mix", 0.0, 100.0, 50.0)
            .with_unit("%")
            .with_group("output"),
    );
    infos
}

/// `ms` milliseconds at `sample_rate`, in frames.
pub(crate) fn ms_to_frames(ms: f32, sample_rate: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)] // audio rates are exact in f32
    let rate = sample_rate as f32;
    ms * rate / 1000.0
}

/// Two delay lines, each long enough for `max_ms` milliseconds at `sample_rate` and the four
/// taps of a cubic read around it. Allocates.
fn lines(max_ms: f32, sample_rate: u32) -> [TapDelay; 2] {
    let frames = ms_to_frames(max_ms, sample_rate).ceil().max(0.0);
    // A few hundred milliseconds of frames at most.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let max_tap = frames as usize + 4;
    [TapDelay::new(max_tap), TapDelay::new(max_tap)]
}

/// The shape a `shape` parameter value asks for: an index into [`SHAPES`], rounded and clamped.
fn shape(value: f32) -> Shape {
    #[allow(clippy::cast_precision_loss)] // a handful of shapes
    let last = (SHAPES.len() - 1) as f32;
    // Held in a small range first, so the cast is exact; NaN casts to 0.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let index = value.round().clamp(0.0, last) as usize;
    SHAPES.get(index).map_or(Shape::Sine, |&(_, shape)| shape)
}

/// A prepared modulated delay.
struct Running<S> {
    lfo: Lfo,
    /// The whole-note length, in frames.
    frames_per_whole_note: f64,
    lines: [TapDelay; 2],
    /// The right channel's LFO offset, in cycles.
    spread: Ramp,
    /// The wet share of the output, 0 to 1.
    mix: Ramp,
    sweep: S,
}

impl<S: Sweep> Running<S> {
    /// A modulated delay at `sample_rate` for a whole note of `frames_per_whole_note` frames (two
    /// seconds if none, or not a positive length), its parameters at `value`'s values by index, at
    /// LFO phase 0 with no ramp in. Allocates.
    fn new(
        sample_rate: u32,
        frames_per_whole_note: Option<f64>,
        value: &dyn Fn(usize) -> f32,
    ) -> Self {
        let mut running = Running {
            lfo: Lfo::new(sample_rate, SEED),
            frames_per_whole_note: sync::whole_note(frames_per_whole_note, sample_rate),
            lines: lines(S::MAX_MS, sample_rate),
            spread: Ramp::new(0.0, sample_rate),
            mix: Ramp::new(0.0, sample_rate),
            sweep: S::new(sample_rate),
        };
        running.set(value);
        // Start at the values, with no ramp in.
        let ramps = [&mut running.spread, &mut running.mix]
            .into_iter()
            .chain(running.sweep.ramps());
        for ramp in ramps {
            *ramp = Ramp::new(ramp.target(), sample_rate);
        }
        running
    }

    /// Takes every parameter's value, by index, once per block.
    fn set(&mut self, value: &dyn Fn(usize) -> f32) {
        let lfo = |control: LfoControl| value(control as usize);
        if lfo(LfoControl::Sync) >= 0.5 {
            let period = sync::period_frames(
                self.frames_per_whole_note,
                lfo(LfoControl::Note),
                lfo(LfoControl::Feel),
            );
            self.lfo.set_period_frames(period);
        } else {
            self.lfo.set_frequency(f64::from(lfo(LfoControl::Rate)));
        }
        self.lfo.set_shape(shape(lfo(LfoControl::Shape)));
        self.sweep.set(&|i| value(LFO_CONTROLS + i));
        let spread = LFO_CONTROLS + S::CONTROLS;
        self.spread.set_target(value(spread) / 360.0);
        self.mix.set_target(value(spread + 1) / 100.0);
    }

    /// One frame of `left`, `right`.
    fn frame(&mut self, left: f32, right: f32) -> (f32, f32) {
        let spread = f64::from(self.spread.next_value());
        let mix = self.mix.next_value();
        let wet = self
            .sweep
            .frame(&mut self.lines, &self.lfo, spread, [left, right]);
        self.lfo.advance();
        let dry = 1.0 - mix;
        (left * dry + wet[0] * mix, right * dry + wet[1] * mix)
    }
}

/// A modulated delay of kind `S`: each channel's input written into a line that `S` reads at
/// delays one LFO sweeps, the right channel `spread` degrees from the left; the output mixes the
/// input with what `S` gives. One stereo input and one stereo output.
pub(crate) struct ModulatedDelay<S> {
    parameters: Vec<Parameter>,
    /// The whole-note length the graph gave, in frames.
    frames_per_whole_note: Option<f64>,
    running: Option<Running<S>>,
}

impl<S: Sweep> ModulatedDelay<S> {
    /// A modulated delay of kind `S` at its defaults. Allocates.
    ///
    /// # Errors
    ///
    /// [`Error::Parameter`] if a parameter cannot be made, which does not happen for the fixed
    /// ranges here.
    pub(crate) fn new() -> Result<Self, Error> {
        let parameters = infos::<S>()
            .into_iter()
            .map(Parameter::new)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ModulatedDelay {
            parameters,
            frames_per_whole_note: None,
            running: None,
        })
    }
}

/// The value of `parameters[index]`, or 0 if there is none.
fn get(parameters: &[Parameter], index: usize) -> f32 {
    parameters.get(index).map_or(0.0, Parameter::get)
}

impl<S: Sweep> Node for ModulatedDelay<S> {
    fn inputs(&self) -> Vec<Port> {
        vec![Port::audio("in", Layout::Stereo)]
    }

    fn outputs(&self) -> Vec<Port> {
        vec![Port::audio("out", Layout::Stereo)]
    }

    fn parameters(&self) -> Vec<Parameter> {
        self.parameters.clone()
    }

    fn uses_tempo(&self) -> bool {
        true
    }

    fn set_tempo(&mut self, frames_per_whole_note: f64) {
        self.frames_per_whole_note = Some(frames_per_whole_note);
    }

    fn prepare(&mut self, sample_rate: u32, _max_frames: usize) {
        let parameters = &self.parameters;
        self.running = Some(Running::new(
            sample_rate,
            self.frames_per_whole_note,
            &|i| get(parameters, i),
        ));
    }

    fn process(&mut self, inputs: &Inputs<'_>, outputs: &mut Outputs<'_>) {
        let parameters = &self.parameters;
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let Some((left, right)) = outputs.stereo_mut(0) else {
            return;
        };
        running.set(&|i| get(parameters, i));
        let (in_left, in_right) = (inputs.channel(0, 0), inputs.channel(0, 1));
        let sample =
            |input: Option<&[f32]>, i: usize| input.and_then(|c| c.get(i)).copied().unwrap_or(0.0);
        for (i, (l, r)) in left.iter_mut().zip(right.iter_mut()).enumerate() {
            (*l, *r) = running.frame(sample(in_left, i), sample(in_right, i));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chorus::Voices;
    use crate::flanger::Comb;

    /// The parameter named `name` of `node`.
    fn parameter<S: Sweep>(node: &ModulatedDelay<S>, name: &str) -> Result<Parameter, String> {
        node.parameters
            .iter()
            .find(|p| p.info().name == name)
            .cloned()
            .ok_or(format!("no {name}"))
    }

    /// Asserts kind `S`'s parameters are the LFO's, then its own, then `spread` and `mix`.
    fn assert_wrapped<S: Sweep>() {
        assert_eq!(S::infos().len(), S::CONTROLS);
        let infos = infos::<S>();
        assert_eq!(infos.len(), LFO_CONTROLS + S::CONTROLS + 2);
        let names: Vec<&str> = infos.iter().map(|i| i.name.as_str()).collect();
        let groups: Vec<&str> = infos.iter().map(|i| i.group.as_str()).collect();
        let tail = infos.len() - 2;
        assert_eq!(
            names[..LFO_CONTROLS],
            ["sync", "rate", "note", "feel", "shape"]
        );
        assert_eq!(names[tail..], ["spread", "mix"]);
        assert_eq!(groups[..LFO_CONTROLS], ["lfo"; LFO_CONTROLS]);
        assert_eq!(groups[tail..], ["stereo", "output"]);
    }

    #[test]
    fn test_infos_wrap_each_kind_in_lfo_spread_and_mix() {
        assert_wrapped::<Voices>();
        assert_wrapped::<Comb>();
    }

    #[test]
    fn test_infos_declare_the_lfo() {
        let infos = infos::<Voices>();
        let rate = &infos[LfoControl::Rate as usize];
        assert_eq!((rate.min, rate.max), (0.05, 5.0));
        assert_eq!(rate.scale, Scale::Logarithmic);
        let shape = &infos[LfoControl::Shape as usize];
        assert_eq!(
            shape.scale,
            Scale::Choice(vec!["sine".to_owned(), "triangle".to_owned()])
        );
        assert_eq!(infos[LfoControl::Note as usize].max, 6.0);
        assert!(infos.iter().all(|i| !i.stopped_only));
    }

    #[test]
    fn test_shape_rounds_and_clamps() {
        assert_eq!(shape(0.0), Shape::Sine);
        assert_eq!(shape(1.0), Shape::Triangle);
        assert_eq!(shape(7.0), Shape::Triangle);
        assert_eq!(shape(f32::NAN), Shape::Sine);
    }

    #[test]
    fn test_prepare_free_takes_rate_and_shape() -> Result<(), Box<dyn std::error::Error>> {
        let mut node = ModulatedDelay::<Voices>::new()?;
        parameter(&node, "shape")?.set(1.0);
        node.prepare(48_000, 64);
        let running = node.running.as_ref().ok_or("not prepared")?;
        assert!((running.lfo.frequency() - 0.8).abs() < 1e-6);
        assert_eq!(running.lfo.shape(), Shape::Triangle);
        Ok(())
    }

    #[test]
    fn test_prepare_with_sync_follows_tempo() -> Result<(), Box<dyn std::error::Error>> {
        let mut node = ModulatedDelay::<Comb>::new()?;
        parameter(&node, "sync")?.set(1.0);
        // 60 BPM in 4/4 at 48 kHz: a whole note is 192 000 frames; 2/1 is eight seconds.
        node.set_tempo(192_000.0);
        node.prepare(48_000, 64);
        let running = node.running.as_ref().ok_or("not prepared")?;
        assert_eq!(running.lfo.frequency(), 0.125);
        // A dotted half note: three seconds.
        parameter(&node, "note")?.set(3.0);
        parameter(&node, "feel")?.set(1.0);
        node.prepare(48_000, 64);
        let running = node.running.as_ref().ok_or("not prepared")?;
        assert!((running.lfo.frequency() - 1.0 / 3.0).abs() < 1e-12);
        Ok(())
    }

    #[test]
    fn test_prepare_without_tempo_uses_two_second_whole_notes()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut node = ModulatedDelay::<Voices>::new()?;
        parameter(&node, "sync")?.set(1.0);
        node.prepare(48_000, 64);
        let running = node.running.as_ref().ok_or("not prepared")?;
        // The chorus's default note is 1/1.
        assert_eq!(running.lfo.frequency(), 0.5);
        Ok(())
    }

    #[test]
    fn test_prepare_starts_spread_and_mix_at_their_values() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut node = ModulatedDelay::<Comb>::new()?;
        parameter(&node, "spread")?.set(180.0);
        parameter(&node, "mix")?.set(25.0);
        node.prepare(48_000, 64);
        let running = node.running.as_ref().ok_or("not prepared")?;
        assert_eq!(running.spread.current(), 0.5);
        assert_eq!(running.mix.current(), 0.25);
        assert!(!running.mix.is_ramping());
        Ok(())
    }

    #[test]
    fn test_lines_hold_the_longest_delay_and_four_taps() {
        let [left, right] = lines(40.0, 48_000);
        assert_eq!(left.max_tap(), 1924);
        assert_eq!(right.max_tap(), 1924);
    }
}
