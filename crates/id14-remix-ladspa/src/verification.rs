//! DSP-level tests use a local mask provider, not a substitute for ONNX contract tests.
use crate::dsp::*;
use crate::engine::*;
use std::f64::consts::PI;
use std::time::{Duration, Instant};

thread_local! {
    static CALLBACK_ALLOCS: std::cell::Cell<(bool, usize)> = const { std::cell::Cell::new((false, 0)) };
}
struct TrackingAllocator;
// Track only the current test's callback thread; worker inference is allowed to allocate.
unsafe impl std::alloc::GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        let _ = CALLBACK_ALLOCS.try_with(|c| {
            let (active, count) = c.get();
            if active {
                c.set((true, count + 1));
            }
        });
        std::alloc::GlobalAlloc::alloc(&std::alloc::System, layout)
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: std::alloc::Layout) {
        std::alloc::GlobalAlloc::dealloc(&std::alloc::System, pointer, layout)
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: std::alloc::Layout, size: usize) -> *mut u8 {
        let _ = CALLBACK_ALLOCS.try_with(|c| {
            let (active, count) = c.get();
            if active {
                c.set((true, count + 1));
            }
        });
        std::alloc::GlobalAlloc::realloc(&std::alloc::System, pointer, layout, size)
    }
}
#[global_allocator]
static CALLBACK_ALLOCATOR: TrackingAllocator = TrackingAllocator;

struct VoiceMask;
impl SeparationModel for VoiceMask {
    fn lookahead(&self) -> usize {
        0
    }
    fn reset(&mut self) {}
    fn infer(&mut self, _: &[f32; 4 * BINS]) -> Result<[f32; 4 * BINS], Status> {
        let mut mask = [0.0; 4 * BINS];
        mask[..BINS].fill(1.0);
        Ok(mask)
    }
}
fn engine() -> Engine {
    let engine = Engine::new(|| Ok(Box::new(VoiceMask)));
    let deadline = Instant::now() + Duration::from_secs(2);
    while engine.model_status() == Status::Loading {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_micros(100));
    }
    assert_eq!(engine.model_status(), Status::Active);
    engine
}
fn process(engine: &mut Engine, input: &[Stereo], paced: bool) -> (Vec<Stereo>, Vec<f64>) {
    let mut result = vec![[0.0; 2]; input.len()];
    let mut durations = Vec::new();
    for (x, y) in input.chunks(256).zip(result.chunks_mut(256)) {
        let start = Instant::now();
        engine.process(x, y);
        durations.push(start.elapsed().as_secs_f64() * 1e6);
        if paced {
            std::thread::sleep(Duration::from_micros(300));
        }
    }
    (result, durations)
}
fn noise(n: usize) -> Vec<Stereo> {
    let mut state = 1234567_u32;
    (0..n)
        .map(|i| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let x = (state as i32 as f64 / i32::MAX as f64 * 0.1) as f32;
            if i % 17 == 0 {
                [-0.0, f32::from_bits(1)]
            } else {
                [x, -x * 0.375]
            }
        })
        .collect()
}
fn tone(n: usize, hz: f64, amplitude: f64) -> Vec<Stereo> {
    (0..n)
        .map(|i| {
            let x = (amplitude * (2.0 * PI * hz * i as f64 / RATE as f64).sin()) as f32;
            [x, x * 0.5]
        })
        .collect()
}
fn projection(x: &[Stereo], hz: f64, ch: usize) -> f64 {
    let (mut re, mut im) = (0.0, 0.0);
    for (i, sample) in x.iter().enumerate() {
        let angle = 2.0 * PI * hz * i as f64 / RATE as f64;
        re += sample[ch] as f64 * angle.cos();
        im += sample[ch] as f64 * angle.sin();
    }
    2.0 * re.hypot(im) / x.len() as f64
}

#[test]
fn exact_dry_paths_and_measured_delay() {
    let input = noise(24_576);
    for mode in ["missing", "off", "zero"] {
        let mut e = if mode == "missing" {
            Engine::new(|| Err(Status::ModelMissing))
        } else {
            engine()
        };
        if mode == "off" {
            e.set_controls(Controls {
                enabled: false,
                ..Controls::default()
            });
        }
        if mode == "zero" {
            e.set_controls(Controls {
                enabled: true,
                db: [0.0; 4],
            });
        }
        let (output, _) = process(&mut e, &input, false);
        let mismatches = output[LATENCY..]
            .iter()
            .zip(&input)
            .filter(|(y, x)| y[0].to_bits() != x[0].to_bits() || y[1].to_bits() != x[1].to_bits())
            .count();
        let measured = (LATENCY - 32..=LATENCY + 32)
            .max_by(|a, b| {
                let corr = |lag: usize| -> f64 {
                    (0..8192)
                        .map(|i| input[i][0] as f64 * output[i + lag][0] as f64)
                        .sum()
                };
                corr(*a).total_cmp(&corr(*b))
            })
            .unwrap();
        println!("dry mode={mode} bit_mismatches={mismatches} reported={} cross_correlation_lag={measured} sr_inclusive_ms={:.3}", e.latency_frames(), 1000.0 * (LATENCY+SR_LATENCY) as f64 / RATE as f64);
        assert_eq!(mismatches, 0);
        assert_eq!(measured, e.latency_frames());
    }
}

#[test]
fn active_gain_stereo_and_callback_cost() {
    let mut e = engine();
    // A single source must return to its input loudness after the slow matcher
    // settles. A permanent +3 dB expectation would contradict loudness fairness.
    let input = tone(RATE * 10, 3000.0, 0.05);
    let (output, mut us) = process(&mut e, &input, true);
    let from = LATENCY + RATE * 8;
    let rms_error = output[from..]
        .iter()
        .map(|y| (y[1] as f64 - 0.5 * y[0] as f64).powi(2))
        .sum::<f64>()
        .sqrt();
    let measured_gain = projection(&output[from..from + RATE], 3000.0, 0) / 0.05;
    us.sort_by(f64::total_cmp);
    println!("active gain_3000_hz={measured_gain:.9} target=1 loudness_delta_db={:.6} stereo_ratio_error={rms_error:e} callback_256_us_p50={:.3} p99={:.3} max={:.3} final_status={:?}", 20.0*measured_gain.log10(), us[us.len()/2], us[us.len()*99/100], us[us.len()-1], e.status());
    assert!(rms_error < 1e-6);
    assert!((20.0 * measured_gain.log10()).abs() <= 0.5);
    assert_eq!(e.status(), Status::Active);
}

#[test]
fn low_band_retention() {
    for hz in [50.0, 100.0, 200.0, 299.0] {
        let mut e = engine();
        let input = tone(RATE * 2, hz, 0.1);
        let (output, _) = process(&mut e, &input, true);
        let start = LATENCY + RATE / 2;
        let error: Vec<_> = (start..start + RATE)
            .map(|i| {
                [
                    output[i][0] - input[i - LATENCY][0],
                    output[i][1] - input[i - LATENCY][1],
                ]
            })
            .collect();
        let difference = projection(&error, hz, 0);
        let relative_db = 20.0 * (difference / 0.1).max(1e-30).log10();
        let all_error_rms =
            (error.iter().map(|x| (x[0] as f64).powi(2)).sum::<f64>() / RATE as f64).sqrt();
        println!(
            "bass hz={hz} difference_relative_db={relative_db:.6} error_rms_dbfs={:.6}",
            20.0 * all_error_rms.max(1e-30).log10()
        );
        assert!(relative_db <= -60.0);
        assert!(all_error_rms < 0.001);
    }
}

#[test]
fn silence_adds_no_signal() {
    let mut e = engine();
    let (output, _) = process(&mut e, &vec![[0.0; 2]; RATE], true);
    let nonzero = output.iter().flatten().filter(|x| x.to_bits() != 0).count();
    println!("silent_output_nonzero_samples={nonzero}");
    assert_eq!(nonzero, 0);
}

#[test]
fn loudness_matching_callback_allocates_nothing() {
    let mut e = engine();
    let mut output = [[0.0; 2]; 256];
    let input = std::array::from_fn::<_, 256, _>(|n| {
        let value = (0.05 * (2.0 * PI * 3000.0 * n as f64 / RATE as f64).sin()) as f32;
        [value, value * 0.7]
    });
    for _ in 0..400 {
        CALLBACK_ALLOCS.with(|c| c.set((true, c.get().1)));
        e.process(&input, &mut output);
        CALLBACK_ALLOCS.with(|c| c.set((false, c.get().1)));
        std::thread::sleep(Duration::from_micros(300));
    }
    let count = CALLBACK_ALLOCS.with(|c| c.get().1);
    println!(
        "callback_allocations={count} loudness_gain_db={}",
        e.loudness_gain_db()
    );
    assert_eq!(count, 0);
    assert!(e.loudness_gain_db() < -0.1, "exercise the active matcher");
}

#[test]
fn stalled_worker_and_switches_fade_current_audio_to_dry() {
    for event in ["worker_pause", "off", "zero"] {
        let mut e = engine();
        let input: Vec<Stereo> = (0..RATE * 2)
            .map(|n| {
                let phase = 2.0 * PI * 3000.0 * n as f64 / RATE as f64;
                [(0.1 * phase.sin()) as f32, (0.1 * phase.cos()) as f32]
            })
            .collect();
        let (mut output, _) = process(&mut e, &input[..RATE], true);
        match event {
            "worker_pause" => e.pause_worker(true),
            "off" => e.set_controls(Controls {
                enabled: false,
                ..Controls::default()
            }),
            _ => e.set_controls(Controls {
                enabled: true,
                db: [0.0; 4],
            }),
        }
        let (tail, _) = process(&mut e, &input[RATE..], true);
        output.extend(tail);
        let amplitude: Vec<f64> = output[RATE - 1024..RATE + 8192]
            .iter()
            .map(|x| (x[0] as f64).hypot(x[1] as f64))
            .collect();
        let max_step = amplitude
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0, f64::max);
        let minimum = amplitude.iter().copied().fold(f64::INFINITY, f64::min);
        let settle = (RATE..RATE + 8192)
            .find(|start| {
                (*start..*start + 1024)
                    .all(|n| output[n].map(f32::to_bits) == input[n - LATENCY].map(f32::to_bits))
            })
            .unwrap_or(RATE + 8192);
        let final_mismatches = (RATE + 8192..RATE * 2)
            .filter(|n| output[*n].map(f32::to_bits) != input[*n - LATENCY].map(f32::to_bits))
            .count();
        println!("transition event={event} max_envelope_sample_step={max_step:e} min_amplitude={minimum:.9} settle_ms={:.3} final_bit_mismatches={final_mismatches} state={:?}", 1000.0*(settle-RATE) as f64/RATE as f64, e.status());
        assert!(max_step < 0.0005);
        assert!(minimum >= 0.099);
        assert_eq!(final_mismatches, 0);
        assert!(settle - RATE <= RATE / 10);
        if event == "worker_pause" {
            assert_eq!(e.status(), Status::Overloaded);
        }
    }
}

#[test]
fn peak_protection_keeps_hot_dc_input_unchanged() {
    let mut e = engine();
    let input: Vec<Stereo> = (0..RATE)
        .map(|n| {
            let x = (0.95 + 0.02 * (2.0 * PI * 6000.0 * n as f64 / RATE as f64).cos()) as f32;
            [x, x]
        })
        .collect();
    let (output, _) = process(&mut e, &input, true);
    let start = LATENCY + RATE / 4;
    let mismatches = (start..RATE)
        .filter(|n| output[*n].map(f32::to_bits) != input[*n - LATENCY].map(f32::to_bits))
        .count();
    println!(
        "hot_dc_protection_bit_mismatches={mismatches} state={:?}",
        e.status()
    );
    assert_eq!(mismatches, 0);
    assert_eq!(e.status(), Status::PeakProtected);
}

// Independent oracle: 32 phases and 96 taps, neither kernel nor estimator is
// borrowed from the implementation's 8-phase / 32-tap conservative bound.
fn true_peak(signal: &[Stereo], from: usize, count: usize) -> f64 {
    let mut peak = 0.0_f64;
    for phase in 0..32 {
        let mut kernel = [0.0; 96];
        for (i, h) in kernel.iter_mut().enumerate() {
            let x = i as f64 - 47.0 - phase as f64 / 32.0;
            let sinc = if x.abs() < 1e-12 {
                1.0
            } else {
                (PI * x).sin() / (PI * x)
            };
            let a = 2.0 * PI * i as f64 / 95.0;
            *h = sinc * (0.42 - 0.5 * a.cos() + 0.08 * (2.0 * a).cos());
        }
        let sum: f64 = kernel.iter().sum();
        for h in &mut kernel {
            *h /= sum;
        }
        for n in from..from + count {
            for ch in 0..2 {
                let value: f64 = kernel
                    .iter()
                    .enumerate()
                    .map(|(i, h)| h * signal[n + i - 47][ch] as f64)
                    .sum();
                peak = peak.max(value.abs());
            }
        }
    }
    peak
}

#[test]
fn independent_true_peak_oracle() {
    for (low, high, high_hz) in [
        (0.6, 0.25, 6000.0),
        (0.0, 0.8, 17000.0),
        (0.0, 0.93, 3000.0),
    ] {
        let mut e = engine();
        let input: Vec<Stereo> = (0..RATE * 2)
            .map(|n| {
                let t = n as f64 / RATE as f64;
                let x = (low * (2.0 * PI * 150.0 * t).sin()
                    + high * (2.0 * PI * high_hz * t + 0.123).sin()) as f32;
                [x, x * 0.375]
            })
            .collect();
        let (output, _) = process(&mut e, &input, true);
        let dry_peak = true_peak(&input, RATE, 1024);
        let wet_peak = true_peak(&output, RATE + LATENCY, 1024);
        let limit = 10_f64.powf(-1.0 / 20.0).max(dry_peak);
        println!(
            "true_peak high_hz={high_hz} input_dbtp={:.6} output_dbtp={:.6} excess_linear={:.9}",
            20.0 * dry_peak.log10(),
            20.0 * wet_peak.log10(),
            (wet_peak - limit).max(0.0)
        );
        assert!(wet_peak <= limit + 1e-5);
        if low > 0.0 {
            let from = LATENCY + RATE / 2;
            let error: Vec<_> = (from..from + RATE)
                .map(|i| [output[i][0] - input[i - LATENCY][0], 0.0])
                .collect();
            let low_db = 20.0 * (projection(&error, 150.0, 0) / low).max(1e-30).log10();
            println!("peak_protected_bass_150_hz_difference_relative_db={low_db:.6}");
            assert!(low_db <= -60.0);
        }
    }
}

struct DelayedMask {
    q: usize,
    history: std::collections::VecDeque<[f32; 4 * BINS]>,
}
impl SeparationModel for DelayedMask {
    fn lookahead(&self) -> usize {
        self.q
    }
    fn reset(&mut self) {
        self.history.clear();
    }
    fn infer(&mut self, x: &[f32; 4 * BINS]) -> Result<[f32; 4 * BINS], Status> {
        let mut mask = [0.0; 4 * BINS];
        for k in 0..BINS {
            let v = if x[k] > 0.0 { 0.9 } else { 0.1 };
            mask[k] = v;
            mask[3 * BINS + k] = 1.0 - v;
        }
        self.history.push_back(mask);
        Ok(if self.history.len() > self.q {
            self.history.pop_front().unwrap()
        } else {
            [0.25; 4 * BINS]
        })
    }
}
#[test]
fn lookahead_masks_align_with_their_original_spectra() {
    let input = tone(RATE, 3123.0, 0.05);
    let mut outputs = Vec::new();
    for q in [0, MAX_LOOKAHEAD] {
        let mut e = Engine::new(move || {
            Ok(Box::new(DelayedMask {
                q,
                history: Default::default(),
            }))
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        while e.model_status() == Status::Loading {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_micros(100));
        }
        let (output, _) = process(&mut e, &input, true);
        outputs.push(output);
    }
    let max_error = outputs[0][LATENCY + 8192..]
        .iter()
        .zip(&outputs[1][LATENCY + 8192..])
        .flat_map(|(a, b)| [(a[0] - b[0]).abs(), (a[1] - b[1]).abs()])
        .fold(0.0_f32, f32::max);
    println!("lookahead_q0_vs_q{MAX_LOOKAHEAD}_max_sample_error={max_error:e}");
    assert_eq!(max_error, 0.0);
}
