//! Running and swapping plans, inside the real-time guard.

mod support;

use std::error::Error as StdError;
use std::f32::consts::FRAC_1_SQRT_2;
use std::sync::atomic::Ordering;

use audio_buffers::Layout;
use realtime_guard::{GuardAllocator, assert_realtime};
use signal_graph::{Config, Graph, NodeId, Runner};
use support::{Counter, Gain, Latency, impulse, render};

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
