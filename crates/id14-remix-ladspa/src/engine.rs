use crate::dsp::*;
use crate::queue::{channel, Consumer, Producer};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

const RING: usize = 16384;
const SPECTRA: usize = 8;
const RENDER_LAG: u64 = 4;
const FADE: f32 = 480.0;
const PEAK_LOOK: usize = 128;
const PEAK_TAPS: usize = 32;
const PEAK_PHASES: usize = 8;
const CEILING: f64 = 0.889; // Margin below -1 dBTP for interpolation/roundoff.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Status {
    Loading = 0,
    Active = 1,
    Off = 2,
    ZeroAmounts = 3,
    ModelMissing = 4,
    ModelInvalid = 5,
    Overloaded = 6,
    PeakProtected = 7,
    UnsupportedRate = 8,
}
impl Status {
    fn from_u32(x: u32) -> Self {
        match x {
            0 => Self::Loading,
            1 => Self::Active,
            4 => Self::ModelMissing,
            8 => Self::UnsupportedRate,
            _ => Self::ModelInvalid,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Controls {
    pub enabled: bool,
    pub db: [f32; 4],
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            enabled: true,
            db: [3.0, 0.0, 0.0, -3.0],
        }
    }
}
impl Controls {
    fn sanitized(self) -> Self {
        Self {
            enabled: self.enabled,
            db: self.db.map(|x| {
                if x.is_finite() {
                    x.clamp(-6.0, 6.0)
                } else {
                    0.0
                }
            }),
        }
    }
    fn wet(self) -> bool {
        self.enabled && self.db.iter().any(|x| *x != 0.0)
    }
}

/// Implementations are exclusively owned by the inference thread.
pub trait SeparationModel: Send {
    fn lookahead(&self) -> usize;
    fn infer(&mut self, x: &[f32; 4 * BINS]) -> Result<[f32; 4 * BINS], Status>;
    fn reset(&mut self);
}
struct Features {
    seq: u64,
    x: [f32; 4 * BINS],
}
struct Mask {
    seq: u64,
    values: [f32; 4 * BINS],
}
struct Spectrum {
    seq: u64,
    data: [[Complex; FFT]; 2],
}
struct Shared {
    stop: AtomicBool,
    paused: AtomicBool,
    state: AtomicU32,
}

pub struct Engine {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
    tx: Producer<Features, 8>,
    rx: Consumer<Mask, 16>,
    fourier: Fourier,
    guard: BassGuard,
    input: Vec<Stereo>,
    residual: Vec<Stereo>,
    tags: Vec<u64>,
    ready: Vec<bool>,
    safe: Vec<f32>,
    safe_until: u64,
    local_safe: Vec<f32>,
    local_until: u64,
    spectra: Vec<Option<Spectrum>>,
    masks: Vec<Option<Mask>>,
    last_mask: [f32; 4 * BINS],
    overlap: [Stereo; HOP],
    kernel: [[f64; PEAK_TAPS]; PEAK_PHASES],
    clock: u64,
    wet: f32,
    peak: f32,
    controls: Controls,
    status: Status,
}

impl Engine {
    /// Loading/validation also run off the audio thread.
    pub fn new<F>(loader: F) -> Self
    where
        F: FnOnce() -> Result<Box<dyn SeparationModel>, Status> + Send + 'static,
    {
        let (tx, mut jobs) = channel::<Features, 8>();
        let (mut results, rx) = channel::<Mask, 16>();
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            state: AtomicU32::new(Status::Loading as u32),
        });
        let state = shared.clone();
        let worker = thread::Builder::new()
            .name("id14-remix-inference".into())
            .spawn(move || {
                let mut model = match loader() {
                    Ok(m) if m.lookahead() <= MAX_LOOKAHEAD => m,
                    Ok(_) => {
                        state
                            .state
                            .store(Status::ModelInvalid as u32, Ordering::Release);
                        return;
                    }
                    Err(e) => {
                        state.state.store(e as u32, Ordering::Release);
                        return;
                    }
                };
                let q = model.lookahead() as u64;
                state.state.store(Status::Active as u32, Ordering::Release);
                let mut previous = None;
                let mut since_reset = 0_u64;
                while !state.stop.load(Ordering::Acquire) {
                    if state.paused.load(Ordering::Acquire) {
                        thread::sleep(Duration::from_micros(200));
                        continue;
                    }
                    let Some(job) = jobs.pop() else {
                        thread::sleep(Duration::from_micros(100));
                        continue;
                    };
                    if previous.map(|p| p + 1) != Some(job.seq) {
                        model.reset();
                        since_reset = 0;
                    }
                    previous = Some(job.seq);
                    // Advance streaming state even through silence: with Q>0 this
                    // frame can supply a mask for earlier non-silent audio.
                    let values = match model.infer(&job.x) {
                        Ok(m) if valid_mask(&m) => m,
                        _ => {
                            state
                                .state
                                .store(Status::ModelInvalid as u32, Ordering::Release);
                            break;
                        }
                    };
                    if since_reset >= q {
                        // A full results queue is overload; never block either thread.
                        let _ = results.push(Mask {
                            seq: job.seq - q,
                            values,
                        });
                    }
                    since_reset += 1;
                }
            })
            .ok();
        if worker.is_none() {
            shared
                .state
                .store(Status::ModelInvalid as u32, Ordering::Release);
        }
        let kernel = std::array::from_fn(|phase| {
            let mut h = std::array::from_fn(|i| {
                let x = i as f64 - (PEAK_TAPS / 2 - 1) as f64 - phase as f64 / PEAK_PHASES as f64;
                let sinc = if x.abs() < 1e-12 {
                    1.0
                } else {
                    (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
                };
                let w = 0.42
                    - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / (PEAK_TAPS - 1) as f64).cos()
                    + 0.08 * (4.0 * std::f64::consts::PI * i as f64 / (PEAK_TAPS - 1) as f64).cos();
                sinc * w
            });
            let sum: f64 = h.iter().sum();
            for v in &mut h {
                *v /= sum;
            }
            h
        });
        Self {
            shared,
            worker,
            tx,
            rx,
            fourier: Fourier::default(),
            guard: BassGuard::default(),
            input: vec![[0.0; 2]; RING],
            residual: vec![[0.0; 2]; RING],
            tags: vec![u64::MAX; RING],
            ready: vec![false; RING],
            safe: vec![1.0; RING],
            safe_until: 0,
            local_safe: vec![1.0; RING],
            local_until: 0,
            spectra: (0..SPECTRA).map(|_| None).collect(),
            masks: (0..16).map(|_| None).collect(),
            last_mask: [0.25; 4 * BINS],
            overlap: [[0.0; 2]; HOP],
            kernel,
            clock: 0,
            wet: 0.0,
            peak: 0.0,
            controls: Controls::default(),
            status: Status::Loading,
        }
    }

    pub fn set_controls(&mut self, controls: Controls) {
        self.controls = controls.sanitized();
    }
    pub fn controls(&self) -> Controls {
        self.controls
    }
    pub fn status(&self) -> Status {
        self.status
    }
    pub fn model_status(&self) -> Status {
        Status::from_u32(self.shared.state.load(Ordering::Acquire))
    }
    pub fn latency_frames(&self) -> usize {
        LATENCY
    }
    /// Diagnostic injection for the verification harness; no LADSPA control exposes this.
    pub fn pause_worker(&self, pause: bool) {
        self.shared.paused.store(pause, Ordering::Release);
    }
    /// Join only from the host's non-audio diagnostic/cleanup thread.
    pub fn stop_worker(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }

    fn dry_at(&self, output: i64) -> Stereo {
        if output < LATENCY as i64 {
            return [0.0; 2];
        }
        self.input[(output as usize - LATENCY) % RING]
    }
    fn correction_at(&self, output: i64) -> Stereo {
        if output < 0 || self.tags[output as usize % RING] != output as u64 {
            return [0.0; 2];
        }
        self.residual[output as usize % RING]
    }
    // Triangle-inequality bound: every contributing correction sample is constrained
    // by this value, so envelopes need not be constant over the interpolation kernel.
    fn peak_bound(&self, center: u64) -> f32 {
        let mut bound = 1.0_f64;
        for phase in 0..PEAK_PHASES {
            let mut base = [0.0_f64; 2];
            let mut delta = [0.0_f64; 2];
            for i in 0..PEAK_TAPS {
                let n = center as i64 + i as i64 - (PEAK_TAPS / 2 - 1) as i64;
                let dry = self.dry_at(n);
                let residual = self.correction_at(n);
                for ch in 0..2 {
                    base[ch] += dry[ch] as f64 * self.kernel[phase][i];
                    delta[ch] += (residual[ch] as f64 * self.kernel[phase][i]).abs();
                }
            }
            for ch in 0..2 {
                if !base[ch].is_finite() || !delta[ch].is_finite() {
                    return 0.0;
                }
                if delta[ch] > 0.0 {
                    bound = bound.min(((CEILING - base[ch].abs()) / delta[ch]).clamp(0.0, 1.0));
                }
            }
        }
        bound as f32
    }

    fn hop(&mut self) {
        let seq = self.clock / HOP as u64;
        while let Some(mask) = self.rx.pop() {
            let slot = mask.seq as usize % self.masks.len();
            self.masks[slot] = Some(mask);
        }
        if self.model_status() != Status::Active {
            return;
        }
        let samples = std::array::from_fn(|i| {
            let absolute = self.clock as i64 + 1 - FFT as i64 + i as i64;
            if absolute < 0 {
                [0.0; 2]
            } else {
                self.input[absolute as usize % RING]
            }
        });
        let data = self.fourier.analysis(&samples);
        let x = self.fourier.features(&data);
        if x.iter().all(|v| v.is_finite()) {
            let _ = self.tx.push(Features { seq, x });
        }
        self.spectra[seq as usize % SPECTRA] = Some(Spectrum { seq, data });
        if seq < RENDER_LAG {
            return;
        }
        let target = seq - RENDER_LAG;
        let Some(spectrum) = self.spectra[target as usize % SPECTRA].as_ref() else {
            return;
        };
        if spectrum.seq != target {
            return;
        }
        let slot = target as usize % self.masks.len();
        let valid = self.masks[slot]
            .as_ref()
            .is_some_and(|mask| mask.seq == target);
        if valid {
            self.last_mask = self.masks[slot].take().unwrap().values;
        }
        // When a mask is missing, retain its spectral shape only during the dry fade.
        // This synthesizes current audio; old audio samples are never repeated.
        let transformed = self
            .fourier
            .residual(&spectrum.data, &self.last_mask, self.controls.db);
        let start = (target as i64 - 1) * HOP as i64 + LATENCY as i64 - FIR_DELAY as i64;
        for n in 0..HOP {
            let value = std::array::from_fn(|ch| transformed[n][ch] + self.overlap[n][ch]);
            self.overlap[n] = transformed[n + HOP];
            let filtered = self.guard.sample(value);
            let position = (start + n as i64) as u64;
            let index = position as usize % RING;
            self.residual[index] = filtered;
            self.tags[index] = position;
            self.ready[index] = valid;
        }
        let until = (start + HOP as i64 - PEAK_TAPS as i64) as u64;
        // A slow initial model load may leave minutes of already-played dry audio.
        // Only the live interpolation neighborhood needs bounds; never replay that
        // history as a burst of work in the callback when the model becomes ready.
        self.safe_until = self
            .safe_until
            .max(self.clock.saturating_sub((2 * PEAK_TAPS) as u64));
        self.local_until = self
            .local_until
            .max(self.clock.saturating_sub(PEAK_TAPS as u64));
        while self.safe_until < until {
            self.safe[self.safe_until as usize % RING] = self.peak_bound(self.safe_until);
            self.safe_until += 1;
        }
        while self.local_until + (PEAK_TAPS as u64) < self.safe_until {
            let mut bound = 1.0_f32;
            for neighbor in -(PEAK_TAPS as i64)..=PEAK_TAPS as i64 {
                let n = self.local_until as i64 + neighbor;
                if n >= 0 {
                    bound = bound.min(self.safe[n as usize % RING]);
                }
            }
            self.local_safe[self.local_until as usize % RING] = bound;
            self.local_until += 1;
        }
    }

    pub fn sample(&mut self, input: Stereo) -> Stereo {
        self.input[self.clock as usize % RING] = input;
        if (self.clock + 1) % HOP as u64 == 0 {
            self.hop();
        }
        let index = self.clock as usize % RING;
        let model = self.model_status();
        let available =
            self.tags[index] == self.clock && self.ready[index] && model == Status::Active;
        let target = if self.controls.wet() && available {
            1.0
        } else {
            0.0
        };
        self.wet += (target - self.wet).clamp(-1.0 / FADE, 1.0 / FADE);
        // Lipschitz lookahead: a newly visible limit cannot cause a discontinuous
        // attack because it enters at distance PEAK_LOOK, where the ramp allows 1.
        let mut allowed = 1.0_f32;
        if self.wet > 0.0 {
            for offset in 0..=PEAK_LOOK {
                let center = self.clock + offset as u64;
                let local = if center < self.local_until {
                    self.local_safe[center as usize % RING]
                } else {
                    1.0
                };
                allowed = allowed.min(local + offset as f32 / PEAK_LOOK as f32);
            }
        }
        self.peak = (self.peak + 1.0 / PEAK_LOOK as f32).min(allowed);
        let dry = self.dry_at(self.clock as i64);
        let delta = self.correction_at(self.clock as i64);
        let amount = self.wet * self.peak;
        let output = if amount == 0.0 || delta == [0.0; 2] {
            dry
        } else {
            std::array::from_fn(|ch| dry[ch] + delta[ch] * amount)
        };
        self.status = if !self.controls.enabled {
            Status::Off
        } else if !self.controls.wet() {
            Status::ZeroAmounts
        } else if model != Status::Active {
            model
        } else if self.clock < LATENCY as u64 {
            Status::Loading
        } else if !available {
            Status::Overloaded
        } else if self.peak < 0.999 {
            Status::PeakProtected
        } else {
            Status::Active
        };
        self.clock += 1;
        output
    }
    pub fn process(&mut self, input: &[Stereo], output: &mut [Stereo]) {
        assert_eq!(input.len(), output.len());
        for (x, y) in input.iter().zip(output) {
            *y = self.sample(*x);
        }
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
