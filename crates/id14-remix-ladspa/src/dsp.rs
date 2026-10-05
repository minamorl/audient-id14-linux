use std::f64::consts::PI;
pub const FFT: usize = 1024;
pub const HOP: usize = 512;
pub const BINS: usize = 513;
pub const RATE: usize = 48_000;
#[cfg(feature = "comparison-legacy-guard")]
pub const LATENCY: usize = 3584;
#[cfg(not(feature = "comparison-legacy-guard"))]
pub const LATENCY: usize = 3776;
pub const SR_LATENCY: usize = 1024;
pub const MAX_LOOKAHEAD: usize = 3;
pub type Stereo = [f32; 2];

#[derive(Clone, Copy, Default, Debug)]
pub struct Complex {
    pub re: f64,
    pub im: f64,
}
impl Complex {
    fn mul(self, other: Self) -> Self {
        Self {
            re: self.re * other.re - self.im * other.im,
            im: self.re * other.im + self.im * other.re,
        }
    }
}

pub struct Fourier {
    roots: [Complex; FFT / 2],
    pub window: [f64; FFT],
}
impl Default for Fourier {
    fn default() -> Self {
        Self {
            roots: std::array::from_fn(|k| {
                let angle = -2.0 * PI * k as f64 / FFT as f64;
                Complex {
                    re: angle.cos(),
                    im: angle.sin(),
                }
            }),
            window: std::array::from_fn(|n| (PI * n as f64 / FFT as f64).sin()),
        }
    }
}
impl Fourier {
    pub fn transform(&self, data: &mut [Complex; FFT], inverse: bool) {
        for i in 0..FFT {
            let j = i.reverse_bits() >> (usize::BITS - 10);
            if i < j {
                data.swap(i, j);
            }
        }
        let mut size = 2;
        while size <= FFT {
            let half = size / 2;
            for start in (0..FFT).step_by(size) {
                for k in 0..half {
                    let mut root = self.roots[k * FFT / size];
                    if inverse {
                        root.im = -root.im;
                    }
                    let b = data[start + k + half].mul(root);
                    let a = data[start + k];
                    data[start + k] = Complex {
                        re: a.re + b.re,
                        im: a.im + b.im,
                    };
                    data[start + k + half] = Complex {
                        re: a.re - b.re,
                        im: a.im - b.im,
                    };
                }
            }
            size *= 2;
        }
        if inverse {
            for x in data {
                x.re /= FFT as f64;
                x.im /= FFT as f64;
            }
        }
    }
    pub fn analysis(&self, samples: &[Stereo; FFT]) -> [[Complex; FFT]; 2] {
        std::array::from_fn(|ch| {
            let mut spectrum = std::array::from_fn(|n| Complex {
                re: samples[n][ch] as f64 * self.window[n],
                im: 0.0,
            });
            self.transform(&mut spectrum, false);
            spectrum
        })
    }
    pub fn features(&self, spectrum: &[[Complex; FFT]; 2]) -> [f32; 4 * BINS] {
        std::array::from_fn(|i| {
            let channel = (i / BINS) / 2;
            let x = spectrum[channel][i % BINS];
            if (i / BINS) % 2 == 0 {
                x.re as f32
            } else {
                x.im as f32
            }
        })
    }
    /// Residual only: common real masks cannot rotate either channel's phase.
    pub fn residual(
        &self,
        spectrum: &[[Complex; FFT]; 2],
        mask: &[f32; 4 * BINS],
        db: [f32; 4],
    ) -> [Stereo; FFT] {
        self.normalized_residual(spectrum, mask, db, 1.0)
    }
    pub fn normalized_residual(
        &self,
        spectrum: &[[Complex; FFT]; 2],
        mask: &[f32; 4 * BINS],
        db: [f32; 4],
        normalization: f64,
    ) -> [Stereo; FFT] {
        if db == [0.0; 4] {
            return [[0.0; 2]; FFT];
        }
        let gains = db.map(|x| 10_f64.powf(x.clamp(-6.0, 6.0) as f64 / 20.0) - 1.0);
        let mut out = [[0.0_f32; 2]; FFT];
        for ch in 0..2 {
            let mut bins = [Complex::default(); FFT];
            for k in 7..BINS {
                let gain: f64 = (0..4)
                    .map(|part| gains[part] * mask[part * BINS + k] as f64)
                    .sum();
                // Normalize only the remixed high band; synthesize its difference
                // from the input. The unchanged dry signal is added later.
                let gain = normalization * (1.0 + gain) - 1.0;
                bins[k] = Complex {
                    re: spectrum[ch][k].re * gain,
                    im: spectrum[ch][k].im * gain,
                };
                if k != FFT / 2 {
                    bins[FFT - k] = Complex {
                        re: bins[k].re,
                        im: -bins[k].im,
                    };
                }
            }
            self.transform(&mut bins, true);
            for n in 0..FFT {
                out[n][ch] = (bins[n].re * self.window[n]) as f32;
            }
        }
        out
    }
}

pub fn valid_mask(mask: &[f32; 4 * BINS]) -> bool {
    (0..BINS).all(|k| {
        let mut sum = 0.0;
        for p in 0..4 {
            let m = mask[p * BINS + k];
            if !m.is_finite() || m < 0.0 {
                return false;
            }
            sum += m;
        }
        (sum - 1.0_f32).abs() <= 1e-4
    })
}

// Extra rejection of synthesis-window leakage below 300 Hz. The default
// equiripple guard has a 450 Hz passband; the old guard exists only for A/B tests.
#[cfg(feature = "comparison-legacy-guard")]
pub const FIR_TAPS: usize = 257;
#[cfg(not(feature = "comparison-legacy-guard"))]
pub const FIR_TAPS: usize = 1025;
pub const FIR_DELAY: usize = FIR_TAPS / 2;
pub struct BassGuard {
    taps: [f64; FIR_TAPS],
    history: [Stereo; FIR_TAPS],
    cursor: usize,
}
impl Default for BassGuard {
    fn default() -> Self {
        #[cfg(not(feature = "comparison-legacy-guard"))]
        let taps = crate::guard_coefficients::COEFFICIENTS;
        #[cfg(feature = "comparison-legacy-guard")]
        let taps = {
            let mut taps = std::array::from_fn(|i| {
                let n = i as f64 - FIR_DELAY as f64;
                let cutoff = 1100.0 / RATE as f64;
                let low = if n == 0.0 {
                    2.0 * cutoff
                } else {
                    (2.0 * PI * cutoff * n).sin() / (PI * n)
                };
                let angle = 2.0 * PI * i as f64 / (FIR_TAPS - 1) as f64;
                -low * (0.42 - 0.5 * angle.cos() + 0.08 * (2.0 * angle).cos())
            });
            let sum: f64 = taps.iter().sum();
            for t in &mut taps {
                *t /= -sum;
            }
            taps[FIR_DELAY] += 1.0;
            taps
        };
        Self {
            taps,
            history: [[0.0; 2]; FIR_TAPS],
            cursor: 0,
        }
    }
}
impl BassGuard {
    /// Zero-phase response used by the loudness predictor, only during setup.
    pub(crate) fn response_at_bin(&self, k: usize) -> f64 {
        let omega = 2.0 * PI * k as f64 / FFT as f64;
        self.taps
            .iter()
            .enumerate()
            .map(|(i, h)| h * (omega * (i as f64 - FIR_DELAY as f64)).cos())
            .sum()
    }
    pub fn sample(&mut self, input: Stereo) -> Stereo {
        self.history[self.cursor] = input;
        let mut result = [0.0_f64; 2];
        for i in 0..FIR_TAPS {
            let v = self.history[(self.cursor + FIR_TAPS - i) % FIR_TAPS];
            result[0] += v[0] as f64 * self.taps[i];
            result[1] += v[1] as f64 * self.taps[i];
        }
        self.cursor = (self.cursor + 1) % FIR_TAPS;
        result.map(|v| v as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fft_roundtrip_and_weighted_overlap() {
        let fft = Fourier::default();
        let mut a = std::array::from_fn(|n| Complex {
            re: (n as f64 * 0.37).sin(),
            im: 0.0,
        });
        let original = a;
        fft.transform(&mut a, false);
        fft.transform(&mut a, true);
        let err = a
            .iter()
            .zip(original)
            .map(|(x, y)| (x.re - y.re).abs())
            .fold(0.0_f64, f64::max);
        let cola = (0..HOP)
            .map(|n| (fft.window[n].powi(2) + fft.window[n + HOP].powi(2) - 1.0).abs())
            .fold(0.0_f64, f64::max);
        println!("fft_max_error={err:e} sqrt_hann_cola_error={cola:e}");
        assert!(err < 1e-12 && cola < 1e-12);
    }
    #[test]
    fn bass_guard_stopband() {
        let guard = BassGuard::default();
        let mut worst = 0.0_f64;
        for hz in 0..=300 {
            let mut re = 0.0;
            let mut im = 0.0;
            for (i, t) in guard.taps.iter().enumerate() {
                let a = 2.0 * PI * hz as f64 * i as f64 / RATE as f64;
                re += t * a.cos();
                im += t * a.sin();
            }
            worst = worst.max(re.hypot(im));
        }
        println!("bass_guard_0_300_hz_worst_db={}", 20.0 * worst.log10());
        assert!(worst < 0.001);
    }
}
