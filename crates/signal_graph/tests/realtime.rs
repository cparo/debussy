//! Running and swapping plans, inside the real-time guard.

mod support;

use std::error::Error as StdError;
use std::f32::consts::FRAC_1_SQRT_2;
use std::sync::atomic::Ordering;

use audio_buffers::Layout;
use realtime_guard::{GuardAllocator, assert_realtime};
use std::sync::Arc;

use modulation::{Curve, Handle, Point};
use signal_graph::{CONTROL_PERIOD, Config, Graph, NodeId, Runner};
use support::{Counter, Gain, Latency, impulse, render, render_playing};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

type TestResult = Result<(), Box<dyn StdError>>;

const RATE: u32 = 48_000;

#[test]
fn test_plan_runs_a_chain() -> TestResult {
    let (mut graph, mut runner) = Graph::new(Config::new(RATE, 128))?;
    let source = graph.add_source(Layout::Mono);
    let gain = graph.add_node(Box::new(Gain::new(0.5)?));
    let sink = graph.add_sink(Layout::Mono);
    graph.connect(source, 0, gain, 0)?;
    graph.connect(gain, 0, sink, 0)?;
    graph.commit()?;
    let input: Vec<f32> = (0..300).map(|i| i as f32).collect();
    let output = render(&mut runner, Some(source), sink, &input, 128)?;
    let expected: Vec<f32> = input.iter().map(|x| x * 0.5).collect();
    assert_eq!(output, expected);
    Ok(())
}

#[test]
fn test_parameter_change_ramps_over_ten_ms() -> TestResult {
    let (mut graph, mut runner) = Graph::new(Config::new(RATE, 128))?;
    let source = graph.add_source(Layout::Mono);
    let gain = graph.add_node(Box::new(Gain::new(1.0)?));
    let sink = graph.add_sink(Layout::Mono);
    graph.connect(source, 0, gain, 0)?;
    graph.connect(gain, 0, sink, 0)?;
    graph.commit()?;
    let parameter = graph.parameters(gain).ok_or("no parameter")?[0].clone();
    parameter.set(0.0);
    let output = render(&mut runner, Some(source), sink, &[1.0; 640], 128)?;
    assert!((output[239] - 0.5).abs() < 1e-3);
    assert!(output[..479].iter().all(|&x| x > 0.0));
    assert!(output[479..].iter().all(|&x| x == 0.0));
    Ok(())
}

/// Runs one block of `frames` with every sample of every source set to `value` and returns
/// channel `channel` of `sink`.
fn one_block(
    runner: &mut Runner,
    sources: &[NodeId],
    sink: NodeId,
    channel: usize,
    value: f32,
) -> Result<f32, &'static str> {
    assert_realtime(|| {
        runner.begin_block(32).map_err(|_| "refused")?;
        for &source in sources {
            let block = runner.source_mut(source).ok_or("no source")?;
            for c in 0..block.channels() {
                block.channel_mut(c).ok_or("no channel")?.fill(value);
            }
        }
        runner.process();
        let block = runner.sink(sink).ok_or("no sink")?;
        Ok(block.channel(channel).ok_or("no channel")?[31])
    })
}

#[test]
fn test_mono_into_stereo_is_panned() -> TestResult {
    let (mut graph, mut runner) = Graph::new(Config::new(RATE, 32))?;
    let source = graph.add_source(Layout::Mono);
    let sink = graph.add_sink(Layout::Stereo);
    let connection = graph.connect(source, 0, sink, 0)?;
    graph.commit()?;
    let left = one_block(&mut runner, &[source], sink, 0, 1.0)?;
    let right = one_block(&mut runner, &[source], sink, 1, 1.0)?;
    assert!((left - FRAC_1_SQRT_2).abs() < 1e-6 && (right - FRAC_1_SQRT_2).abs() < 1e-6);
    graph.set_pan(connection, -1.0)?;
    graph.commit()?;
    let left = one_block(&mut runner, &[source], sink, 0, 1.0)?;
    let right = one_block(&mut runner, &[source], sink, 1, 1.0)?;
    assert!((left - 1.0).abs() < 1e-6 && right.abs() < 1e-6);
    Ok(())
}

#[test]
fn test_stereo_into_mono_is_folded_and_inputs_sum() -> TestResult {
    let (mut graph, mut runner) = Graph::new(Config::new(RATE, 32))?;
    let stereo = graph.add_source(Layout::Stereo);
    let mono = graph.add_source(Layout::Mono);
    let sink = graph.add_sink(Layout::Mono);
    graph.connect(stereo, 0, sink, 0)?;
    graph.connect(mono, 0, sink, 0)?;
    graph.commit()?;
    let value = one_block(&mut runner, &[stereo, mono], sink, 0, 0.5)?;
    assert!((value - (FRAC_1_SQRT_2 + 0.5)).abs() < 1e-6);
    Ok(())
}

/// Renders an impulse through a loop, in blocks of `block`: source → mix → sink, and
/// mix → 2 ms delay → gain 0.5 → mix.
fn render_feedback(block: usize) -> Result<Vec<f32>, Box<dyn StdError>> {
    let (mut graph, mut runner) = Graph::new(Config::new(RATE, 1024))?;
    let source = graph.add_source(Layout::Mono);
    let mix = graph.add_node(Box::new(Gain::new(1.0)?));
    let delay = graph.add_feedback_delay(Layout::Mono, 0.002)?;
    let feedback = graph.add_node(Box::new(Gain::new(0.5)?));
    let sink = graph.add_sink(Layout::Mono);
    graph.connect(source, 0, mix, 0)?;
    graph.connect(mix, 0, sink, 0)?;
    graph.connect(mix, 0, delay, 0)?;
    graph.connect(delay, 0, feedback, 0)?;
    graph.connect(feedback, 0, mix, 0)?;
    graph.commit()?;
    Ok(render(
        &mut runner,
        Some(source),
        sink,
        &impulse(4096),
        block,
    )?)
}

#[test]
fn test_feedback_loop_does_not_depend_on_block_size() -> TestResult {
    let reference = render_feedback(1024)?;
    for (n, &x) in reference.iter().enumerate() {
        let expected = if n % 96 == 0 {
            0.5_f32.powi((n / 96) as i32)
        } else {
            0.0
        };
        assert_eq!(x, expected, "frame {n}");
    }
    for block in [32, 100, 128, 1000] {
        assert_eq!(render_feedback(block)?, reference, "block {block}");
    }
    Ok(())
}

#[test]
fn test_delay_compensation_aligns_parallel_paths() -> TestResult {
    let (mut graph, mut runner) = Graph::new(Config::new(RATE, 64))?;
    let source = graph.add_source(Layout::Mono);
    let slow = graph.add_node(Box::new(Latency::new(10)));
    let mix = graph.add_node(Box::new(Gain::new(1.0)?));
    let sink = graph.add_sink(Layout::Mono);
    let direct = graph.add_sink(Layout::Mono);
    graph.connect(source, 0, slow, 0)?;
    graph.connect(slow, 0, mix, 0)?;
    graph.connect(source, 0, mix, 0)?;
    graph.connect(mix, 0, sink, 0)?;
    graph.connect(source, 0, direct, 0)?;
    graph.commit()?;
    let output = render(&mut runner, Some(source), sink, &impulse(200), 64)?;
    let mut expected = vec![0.0; 200];
    expected[10] = 2.0;
    assert_eq!(output, expected);
    assert_eq!(runner.latency(), 10);
    // A path with no latency to another sink is delayed to match.
    let output = render(&mut runner, Some(source), direct, &impulse(200), 64)?;
    expected[10] = 1.0;
    assert_eq!(output, expected);
    Ok(())
}

#[test]
fn test_commit_carries_node_state_over() -> TestResult {
    let (mut graph, mut runner) = Graph::new(Config::new(RATE, 64))?;
    let (counter, _) = Counter::new();
    let counter = graph.add_node(Box::new(counter));
    let sink = graph.add_sink(Layout::Mono);
    graph.connect(counter, 0, sink, 0)?;
    graph.commit()?;
    render(&mut runner, None, sink, &[0.0; 100], 64)?;
    let other = graph.add_sink(Layout::Mono);
    graph.connect(counter, 0, other, 0)?;
    graph.commit()?;
    let output = render(&mut runner, None, other, &[0.0; 3], 3)?;
    assert_eq!(output, [100.0, 101.0, 102.0]);
    Ok(())
}

#[test]
fn test_removed_node_is_dropped_off_the_realtime_thread() -> TestResult {
    let (mut graph, mut runner) = Graph::new(Config::new(RATE, 64))?;
    let (counter, dropped) = Counter::new();
    let counter = graph.add_node(Box::new(counter));
    let sink = graph.add_sink(Layout::Mono);
    graph.connect(counter, 0, sink, 0)?;
    graph.commit()?;
    render(&mut runner, None, sink, &[0.0; 64], 64)?;
    graph.remove_node(counter)?;
    graph.commit()?;
    // The swap happens inside the guard; the old plan is sent back, not freed.
    let output = render(&mut runner, None, sink, &[1.0; 64], 64)?;
    assert!(output.iter().all(|&x| x == 0.0));
    assert!(!dropped.load(Ordering::SeqCst));
    graph.collect();
    assert!(dropped.load(Ordering::SeqCst));
    Ok(())
}

#[test]
fn test_begin_block_refuses_oversized_blocks() -> TestResult {
    let (_graph, mut runner) = Graph::new(Config::new(RATE, 64))?;
    let result = assert_realtime(|| runner.begin_block(65).is_err());
    assert!(result);
    Ok(())
}

#[test]
fn test_replace_node_copies_parameter_values() -> TestResult {
    let (mut graph, mut runner) = Graph::new(Config::new(RATE, 64))?;
    let source = graph.add_source(Layout::Mono);
    let gain = graph.add_node(Box::new(Gain::new(1.0)?));
    let sink = graph.add_sink(Layout::Mono);
    graph.connect(source, 0, gain, 0)?;
    graph.connect(gain, 0, sink, 0)?;
    graph.commit()?;
    graph.parameters(gain).ok_or("no parameter")?[0].set(0.25);
    let dropped = graph.replace_node(gain, Box::new(Gain::new(1.0)?))?;
    assert!(dropped.is_empty());
    graph.commit()?;
    // The new node starts at the copied value, with no ramp.
    let output = render(&mut runner, Some(source), sink, &[1.0; 64], 64)?;
    assert!(output.iter().all(|&x| x == 0.25));
    Ok(())
}

#[test]
fn test_replaced_node_runs_a_new_instance() -> TestResult {
    let (mut graph, mut runner) = Graph::new(Config::new(RATE, 64))?;
    let (counter, old_dropped) = Counter::new();
    let counter = graph.add_node(Box::new(counter));
    let sink = graph.add_sink(Layout::Mono);
    graph.connect(counter, 0, sink, 0)?;
    graph.commit()?;
    render(&mut runner, None, sink, &[0.0; 64], 64)?;
    let (fresh, _) = Counter::new();
    graph.replace_node(counter, Box::new(fresh))?;
    graph.commit()?;
    let output = render(&mut runner, None, sink, &[0.0; 3], 3)?;
    assert_eq!(output, [0.0, 1.0, 2.0]);
    assert!(!old_dropped.load(Ordering::SeqCst));
    graph.collect();
    assert!(old_dropped.load(Ordering::SeqCst));
    Ok(())
}

/// A straight curve from value 0 at frame 0 to 1 at frame `end`.
fn ramp_curve(end: i64) -> Result<Arc<Curve>, Box<dyn StdError>> {
    let third = end as f64 / 3.0;
    Ok(Arc::new(Curve::new(vec![
        Point::new(0, 0.0).with_handles(Handle::default(), Handle::new(third, 1.0 / 3.0)),
        Point::new(end, 1.0).with_handles(Handle::new(-third, -1.0 / 3.0), Handle::default()),
    ])?))
}

/// A graph, its runner, and its source, gain and sink.
type Driven = (Graph, Runner, NodeId, NodeId, NodeId);

/// A source through `upstream` frames of latency into a gain driven by `curve`, into a sink.
fn driven_gain(
    curve: Arc<Curve>,
    upstream: usize,
    max_frames: usize,
) -> Result<Driven, Box<dyn StdError>> {
    let (mut graph, runner) = Graph::new(Config::new(RATE, max_frames))?;
    let source = graph.add_source(Layout::Mono);
    let gain = graph.add_node(Box::new(Gain::new(1.0)?));
    let sink = graph.add_sink(Layout::Mono);
    let curve = graph.add_curve(curve);
    if upstream > 0 {
        let delay = graph.add_node(Box::new(Latency::new(upstream)));
        graph.connect(source, 0, delay, 0)?;
        graph.connect(delay, 0, gain, 0)?;
    } else {
        graph.connect(source, 0, gain, 0)?;
    }
    graph.connect(gain, 0, sink, 0)?;
    graph.connect_curve(curve, gain, "gain")?;
    graph.commit()?;
    Ok((graph, runner, source, gain, sink))
}

#[test]
fn test_curve_output_does_not_depend_on_block_size() -> TestResult {
    let input = vec![1.0; 6_000];
    let mut outputs = Vec::new();
    for block in [64, 100, 512] {
        let (_graph, mut runner, source, _, sink) = driven_gain(ramp_curve(4_800)?, 0, 512)?;
        outputs.push(render_playing(
            &mut runner,
            Some(source),
            sink,
            &input,
            block,
            0,
        )?);
    }
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[0], outputs[2]);
    // The gain (0 to 4) reaches the curve's value at each control point and is linear between.
    let gain = |frame: f32| 4.0 * (frame / 4_800.0).min(1.0);
    for k in (CONTROL_PERIOD as usize - 1..6_000).step_by(CONTROL_PERIOD as usize) {
        assert!(
            (outputs[0][k] - gain(k as f32 + 1.0)).abs() < 1e-4,
            "frame {k}"
        );
    }
    for (k, &x) in outputs[0].iter().enumerate().take(200).skip(100) {
        assert!((x - gain(k as f32 + 1.0)).abs() < 1e-4, "frame {k}");
    }
    Ok(())
}

#[test]
fn test_curve_is_read_behind_by_the_nodes_arrival_time() -> TestResult {
    let input = vec![1.0; 3_000];
    let (_g, mut runner, source, _, sink) = driven_gain(ramp_curve(4_800)?, 0, 128)?;
    let plain = render_playing(&mut runner, Some(source), sink, &input, 128, 0)?;
    let (_g, mut runner, source, _, sink) = driven_gain(ramp_curve(4_800)?, 96, 128)?;
    let late = render_playing(&mut runner, Some(source), sink, &input, 128, 0)?;
    // The gain sees audio from 96 frames earlier, so reads the curve 96 frames earlier.
    for k in 200..3_000 {
        assert!((late[k] - plain[k - 96]).abs() < 1e-4, "frame {k}");
    }
    Ok(())
}

#[test]
fn test_curve_overrides_the_set_value_and_releases_to_it() -> TestResult {
    let flat = Arc::new(Curve::new(vec![Point::new(0, 0.25)])?);
    let (mut graph, mut runner, source, gain, sink) = driven_gain(flat, 0, 128)?;
    let parameter = graph.parameters(gain).ok_or("no parameter")?[0].clone();
    parameter.set(2.0);
    let driven = render_playing(&mut runner, Some(source), sink, &[1.0; 1_024], 128, 0)?;
    // The curve's 0.25 maps to a gain of 1 on the 0 to 4 range; the set value is ignored.
    assert!(driven[100..].iter().all(|&x| (x - 1.0).abs() < 1e-6));
    assert!(parameter.is_automated());
    assert_eq!(parameter.base(), 2.0);

    let control = graph.control_connections().next().ok_or("no control")?.0;
    graph.disconnect(control)?;
    graph.commit()?;
    let released = render_playing(&mut runner, Some(source), sink, &[1.0; 1_024], 128, 1_024)?;
    assert!(!parameter.is_automated());
    // Back to the set value over the normal 10 ms ramp.
    assert!(released[..479].iter().all(|&x| x < 2.0));
    assert!(released[480..].iter().all(|&x| x == 2.0));
    Ok(())
}

#[test]
fn test_curve_is_read_at_the_playhead_while_stopped() -> TestResult {
    let (_g, mut runner, source, _, sink) = driven_gain(ramp_curve(4_800)?, 0, 128)?;
    runner.set_transport(2_400, false);
    let output = render(&mut runner, Some(source), sink, &[1.0; 256], 128)?;
    // Half way along the curve: a gain of 2, reached within the first block.
    assert!(output[127..].iter().all(|&x| (x - 2.0).abs() < 1e-4));
    Ok(())
}
