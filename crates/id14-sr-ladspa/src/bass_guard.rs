//! Preserve the delayed source bass and stereo image, filtering only SR's residual.
//!
//! Implements realtime_sr.bass_completion_filter_improvement_criterion. The pin
//! leaves the method/band free: four identical bilinear first-order high-pass
//! sections at 300 Hz suppress low-frequency changes made by completion. Both
//! channels use identical coefficients but independent history; no mono summing,
//! lookahead, allocation, or extra frame delay occurs in process. A stereo-linked
//! instantaneous ceiling bounds reconstructed samples at full scale.
//! This protects source bass; it does not claim to repair defects in the source.

const DELAY_FRAMES: usize = 1024; // Adapter + engine delay, supplied by the seam.
const RATE: f64 = 48_000.0;
const CORNER_HZ: f64 = 300.0;

#[derive(Clone, Copy, Default)]
struct Section {
    input: f64,
    output: f64,
}

pub(super) struct BassGuard {
    dry: [[f32; 2]; DELAY_FRAMES],
    cursor: usize,
    sections: [[Section; 4]; 2],
    feed: f64,
    feedback: f64,
    #[cfg(test)]
    bypass: bool,
}

impl BassGuard {
    pub(super) fn new() -> Self {
        let k = (std::f64::consts::PI * CORNER_HZ / RATE).tan();
        Self {
            dry: [[0.0; 2]; DELAY_FRAMES],
            cursor: 0,
            sections: [[Section::default(); 4]; 2],
            feed: 1.0 / (1.0 + k),
            feedback: (1.0 - k) / (1.0 + k),
            #[cfg(test)]
            bypass: false,
        }
    }

    pub(super) fn process(&mut self, input: [f32; 2], wet: [f32; 2], amount: f32) -> [f32; 2] {
        let dry = self.dry[self.cursor];
        self.dry[self.cursor] = input.map(|x| if x.is_finite() { x } else { 0.0 });
        self.cursor = (self.cursor + 1) % DELAY_FRAMES;
        let mut output = [0.0_f64; 2];
        for channel in 0..2 {
            let wet = if wet[channel].is_finite() {
                wet[channel]
            } else {
                dry[channel]
            };
            let mut residual = f64::from(wet) - f64::from(dry[channel]);
            for section in &mut self.sections[channel] {
                let next = self.feed * (residual - section.input) + self.feedback * section.output;
                section.input = residual;
                section.output = next;
                residual = next;
            }
            output[channel] = f64::from(wet)
                + f64::from(amount) * (f64::from(dry[channel]) + residual - f64::from(wet));
        }
        // Continue tracking during bypass so enabling cannot expose stale state.
        // Preserve the existing engine's exact zero-mix output (including rounding).
        if amount == 0.0 {
            return wet;
        }
        #[cfg(test)]
        if self.bypass {
            return wet;
        }
        let peak = output[0].abs().max(output[1].abs()).max(1.0);
        output.map(|value| (value / peak) as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Adapter;
    use std::f64::consts::TAU;

    const FRAMES: usize = 48_000;
    const BINS: [f64; 5] = [60.0, 90.0, 120.0, 180.0, 240.0];

    fn tone(frame: usize, hz: f64, phase: f64) -> f64 {
        (TAU * hz * frame as f64 / RATE + phase).sin()
    }

    fn source(frame: usize) -> [f32; 2] {
        let mid = 0.14 * tone(frame, 60.0, 0.0) + 0.09 * tone(frame, 120.0, 0.2);
        let side = 0.07 * tone(frame, 60.0, 0.7) + 0.06 * tone(frame, 240.0, 0.3);
        let high = 0.05 * tone(frame, 6_000.0, 0.0);
        [(mid + side + high) as f32, (mid - side + high) as f32]
    }

    // Complex amplitudes over a half second: every test frequency occupies an
    // integer number of periods. Discard startup; include phase, not only RMS.
    fn spectrum(signal: &[[f32; 2]]) -> Vec<[[f64; 2]; 2]> {
        BINS.iter()
            .map(|&hz| {
                let mut bins = [[0.0; 2]; 2];
                for (frame, sample) in signal.iter().enumerate().skip(FRAMES / 2).take(FRAMES / 2) {
                    let angle = TAU * hz * frame as f64 / RATE;
                    for channel in 0..2 {
                        bins[channel][0] += f64::from(sample[channel]) * angle.cos();
                        bins[channel][1] += f64::from(sample[channel]) * angle.sin();
                    }
                }
                for channel in &mut bins {
                    for component in channel {
                        *component *= 4.0 / FRAMES as f64;
                    }
                }
                bins
            })
            .collect()
    }

    fn metrics(reference: &[[f32; 2]], signal: &[[f32; 2]]) -> (f64, f64) {
        let reference = spectrum(reference);
        let actual = spectrum(signal);
        let mut error = 0.0;
        let mut power = 0.0;
        let width = |bins: &Vec<[[f64; 2]; 2]>| {
            let mut mid = 0.0;
            let mut side = 0.0;
            for bin in bins {
                for component in 0..2 {
                    mid += (bin[0][component] + bin[1][component]).powi(2);
                    side += (bin[0][component] - bin[1][component]).powi(2);
                }
            }
            (side / mid).sqrt()
        };
        for (expected, got) in reference.iter().zip(&actual) {
            for channel in 0..2 {
                for component in 0..2 {
                    error += (got[channel][component] - expected[channel][component]).powi(2);
                    power += expected[channel][component].powi(2);
                }
            }
        }
        (
            (error / power).sqrt(),
            (width(&actual) - width(&reference)).abs(),
        )
    }

    #[test]
    fn bass_criterion_known_contamination() {
        let reference: Vec<_> = (0..FRAMES).map(source).collect();
        for mix in [0.0, 100.0, 150.0, 200.0] {
            let mut guard = BassGuard::new();
            let mut before = Vec::new();
            let mut after = Vec::new();
            for frame in 0..FRAMES + DELAY_FRAMES {
                let input = if frame < FRAMES {
                    source(frame)
                } else {
                    [0.0; 2]
                };
                let mut wet = [0.0; 2];
                if frame >= DELAY_FRAMES {
                    let t = frame - DELAY_FRAMES;
                    let dry = source(t);
                    let dirt = mix / 100.0 * 0.04 * tone(t, 90.0, 0.1);
                    // Add unrelated bass and narrow the intended stereo bass.
                    wet = [
                        (f64::from(dry[0]) + dirt) as f32,
                        (f64::from(dry[1]) + dirt + mix / 100.0 * 0.25 * f64::from(dry[0] - dry[1]))
                            as f32,
                    ];
                }
                let output = guard.process(input, wet, if mix == 0.0 { 0.0 } else { 1.0 });
                if frame >= DELAY_FRAMES {
                    before.push(wet);
                    after.push(output);
                }
            }
            let pre = metrics(&reference, &before);
            let post = metrics(&reference, &after);
            println!(
                "synthetic mix={mix:.0} bass_error={:.9}->{:.9} width_error={:.9}->{:.9}",
                pre.0, post.0, pre.1, post.1
            );
            if mix == 0.0 {
                assert_eq!(before, after);
            } else {
                assert!(post.0 < pre.0 * 0.1, "low-band error: {pre:?} -> {post:?}");
                assert!(post.1 < pre.1 * 0.1, "width error: {pre:?} -> {post:?}");
            }
        }
    }

    fn render(mix: f32, bypass: bool, block: usize) -> Vec<[f32; 2]> {
        let mut adapter = Adapter::new(RATE as usize);
        adapter.bass_guard.bypass = bypass;
        let input: Vec<_> = (0..FRAMES + DELAY_FRAMES)
            .map(|t| if t < FRAMES { source(t) } else { [0.0; 2] })
            .collect();
        let mut result = Vec::new();
        for chunk in input.chunks(block) {
            let left: Vec<_> = chunk.iter().map(|s| s[0]).collect();
            let right: Vec<_> = chunk.iter().map(|s| s[1]).collect();
            let mut out_l = vec![0.0; chunk.len()];
            let mut out_r = vec![0.0; chunk.len()];
            adapter.process_with_user_mix(&left, &right, &mut out_l, &mut out_r, mix);
            result.extend(out_l.into_iter().zip(out_r).map(|(l, r)| [l, r]));
        }
        result
    }

    #[test]
    fn bass_criterion_adapter_all_mixes() {
        let reference: Vec<_> = (0..FRAMES).map(source).collect();
        for mix in [0.0, 100.0, 150.0, 200.0] {
            let before = render(mix, true, 137);
            let after = render(mix, false, 137);
            let pre = metrics(&reference, &before[DELAY_FRAMES..]);
            let post = metrics(&reference, &after[DELAY_FRAMES..]);
            println!(
                "adapter mix={mix:.0} bass_error={:.9}->{:.9} width_error={:.9}->{:.9}",
                pre.0, post.0, pre.1, post.1
            );
            assert!(
                post.0 <= pre.0 * 1.001 + 1e-7,
                "bass error: {pre:?} -> {post:?}"
            );
            assert!(post.1 <= pre.1 + 1e-7, "width error: {pre:?} -> {post:?}");
            assert!(after.iter().flatten().all(|v| v.is_finite()));
            if mix == 0.0 {
                assert!(before == after, "dry mix must be unchanged");
            }
        }
    }

    #[test]
    fn adapter_block_partition_does_not_change_output() {
        assert_eq!(render(200.0, false, 1), render(200.0, false, 511));
    }

    #[test]
    fn adapter_dry_impulse_retains_1024_frame_delay() {
        let mut adapter = Adapter::new(RATE as usize);
        let mut left = vec![0.0; 4096];
        let mut right = left.clone();
        left[0] = 0.25;
        right[0] = -0.125;
        let mut out_l = vec![0.0; left.len()];
        let mut out_r = out_l.clone();
        adapter.process_with_user_mix(&left, &right, &mut out_l, &mut out_r, 0.0);
        assert_eq!(out_l.iter().position(|&v| v != 0.0), Some(DELAY_FRAMES));
        assert_eq!(out_l[DELAY_FRAMES], left[0]);
        assert_eq!(out_r[DELAY_FRAMES], right[0]);
        assert_eq!(out_l.iter().filter(|&&v| v != 0.0).count(), 1);
        assert_eq!(out_r.iter().filter(|&&v| v != 0.0).count(), 1);
        println!(
            "adapter delay_frames={DELAY_FRAMES} delay_ms={:.6} added_guard_frames=0",
            DELAY_FRAMES as f64 / RATE * 1000.0
        );
    }

    #[test]
    fn high_frequency_residual_survives_and_nonfinite_input_recovers() {
        let mut guard = BassGuard::new();
        guard.process(
            [f32::NAN, f32::INFINITY],
            [f32::NAN, f32::NEG_INFINITY],
            1.0,
        );
        let mut before = 0.0;
        let mut after = 0.0;
        for t in 0..FRAMES {
            let wet = (0.2 * tone(t, 8_000.0, 0.0)) as f32;
            let output = guard.process([0.0; 2], [wet, -wet], 1.0);
            assert!(output.iter().all(|v| v.is_finite()));
            assert_eq!(output[0], -output[1]);
            if t > FRAMES / 2 {
                before += f64::from(wet).powi(2);
                after += f64::from(output[0]).powi(2);
            }
        }
        assert!((after / before).sqrt() > 0.99);
    }
}
