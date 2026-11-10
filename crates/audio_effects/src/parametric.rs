//! The 4-band parametric equaliser with optional high-pass and low-pass stages
//! (`docs/architecture/effects.md` "Parametric equaliser").

use audio_buffers::Layout;
use dsp::{Svf, SvfCoefficients};
use signal_graph::{Inputs, Node, Outputs, Parameter, ParameterInfo, Port, Ramp, Scale};

use crate::error::Error;
use crate::filters::{self, Cascade, Response};

/// The number of bands.
pub const BANDS: usize = 4;

/// The lowest band frequency, in Hz.
const MIN_FREQUENCY: f32 = 20.0;

/// The highest band frequency, in Hz.
const MAX_FREQUENCY: f32 = 20_000.0;

/// The largest cut or boost, in dB.
pub const MAX_BAND_GAIN_DB: f32 = 24.0;

/// The narrowest and widest Q.
const Q_RANGE: (f32, f32) = (0.1, 18.0);

/// The Q a new band starts at: √½, a Butterworth-like width.
const DEFAULT_Q: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// Each band's starting frequency, in Hz.
const DEFAULT_FREQUENCIES: [f32; BANDS] = [100.0, 500.0, 2_000.0, 8_000.0];

/// A band's response. Its index is the `band N type` value, and the order is part of the saved
/// format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandType {
    /// Boosts or cuts around the frequency, by the gain, over a width set by Q.
    Bell,
    /// Boosts or cuts below the frequency.
    LowShelf,
    /// Boosts or cuts above the frequency.
    HighShelf,
    /// Removes the frequency, over a width set by Q; ignores the gain.
    Notch,
}

impl BandType {
    /// Every type, by index.
    const ALL: [BandType; 4] = [
        BandType::Bell,
        BandType::LowShelf,
        BandType::HighShelf,
        BandType::Notch,
    ];

    /// The option names, in index order.
    const NAMES: [&str; 4] = ["bell", "low shelf", "high shelf", "notch"];

    /// The type a parameter value means, rounded to the nearest index.
    pub fn from_value(value: f32) -> BandType {
        // Rounded and clamped to the table's indices first.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let index = value.round().clamp(0.0, 3.0) as usize;
        Self::ALL[index]
    }

    /// The parameter value for this type.
    pub fn value(self) -> f32 {
        match self {
            BandType::Bell => 0.0,
            BandType::LowShelf => 1.0,
            BandType::HighShelf => 2.0,
            BandType::Notch => 3.0,
        }
    }

    /// The coefficients for this type at `frequency` (Hz), `q` and `gain_db`, at `sample_rate`.
    pub(crate) fn design(
        self,
        frequency: f32,
        q: f32,
        gain_db: f32,
        sample_rate: f32,
    ) -> SvfCoefficients {
        match self {
            BandType::Bell => SvfCoefficients::bell(frequency, q, gain_db, sample_rate),
            BandType::LowShelf => SvfCoefficients::low_shelf(frequency, q, gain_db, sample_rate),
            BandType::HighShelf => SvfCoefficients::high_shelf(frequency, q, gain_db, sample_rate),
            BandType::Notch => SvfCoefficients::notch(frequency, q, sample_rate),
        }
    }
}

/// One of a band's controls, each a parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandControl {
    /// On or off; default off.
    Enabled,
    /// The [`BandType`], as its index; default bell.
    Type,
    /// Centre or corner frequency, 20 Hz to 20 kHz, logarithmic.
    Frequency,
    /// Cut or boost in dB, ±[`MAX_BAND_GAIN_DB`]; default 0.
    Gain,
    /// Width or slope, 0.1 to 18, logarithmic; default √½.
    Q,
}

/// The saved name of `control` on band `band` (0-based; names count from 1), such as
/// `"band 1 frequency"`. Names are part of the saved format.
pub fn band_parameter_name(band: usize, control: BandControl) -> String {
    let control = match control {
        BandControl::Enabled => "enabled",
        BandControl::Type => "type",
        BandControl::Frequency => "frequency",
        BandControl::Gain => "gain",
        BandControl::Q => "q",
    };
    format!("band {} {control}", band + 1)
}

/// One of a high-pass or low-pass stage's controls, each a parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassControl {
    /// On or off; default off.
    Enabled,
    /// The cutoff, 20 Hz to 20 kHz, logarithmic; default 80 Hz for the high-pass and 12 kHz for
    /// the low-pass.
    Cutoff,
    /// The order, 1 to [`crate::MAX_ORDER`] poles, changeable only while stopped; default 2.
    Order,
}

/// The group of the stage passing `response`'s side: `"high-pass"` or `"low-pass"`.
fn pass_group(response: Response) -> &'static str {
    match response {
        Response::HighPass => "high-pass",
        Response::LowPass => "low-pass",
    }
}

/// The saved name of `control` on the stage passing `response`'s side, such as
/// `"high-pass cutoff"`. Names are part of the saved format.
pub fn pass_parameter_name(response: Response, control: PassControl) -> String {
    let control = match control {
        PassControl::Enabled => "enabled",
        PassControl::Cutoff => "cutoff",
        PassControl::Order => "order",
    };
    format!("{} {control}", pass_group(response))
}

/// Whether a stage or band whose `enabled` parameter stands at `enabled` is on.
pub(crate) fn is_on(enabled: f32) -> bool {
    enabled >= 0.5
}

/// The response a band whose `enabled` and `type` parameters stand at `enabled` and `kind`
/// asks for; `None` when it is off.
pub(crate) fn wanted(enabled: f32, kind: f32) -> Option<BandType> {
    is_on(enabled).then(|| BandType::from_value(kind))
}

/// A pass stage's state once prepared.
struct PassState {
    cascade: Cascade,
    /// From 0 (the input passes unfiltered) to 1 (filtered), ramped when the stage is turned on
    /// or off.
    mix: Ramp,
}

impl PassState {
    /// Whether the stage does nothing and can be skipped: faded out and at rest.
    fn is_idle(&self) -> bool {
        self.mix.target() == 0.0 && !self.mix.is_ramping()
    }
}

/// An optional Butterworth high-pass or low-pass stage: its parameters and state.
struct PassStage {
    response: Response,
    enabled: Parameter,
    cutoff: Parameter,
    order: Parameter,
    state: Option<PassState>,
}

impl PassStage {
    fn new(response: Response) -> Result<Self, Error> {
        let group = pass_group(response);
        let name = |control| pass_parameter_name(response, control);
        Ok(PassStage {
            response,
            enabled: Parameter::new(
                ParameterInfo::new(name(PassControl::Enabled), 0.0, 1.0, 0.0)
                    .with_group(group)
                    .with_scale(Scale::Toggle)
                    .with_state_labels("Enabled", "Disabled"),
            )?,
            cutoff: Parameter::new(
                filters::cutoff_info(name(PassControl::Cutoff), response).with_group(group),
            )?,
            order: Parameter::new(filters::order_info(name(PassControl::Order)).with_group(group))?,
            state: None,
        })
    }

    fn parameters(&self) -> [Parameter; 3] {
        [
            self.enabled.clone(),
            self.cutoff.clone(),
            self.order.clone(),
        ]
    }

    fn prepare(&mut self, sample_rate: u32) {
        let on = is_on(self.enabled.get());
        self.state = Some(PassState {
            cascade: Cascade::new(
                self.response,
                self.cutoff.get(),
                self.order.get(),
                sample_rate,
            ),
            mix: Ramp::new(if on { 1.0 } else { 0.0 }, sample_rate),
        });
    }

    /// Filters `left` and `right` in place, crossfading from or to the unfiltered input while
    /// the stage is turned on or off.
    fn process(&mut self, left: &mut [f32], right: &mut [f32], sample_rate: u32) {
        let on = is_on(self.enabled.get());
        let (response, cutoff, order) = (self.response, self.cutoff.get(), self.order.get());
        let Some(state) = self.state.as_mut() else {
            return;
        };
        if state.is_idle() {
            if !on {
                return;
            }
            // Waking from rest: start at the current cutoff and order, from silence.
            state.cascade = Cascade::new(response, cutoff, order, sample_rate);
        } else {
            state.cascade.set(response, cutoff, order);
        }
        state.mix.set_target(if on { 1.0 } else { 0.0 });
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            state.cascade.tick(response);
            let mix = state.mix.next_value();
            let (wet_l, wet_r) = (state.cascade.process(0, *l), state.cascade.process(1, *r));
            if mix >= 1.0 {
                (*l, *r) = (wet_l, wet_r);
            } else {
                *l += mix * (wet_l - *l);
                *r += mix * (wet_r - *r);
            }
        }
        if state.is_idle() {
            // A fade to off has ended: the stage rests from silence when next turned on.
            state.cascade.reset();
        }
    }
}

/// A band's state once prepared.
struct BandState {
    /// Frequency in log2(Hz), gain in dB and Q in log2, as ramped.
    frequency: Ramp,
    gain: Ramp,
    q: Ramp,
    /// The response the band is moving to; `None` passes through.
    target: Option<BandType>,
    /// The coefficients a type or enable change morphs from.
    from: SvfCoefficients,
    /// From 0 to 1 over a morph; at rest, 1.
    morph: Ramp,
    /// The coefficients for the latest sample.
    current: SvfCoefficients,
    /// Left and right.
    filters: [Svf; 2],
}

/// One band's parameters and state.
struct Band {
    enabled: Parameter,
    kind: Parameter,
    frequency: Parameter,
    gain: Parameter,
    q: Parameter,
    state: Option<BandState>,
}

impl Band {
    fn new(index: usize) -> Result<Self, Error> {
        let group = format!("band {}", index + 1);
        let info = |control, min, max, default| {
            ParameterInfo::new(band_parameter_name(index, control), min, max, default)
                .with_group(&group)
        };
        let types = BandType::NAMES.map(String::from).to_vec();
        Ok(Band {
            enabled: Parameter::new(
                info(BandControl::Enabled, 0.0, 1.0, 0.0)
                    .with_scale(Scale::Toggle)
                    .with_state_labels("Enabled", "Disabled"),
            )?,
            kind: Parameter::new(
                info(BandControl::Type, 0.0, 3.0, 0.0).with_scale(Scale::Choice(types)),
            )?,
            frequency: Parameter::new(
                info(
                    BandControl::Frequency,
                    MIN_FREQUENCY,
                    MAX_FREQUENCY,
                    DEFAULT_FREQUENCIES[index % BANDS],
                )
                .with_unit("Hz")
                .with_scale(Scale::Logarithmic),
            )?,
            gain: Parameter::new(
                info(BandControl::Gain, -MAX_BAND_GAIN_DB, MAX_BAND_GAIN_DB, 0.0).with_unit("dB"),
            )?,
            q: Parameter::new(
                info(BandControl::Q, Q_RANGE.0, Q_RANGE.1, DEFAULT_Q)
                    .with_scale(Scale::Logarithmic),
            )?,
            state: None,
        })
    }

    /// The response the parameters ask for; `None` when the band is off.
    fn wanted(&self) -> Option<BandType> {
        wanted(self.enabled.get(), self.kind.get())
    }
}

impl BandState {
    /// The coefficients the band's target asks for at the ramped frequency, gain and Q; with no
    /// target, the morph's start with the identity mix.
    fn target_coefficients(&self, sample_rate: f32) -> SvfCoefficients {
        match self.target {
            Some(kind) => kind.design(
                self.frequency.current().exp2(),
                self.q.current().exp2(),
                self.gain.current(),
                sample_rate,
            ),
            None => self.from.bypassed(),
        }
    }

    /// Whether any value is moving, so coefficients must be recomputed each sample.
    fn is_moving(&self) -> bool {
        self.frequency.is_ramping()
            || self.gain.is_ramping()
            || self.q.is_ramping()
            || self.morph.is_ramping()
    }

    /// Whether the band does nothing and can be skipped: off and not morphing.
    fn is_idle(&self) -> bool {
        self.target.is_none() && !self.morph.is_ramping()
    }

    /// Steps every ramp one sample and sets the coefficients for it.
    fn advance(&mut self, sample_rate: f32) {
        self.frequency.next_value();
        self.gain.next_value();
        self.q.next_value();
        let t = self.morph.next_value();
        let target = self.target_coefficients(sample_rate);
        self.current = if t < 1.0 {
            self.from.lerp(&target, t)
        } else {
            target
        };
    }
}

/// A parametric equaliser of [`BANDS`] bands in series, each a bell, low shelf, high shelf or
/// notch with its own frequency, gain and Q, after an optional Butterworth high-pass stage and
/// before an optional Butterworth low-pass stage. One stereo input and one stereo output.
///
/// Frequency and Q ramp in their logarithms and gain in dB, with coefficients recomputed each
/// sample while they move. Turning a band on or off, or changing its type, morphs its
/// coefficients over `signal_graph::RAMP_SECONDS`; turning a pass stage on or off crossfades it
/// with its input over the same time. A band or stage that is off is skipped. A pass stage's
/// order, like [`crate::Butterworth`]'s, takes effect at once, from rest.
pub struct ParametricEq {
    high_pass: PassStage,
    bands: Vec<Band>,
    low_pass: PassStage,
    /// The sample rate, as prepared.
    rate: u32,
    sample_rate: f32,
}

impl ParametricEq {
    /// An equaliser with every band and stage off, each at its default frequency. Allocates.
    ///
    /// # Errors
    ///
    /// [`Error::Parameter`] if a parameter cannot be made, which does not happen for the fixed
    /// ranges here.
    pub fn new() -> Result<Self, Error> {
        let bands = (0..BANDS).map(Band::new).collect::<Result<Vec<_>, _>>()?;
        Ok(ParametricEq {
            high_pass: PassStage::new(Response::HighPass)?,
            bands,
            low_pass: PassStage::new(Response::LowPass)?,
            rate: 48_000,
            sample_rate: 48_000.0,
        })
    }
}

impl Node for ParametricEq {
    fn inputs(&self) -> Vec<Port> {
        vec![Port::audio("in", Layout::Stereo)]
    }

    fn outputs(&self) -> Vec<Port> {
        vec![Port::audio("out", Layout::Stereo)]
    }

    fn parameters(&self) -> Vec<Parameter> {
        let bands = self.bands.iter().flat_map(|b| {
            [
                b.enabled.clone(),
                b.kind.clone(),
                b.frequency.clone(),
                b.gain.clone(),
                b.q.clone(),
            ]
        });
        self.high_pass
            .parameters()
            .into_iter()
            .chain(bands)
            .chain(self.low_pass.parameters())
            .collect()
    }

    fn prepare(&mut self, sample_rate: u32, _max_frames: usize) {
        // Sample rates are exact in `f32` well past any audio rate.
        #[allow(clippy::cast_precision_loss)]
        let rate = sample_rate as f32;
        self.sample_rate = rate;
        self.rate = sample_rate;
        self.high_pass.prepare(sample_rate);
        self.low_pass.prepare(sample_rate);
        for band in &mut self.bands {
            let target = band.wanted();
            let mut state = BandState {
                frequency: Ramp::new(band.frequency.get().log2(), sample_rate),
                gain: Ramp::new(band.gain.get(), sample_rate),
                q: Ramp::new(band.q.get().log2(), sample_rate),
                target,
                from: SvfCoefficients::identity(),
                morph: Ramp::new(1.0, sample_rate),
                current: SvfCoefficients::identity(),
                filters: [Svf::new(); 2],
            };
            // With no target, pass through at the band's own frequency.
            let designed = BandType::from_value(band.kind.get()).design(
                band.frequency.get(),
                band.q.get(),
                band.gain.get(),
                rate,
            );
            state.from = designed.bypassed();
            state.current = state.target_coefficients(rate);
            band.state = Some(state);
        }
    }

    fn process(&mut self, inputs: &Inputs<'_>, outputs: &mut Outputs<'_>) {
        let Some((left, right)) = outputs.stereo_mut(0) else {
            return;
        };
        let (in_left, in_right) = (inputs.channel(0, 0), inputs.channel(0, 1));
        let sample =
            |input: Option<&[f32]>, i: usize| input.and_then(|c| c.get(i)).copied().unwrap_or(0.0);
        for (i, (l, r)) in left.iter_mut().zip(right.iter_mut()).enumerate() {
            *l = sample(in_left, i);
            *r = sample(in_right, i);
        }
        self.high_pass.process(left, right, self.rate);
        let rate = self.sample_rate;
        for band in &mut self.bands {
            let wanted = band.wanted();
            let (frequency, gain, q) = (band.frequency.get(), band.gain.get(), band.q.get());
            let Some(state) = band.state.as_mut() else {
                continue;
            };
            state.frequency.set_target(frequency.log2());
            state.gain.set_target(gain);
            state.q.set_target(q.log2());
            if wanted != state.target {
                if state.is_idle() {
                    // Waking from rest: start from the new response's own `g` and `k`, so
                    // only the mix moves, with the filter state cleared when it went idle.
                    let kind = wanted.unwrap_or(BandType::Bell);
                    state.from = kind
                        .design(
                            state.frequency.current().exp2(),
                            state.q.current().exp2(),
                            state.gain.current(),
                            rate,
                        )
                        .bypassed();
                } else {
                    state.from = state.current;
                }
                state.target = wanted;
                state.morph = Ramp::new(0.0, self.rate);
                state.morph.set_target(1.0);
            }
            if state.is_idle() {
                continue;
            }
            for (l, r) in left.iter_mut().zip(right.iter_mut()) {
                if state.is_moving() {
                    state.advance(rate);
                }
                *l = state.filters[0].process(*l, &state.current);
                *r = state.filters[1].process(*r, &state.current);
            }
            if state.is_idle() {
                // A morph to off has ended: the band rests from silence when next turned on.
                state.filters = [Svf::new(); 2];
            }
        }
        self.low_pass.process(left, right, self.rate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_declares_ports_and_parameters() -> Result<(), Error> {
        let eq = ParametricEq::new()?;
        assert_eq!(eq.inputs(), vec![Port::audio("in", Layout::Stereo)]);
        assert_eq!(eq.outputs(), vec![Port::audio("out", Layout::Stereo)]);
        let infos: Vec<ParameterInfo> = eq.parameters().iter().map(|p| p.info().clone()).collect();
        assert_eq!(infos.len(), 3 + 5 * BANDS + 3);
        let names: Vec<&str> = infos[3..8].iter().map(|i| i.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "band 1 enabled",
                "band 1 type",
                "band 1 frequency",
                "band 1 gain",
                "band 1 q"
            ]
        );
        assert_eq!(infos[3].scale, Scale::Toggle);
        assert_eq!(
            infos[3].state_labels,
            Some(("Enabled".to_owned(), "Disabled".to_owned()))
        );
        assert_eq!(infos[3].default, 0.0);
        assert!(matches!(&infos[4].scale, Scale::Choice(o) if o.len() == 4 && o[3] == "notch"));
        let defaults: Vec<f32> = infos[3..23]
            .iter()
            .skip(2)
            .step_by(5)
            .map(|i| i.default)
            .collect();
        assert_eq!(defaults, DEFAULT_FREQUENCIES);
        assert_eq!(
            (infos[6].min, infos[6].max, infos[6].unit.as_str()),
            (-24.0, 24.0, "dB")
        );
        assert_eq!((infos[7].min, infos[7].max), (0.1, 18.0));
        assert_eq!(infos[7].default, DEFAULT_Q);
        assert!(infos[3..23].iter().all(|i| !i.stopped_only));
        assert_eq!(infos[22].group, "band 4");
        Ok(())
    }

    #[test]
    fn test_new_frames_the_bands_with_pass_stages() -> Result<(), Error> {
        let eq = ParametricEq::new()?;
        let infos: Vec<ParameterInfo> = eq.parameters().iter().map(|p| p.info().clone()).collect();
        for (stage, group, cutoff) in [
            (&infos[..3], "high-pass", 80.0),
            (&infos[23..], "low-pass", 12_000.0),
        ] {
            let names: Vec<&str> = stage.iter().map(|i| i.name.as_str()).collect();
            assert_eq!(
                names,
                [
                    format!("{group} enabled"),
                    format!("{group} cutoff"),
                    format!("{group} order")
                ]
            );
            assert!(stage.iter().all(|i| i.group == group));
            assert_eq!(stage[0].scale, Scale::Toggle);
            assert_eq!(stage[0].default, 0.0);
            assert_eq!(
                (stage[1].min, stage[1].max, stage[1].default),
                (20.0, 20_000.0, cutoff)
            );
            assert_eq!(stage[1].scale, Scale::Logarithmic);
            assert!(!stage[1].stopped_only);
            assert_eq!(
                (stage[2].min, stage[2].max, stage[2].default),
                (1.0, 8.0, 2.0)
            );
            assert_eq!(stage[2].scale, Scale::Integer);
            assert!(stage[2].stopped_only);
        }
        assert_eq!(
            pass_parameter_name(Response::LowPass, PassControl::Cutoff),
            "low-pass cutoff"
        );
        Ok(())
    }

    #[test]
    fn test_band_type_round_trips() {
        for kind in BandType::ALL {
            assert_eq!(BandType::from_value(kind.value()), kind);
        }
        assert_eq!(BandType::from_value(9.0), BandType::Notch);
        assert_eq!(BandType::from_value(-1.0), BandType::Bell);
    }
}
