//! 48 kHz, stereo, 256-frame streaming high-frequency completion.
//! The trained weights are embedded. The caller owns audio routing and input bandwidth.

use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};
use std::f32::consts::PI;
use std::sync::Arc;

pub const SAMPLE_RATE: usize = 48_000;
pub const CHANNELS: usize = 2;
pub const CHUNK_FRAMES: usize = 256;
pub const ALGORITHM_DELAY_FRAMES: usize = 768;
const WINDOW: usize = 1024;
const LOW: usize = 257;
const HIGH_START: usize = 257;
const HIGH_END: usize = 427;
const POOLED: usize = 128;
const CONTEXT: usize = 4;
const INPUT: usize = POOLED * CONTEXT;
const HIDDEN: usize = 256;
const BINS: usize = HIGH_END - HIGH_START;
const OUT: usize = BINS;
const LIMITER_RELEASE_PER_CHUNK: f32 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bandwidth {
    /// Caller has determined that the source lacks energy above 12 kHz.
    BandLimited,
    /// Explicitly preserve the full-band source; no model output is mixed in.
    FullBandBypass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessError {
    WrongChunkSize,
}

impl ProcessError {
    pub fn code(self) -> &'static str {
        "wrong_chunk_size"
    }
    pub fn message(self) -> &'static str {
        "input and output must each contain exactly 256 stereo frames"
    }
    pub fn details(self) -> &'static str {
        "expected 512 interleaved f32 samples"
    }
    pub fn trace_id(self) -> Option<&'static str> {
        None
    }
}

struct Layer {
    weights: Vec<f32>,
    bias: Vec<f32>,
    inputs: usize,
    outputs: usize,
}

impl Layer {
    fn load(bytes: &[u8], offset: &mut usize, inputs: usize, outputs: usize) -> Self {
        let take = |count: usize, offset: &mut usize| -> Vec<f32> {
            (0..count)
                .map(|_| {
                    let value = f32::from_le_bytes(
                        bytes[*offset..*offset + 4]
                            .try_into()
                            .expect("checkpoint length"),
                    );
                    *offset += 4;
                    value
                })
                .collect()
        };
        Self {
            weights: take(inputs * outputs, offset),
            bias: take(outputs, offset),
            inputs,
            outputs,
        }
    }

    fn run(&self, input: &[f32], output: &mut [f32]) {
        for (row, target) in output.iter_mut().take(self.outputs).enumerate() {
            let mut sum = self.bias[row];
            let weights = &self.weights[row * self.inputs..(row + 1) * self.inputs];
            for (&weight, &value) in weights.iter().zip(input) {
                sum += weight * value;
            }
            *target = sum;
        }
    }
}

struct Model {
    first: Layer,
    second: Layer,
    third: Layer,
}

impl Model {
    fn embedded() -> Self {
        let bytes = include_bytes!("../model/checkpoint.bin");
        assert_eq!(&bytes[..8], b"ID14SR04", "checkpoint format");
        let read_dim = |i| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap()) as usize;
        assert_eq!(
            (read_dim(8), read_dim(12), read_dim(16), read_dim(20)),
            (INPUT, HIDDEN, OUT, CONTEXT)
        );
        let mut offset = 24;
        let first = Layer::load(bytes, &mut offset, INPUT, HIDDEN);
        let second = Layer::load(bytes, &mut offset, HIDDEN, HIDDEN);
        let third = Layer::load(bytes, &mut offset, HIDDEN, OUT);
        assert_eq!(offset, bytes.len(), "checkpoint has trailing data");
        Self {
            first,
            second,
            third,
        }
    }

    fn predict(&self, features: &[f32; INPUT], result: &mut [f32; OUT]) {
        let mut hidden_a = [0.0; HIDDEN];
        let mut hidden_b = [0.0; HIDDEN];
        self.first.run(features, &mut hidden_a);
        hidden_a.iter_mut().for_each(|x| *x = x.max(0.0));
        self.second.run(&hidden_a, &mut hidden_b);
        hidden_b.iter_mut().for_each(|x| *x = x.max(0.0));
        self.third.run(&hidden_b, result);
        result.iter_mut().for_each(|x| {
            *x = if *x > 20.0 { *x } else { x.exp().ln_1p() };
        });
    }
}

/// Fixed-size callback API. Construct outside the audio callback; `process` allocates nothing.
pub struct StreamingSr {
    model: Model,
    forward: Arc<dyn Fft<f32>>,
    inverse: Arc<dyn Fft<f32>>,
    forward_scratch: Vec<Complex32>,
    inverse_scratch: Vec<Complex32>,
    spectrum: Vec<Complex32>,
    history: [[f32; WINDOW]; CHANNELS],
    feature_history: [[f32; INPUT]; CHANNELS],
    overlap: [[f32; WINDOW]; CHANNELS],
    window: [f32; WINDOW],
    peak_guard_chunks: usize,
    limiter_gain: f32,
}

impl Default for StreamingSr {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamingSr {
    pub fn new() -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(WINDOW);
        let inverse = planner.plan_fft_inverse(WINDOW);
        let forward_scratch = vec![Complex32::new(0.0, 0.0); forward.get_inplace_scratch_len()];
        let inverse_scratch = vec![Complex32::new(0.0, 0.0); inverse.get_inplace_scratch_len()];
        let mut window = [0.0; WINDOW];
        for (index, value) in window.iter_mut().enumerate() {
            *value = 0.5 - 0.5 * (2.0 * PI * index as f32 / WINDOW as f32).cos();
        }
        Self {
            model: Model::embedded(),
            forward,
            inverse,
            forward_scratch,
            inverse_scratch,
            spectrum: vec![Complex32::new(0.0, 0.0); WINDOW],
            history: [[0.0; WINDOW]; CHANNELS],
            feature_history: [[0.0; INPUT]; CHANNELS],
            overlap: [[0.0; WINDOW]; CHANNELS],
            window,
            peak_guard_chunks: 0,
            limiter_gain: 1.0,
        }
    }

    /// `enabled=false` and `FullBandBypass` retain the same 768-frame delay as completion.
    pub fn process(
        &mut self,
        input: &[f32],
        output: &mut [f32],
        enabled: bool,
        bandwidth: Bandwidth,
    ) -> Result<(), ProcessError> {
        let completion_mix = if enabled && bandwidth == Bandwidth::BandLimited {
            1.0
        } else {
            0.0
        };
        self.process_with_mix(input, output, completion_mix)
    }

    /// Process one chunk with a continuously variable completion mix.
    ///
    /// A mix of `0.0` is the fixed-delay bypass and `1.0` is equivalent to
    /// `process(..., true, Bandwidth::BandLimited)`. Values up to `2.0`
    /// extrapolate the completed high band. Values outside `0.0..=2.0`
    /// and non-finite values are clamped to a safe bypass/completion range.
    pub fn process_with_mix(
        &mut self,
        input: &[f32],
        output: &mut [f32],
        completion_mix: f32,
    ) -> Result<(), ProcessError> {
        if input.len() != CHUNK_FRAMES * CHANNELS || output.len() != input.len() {
            return Err(ProcessError::WrongChunkSize);
        }
        let completion_mix = if completion_mix.is_finite() {
            completion_mix.clamp(0.0, 2.0)
        } else {
            0.0
        };
        if completion_mix > 1.0 {
            self.peak_guard_chunks = (WINDOW - CHUNK_FRAMES) / CHUNK_FRAMES;
        }
        for channel in 0..CHANNELS {
            self.history[channel].copy_within(CHUNK_FRAMES..WINDOW, 0);
            for frame in 0..CHUNK_FRAMES {
                self.history[channel][WINDOW - CHUNK_FRAMES + frame] =
                    input[frame * CHANNELS + channel];
            }
            for i in 0..WINDOW {
                self.spectrum[i] = Complex32::new(self.history[channel][i] * self.window[i], 0.0);
            }
            self.forward
                .process_with_scratch(&mut self.spectrum, &mut self.forward_scratch);
            self.complete_high_band(channel, completion_mix);
            self.inverse
                .process_with_scratch(&mut self.spectrum, &mut self.inverse_scratch);
            for i in 0..WINDOW {
                self.overlap[channel][i] +=
                    self.spectrum[i].re * self.window[i] / (WINDOW as f32 * 1.5);
            }
        }
        // The current output chunk is already buffered. One gain for both
        // channels preserves the waveform and stereo balance without flat tops.
        let protect_peak =
            completion_mix > 1.0 || self.peak_guard_chunks > 0 || self.limiter_gain < 1.0;
        if protect_peak {
            let peak = self
                .overlap
                .iter()
                .flat_map(|channel| &channel[..CHUNK_FRAMES])
                .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
            let safe_gain = if peak > 1.0 {
                (1.0 - f32::EPSILON) / peak
            } else {
                1.0
            };
            self.limiter_gain =
                safe_gain.min((self.limiter_gain + LIMITER_RELEASE_PER_CHUNK).min(1.0));
        }
        for channel in 0..CHANNELS {
            for frame in 0..CHUNK_FRAMES {
                let sample = self.overlap[channel][frame];
                output[frame * CHANNELS + channel] = if protect_peak {
                    sample * self.limiter_gain
                } else {
                    sample
                };
            }
            self.overlap[channel].copy_within(CHUNK_FRAMES..WINDOW, 0);
            self.overlap[channel][WINDOW - CHUNK_FRAMES..].fill(0.0);
        }
        if completion_mix <= 1.0 {
            self.peak_guard_chunks = self.peak_guard_chunks.saturating_sub(1);
        }
        Ok(())
    }

    fn complete_high_band(&mut self, channel: usize, completion_mix: f32) {
        let mut scale = 0.0;
        for bin in 0..LOW {
            scale += self.spectrum[bin].norm();
        }
        scale /= LOW as f32;
        let original_scale = scale;
        scale = scale.max(0.05);
        self.feature_history[channel].copy_within(POOLED..INPUT, 0);
        for pooled in 0..POOLED {
            let first = (self.spectrum[1 + 2 * pooled].norm() / scale).ln_1p();
            let second = (self.spectrum[2 + 2 * pooled].norm() / scale).ln_1p();
            self.feature_history[channel][INPUT - POOLED + pooled] = 0.5 * (first + second);
        }
        if completion_mix <= 0.0 || original_scale < 0.0001 {
            return;
        }
        let mut prediction = [0.0; OUT];
        self.model
            .predict(&self.feature_history[channel], &mut prediction);
        let low_energy: f32 = self.spectrum[..LOW].iter().map(Complex32::norm_sqr).sum();
        let magnitudes = prediction.map(|value| value.exp_m1() * scale);
        let predicted_energy: f32 = magnitudes.iter().map(|value| value * value).sum();
        let energy_gain = if predicted_energy > 0.0 {
            (0.05 * low_energy / predicted_energy).sqrt().min(1.0)
        } else {
            1.0
        };
        let high_peak_bound = 8.0 * magnitudes.iter().sum::<f32>() / (WINDOW as f32 * 1.5);
        let input_peak = self.history[channel]
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        let peak_gain = if high_peak_bound > 0.0 {
            ((1.0 - input_peak).max(0.0) / high_peak_bound).min(1.0)
        } else {
            1.0
        };
        let gain = energy_gain.min(peak_gain);
        for (index, bin) in (HIGH_START..HIGH_END).enumerate() {
            let source = (bin / 2).max(1);
            let reference = if bin % 2 == 1 {
                (self.spectrum[source] + self.spectrum[source + 1]) * 0.5
            } else {
                self.spectrum[source]
            };
            let basis = if reference.norm() > 1e-6 {
                Complex32::from_polar(1.0, reference.arg() * 2.0)
            } else {
                Complex32::new(1.0, 0.0)
            };
            let generated = basis * (magnitudes[index] * gain);
            let value = self.spectrum[bin] * (1.0 - completion_mix) + generated * completion_mix;
            self.spectrum[bin] = value;
            self.spectrum[WINDOW - bin] = value.conj();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_output_matches_training_runtime() {
        let model = Model::embedded();
        let mut features = [0.0; INPUT];
        for (index, value) in features.iter_mut().enumerate() {
            *value = (index % 17) as f32 * 0.03;
        }
        let mut prediction = [0.0; OUT];
        model.predict(&features, &mut prediction);
        let reference = [
            0.07687059,
            0.082454465,
            0.07752606,
            0.07431271,
            0.07899727,
            0.07415102,
        ];
        for (actual, expected) in prediction.iter().zip(reference) {
            assert!((actual - expected).abs() < 0.00001);
        }
        assert!((prediction.iter().sum::<f32>() - 11.127815).abs() < 0.001);
    }

    #[test]
    fn bypass_preserves_stereo_with_fixed_delay() {
        for (enabled, bandwidth) in [
            (false, Bandwidth::BandLimited),
            (true, Bandwidth::FullBandBypass),
        ] {
            let mut engine = StreamingSr::new();
            for chunk in 0..12 {
                let mut input = [0.0; CHUNK_FRAMES * 2];
                let mut output = [0.0; CHUNK_FRAMES * 2];
                for frame in 0..CHUNK_FRAMES {
                    input[frame * 2] = ((chunk * CHUNK_FRAMES + frame) as f32 * 0.013).sin() * 0.3;
                    input[frame * 2 + 1] =
                        ((chunk * CHUNK_FRAMES + frame) as f32 * 0.033).cos() * 0.2;
                }
                engine
                    .process(&input, &mut output, enabled, bandwidth)
                    .unwrap();
                if chunk >= 3 {
                    for frame in 0..CHUNK_FRAMES {
                        let index = (chunk - 3) * CHUNK_FRAMES + frame;
                        assert!(
                            (output[frame * 2] - (index as f32 * 0.013).sin() * 0.3).abs() < 0.0001
                        );
                        assert!(
                            (output[frame * 2 + 1] - (index as f32 * 0.033).cos() * 0.2).abs()
                                < 0.0001
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn trained_path_changes_bandlimited_audio() {
        let mut on = StreamingSr::new();
        let mut off = StreamingSr::new();
        let mut total_difference = 0.0;
        for chunk in 0..12 {
            let mut input = [0.0; CHUNK_FRAMES * 2];
            let mut processed = [0.0; CHUNK_FRAMES * 2];
            let mut passthrough = [0.0; CHUNK_FRAMES * 2];
            for frame in 0..CHUNK_FRAMES {
                let sample = ((chunk * CHUNK_FRAMES + frame) as f32 * 2.0 * PI * 2000.0
                    / SAMPLE_RATE as f32)
                    .sin()
                    * 0.2;
                input[frame * 2] = sample;
                input[frame * 2 + 1] = sample;
            }
            on.process(&input, &mut processed, true, Bandwidth::BandLimited)
                .unwrap();
            off.process(&input, &mut passthrough, false, Bandwidth::BandLimited)
                .unwrap();
            total_difference += processed
                .iter()
                .zip(passthrough)
                .map(|(a, b)| (a - b).abs())
                .sum::<f32>();
        }
        assert!(
            total_difference > 1e-6,
            "the completion path must affect audio"
        );
    }

    #[test]
    fn enabled_path_preserves_existing_low_tone() {
        let mut engine = StreamingSr::new();
        let mut expected_cos = 0.0_f64;
        let mut expected_sin = 0.0_f64;
        let mut actual_cos = 0.0_f64;
        let mut actual_sin = 0.0_f64;
        for chunk in 0..24 {
            let mut input = [0.0; CHUNK_FRAMES * CHANNELS];
            let mut output = [0.0; CHUNK_FRAMES * CHANNELS];
            for frame in 0..CHUNK_FRAMES {
                let index = chunk * CHUNK_FRAMES + frame;
                let sample = (2.0 * PI * 3000.0 * index as f32 / SAMPLE_RATE as f32).sin() * 0.2;
                input[frame * CHANNELS] = sample;
                input[frame * CHANNELS + 1] = sample;
            }
            engine
                .process(&input, &mut output, true, Bandwidth::BandLimited)
                .unwrap();
            if chunk >= 3 {
                for frame in 0..CHUNK_FRAMES {
                    let index = (chunk - 3) * CHUNK_FRAMES + frame;
                    let angle =
                        2.0 * std::f64::consts::PI * 3000.0 * index as f64 / SAMPLE_RATE as f64;
                    let reference = angle.sin() * 0.2;
                    expected_cos += reference * angle.cos();
                    expected_sin += reference * angle.sin();
                    actual_cos += output[frame * CHANNELS] as f64 * angle.cos();
                    actual_sin += output[frame * CHANNELS] as f64 * angle.sin();
                }
            }
        }
        let expected = (expected_cos * expected_cos + expected_sin * expected_sin).sqrt();
        let actual = (actual_cos * actual_cos + actual_sin * actual_sin).sqrt();
        assert!(
            (actual / expected - 1.0).abs() < 0.01,
            "low-band tone level changed"
        );
    }

    #[test]
    fn completion_keeps_stereo_channels_independent() {
        let mut engine = StreamingSr::new();
        for chunk in 0..8 {
            let mut input = [0.0; CHUNK_FRAMES * CHANNELS];
            let mut output = [0.0; CHUNK_FRAMES * CHANNELS];
            for frame in 0..CHUNK_FRAMES {
                let index = chunk * CHUNK_FRAMES + frame;
                input[frame * CHANNELS] =
                    (2.0 * PI * 2500.0 * index as f32 / SAMPLE_RATE as f32).sin() * 0.2;
            }
            engine
                .process(&input, &mut output, true, Bandwidth::BandLimited)
                .unwrap();
            assert!(output
                .iter()
                .skip(1)
                .step_by(CHANNELS)
                .all(|sample| sample.abs() < 1e-7));
        }
    }

    #[test]
    fn on_off_and_fullband_bypass_are_callable() {
        let mut engine = StreamingSr::new();
        let input = [0.0; CHUNK_FRAMES * 2];
        let mut output = [0.0; CHUNK_FRAMES * 2];
        engine
            .process(&input, &mut output, true, Bandwidth::BandLimited)
            .unwrap();
        engine
            .process(&input, &mut output, true, Bandwidth::FullBandBypass)
            .unwrap();
        engine
            .process(&input, &mut output, false, Bandwidth::BandLimited)
            .unwrap();
        engine.process_with_mix(&input, &mut output, 0.5).unwrap();
        assert_eq!(
            engine.process(&input[..1], &mut output, true, Bandwidth::BandLimited),
            Err(ProcessError::WrongChunkSize)
        );
    }

    #[test]
    fn continuous_mix_endpoints_match_existing_api() {
        let mut old_on = StreamingSr::new();
        let mut mixed_on = StreamingSr::new();
        let mut old_off = StreamingSr::new();
        let mut mixed_off = StreamingSr::new();
        for chunk in 0..8 {
            let mut input = [0.0; CHUNK_FRAMES * CHANNELS];
            let mut a = [0.0; CHUNK_FRAMES * CHANNELS];
            let mut b = [0.0; CHUNK_FRAMES * CHANNELS];
            let mut c = [0.0; CHUNK_FRAMES * CHANNELS];
            let mut d = [0.0; CHUNK_FRAMES * CHANNELS];
            for frame in 0..CHUNK_FRAMES {
                let index = chunk * CHUNK_FRAMES + frame;
                input[frame * CHANNELS] =
                    (2.0 * PI * 2_000.0 * index as f32 / SAMPLE_RATE as f32).sin() * 0.2;
                input[frame * CHANNELS + 1] = input[frame * CHANNELS];
            }
            old_on
                .process(&input, &mut a, true, Bandwidth::BandLimited)
                .unwrap();
            mixed_on.process_with_mix(&input, &mut b, 1.0).unwrap();
            old_off
                .process(&input, &mut c, false, Bandwidth::BandLimited)
                .unwrap();
            mixed_off.process_with_mix(&input, &mut d, 0.0).unwrap();
            assert_eq!(a, b);
            assert_eq!(c, d);
        }
    }

    #[test]
    fn peak_guard_covers_the_overlap_tail_after_leaving_boosted_mix() {
        let mut engine = StreamingSr::new();
        let mut output = [0.0; CHUNK_FRAMES * CHANNELS];
        for chunk in 0..8 {
            let input: [f32; CHUNK_FRAMES * CHANNELS] = std::array::from_fn(|index| {
                let frame = chunk * CHUNK_FRAMES + index / CHANNELS;
                (2.0 * PI * 3_000.0 * frame as f32 / SAMPLE_RATE as f32).sin() * 8.0
            });
            engine.process_with_mix(&input, &mut output, 2.0).unwrap();
            assert!(output.iter().all(|sample| sample.abs() <= 1.0));
        }
        for _ in 0..3 {
            engine
                .process_with_mix(&[0.0; CHUNK_FRAMES * CHANNELS], &mut output, 0.0)
                .unwrap();
            assert!(output.iter().all(|sample| sample.abs() <= 1.0));
        }
    }

    #[test]
    fn boosted_high_level_transient_is_limited_without_flat_topping() {
        let mut engine = StreamingSr::new();
        let mut output = [0.0; CHUNK_FRAMES * CHANNELS];
        let mut gain_reduction_seen = false;
        let mut loud_samples = 0;
        let mut flat_triples = 0;
        let mut peak = 0.0_f32;
        for chunk in 0..12 {
            let input: [f32; CHUNK_FRAMES * CHANNELS] = std::array::from_fn(|index| {
                if chunk == 1 {
                    (2.0 * PI * 3_000.0 * (index / CHANNELS) as f32 / SAMPLE_RATE as f32).sin()
                        * 8.0
                } else {
                    0.0
                }
            });
            engine.process_with_mix(&input, &mut output, 2.0).unwrap();
            gain_reduction_seen |= engine.limiter_gain < 0.99;
            assert!(output.iter().all(|sample| sample.abs() <= 1.0));
            let left = output.iter().step_by(CHANNELS).copied().collect::<Vec<_>>();
            for sample in &left {
                peak = peak.max(sample.abs());
                loud_samples += usize::from(sample.abs() > 0.8);
            }
            flat_triples += left
                .windows(3)
                .filter(|samples| {
                    samples.iter().all(|sample| sample.abs() > 0.8)
                        && (samples[0] - samples[1]).abs() < 1e-6
                        && (samples[1] - samples[2]).abs() < 1e-6
                })
                .count();
        }
        println!(
            "transient_peak={peak:.9} loud_samples={loud_samples} flat_triples={flat_triples} gain_reduction_seen={gain_reduction_seen}"
        );
        assert!(gain_reduction_seen);
        assert!(loud_samples > 10, "transient must exercise the limiter");
        assert!(peak > 0.9 && peak < 1.0, "peak={peak}");
        assert_eq!(flat_triples, 0, "limiter must not flatten the transient");
    }
}
